pub mod chain;
pub mod config;
pub mod error;

use std::{collections::HashMap, sync::Arc};

use chain::Chain;
use config::Cli;
use protocol::{Chain as PChain, Function, ServerToClient, XdpLink};
use tokio::fs;
use uuid::Uuid;

pub async fn compile_chain(config: &Cli) -> anyhow::Result<ChainData> {
	const TMP_DIR: &str = "tmp";
	const XDP_DIR: &str = "xdp";
	const USR_DIR: &str = "user";
	let mut base_dir = fs::canonicalize(config.path.clone()).await?;
	// base_binaries_dir.push(TMP_DIR);
	base_dir.push("chain.toml");

	let config_bytes = fs::read(&base_dir).await?;
	let chain: Chain = toml::from_slice(&config_bytes)?;

	base_dir.pop();

	// Remove old temp data.
	let mut tmp_dir = base_dir.clone();
	tmp_dir.push(TMP_DIR);
	let _ = fs::remove_dir_all(&tmp_dir).await;
	let _ = fs::create_dir(&tmp_dir).await;

	// Analyse chain + and find output variants for packet processing NFs.
	let nf_return_types = chain.get_nf_return_types(base_dir.clone()).await?;

	// --- XDP ---
	// Create XDP variants.
	let mut xdp_dir = tmp_dir.clone();
	xdp_dir.push(XDP_DIR);
	let _ = fs::create_dir(&xdp_dir).await;
	chain.generate_xdp_cargo_toml(xdp_dir.clone()).await?;

	// Create base directory, lib.rs.
	let mut src_path = xdp_dir.clone();
	src_path.push("src");
	fs::create_dir(&src_path).await?;

	src_path.push("lib.rs");
	fs::write(&src_path, b"#![no_std]").await?;
	src_path.pop();

	chain
		.write_xdp_programs(&nf_return_types, &mut src_path)
		.await?;

	let (mut binaries, mut name_to_uuid) = chain
		.compile_xdp_binaries(src_path, &config.vmlinux)
		.await?;
	eprintln!("Built eBPF binaries.");

	// --- XDP ---

	// --- USER ---
	let mut usr_dir = tmp_dir.clone();
	usr_dir.push(USR_DIR);
	let _ = fs::create_dir(&usr_dir).await;

	chain.generate_userland_cargo_toml(usr_dir.clone()).await?;
	chain
		.write_userland_programs(&nf_return_types, &mut usr_dir)
		.await?;
	chain
		.compile_userland_binaries(usr_dir, &mut binaries, &mut name_to_uuid, &config.target)
		.await?;

	// --- USER ---

	dbg!(&name_to_uuid);
	let links = chain.make_concrete(&name_to_uuid)?;
	dbg!(&links);

	Ok(ChainData {
		binaries,
		name_to_uuid,
		links,
	})
}

pub struct ChainData {
	pub binaries: HashMap<Uuid, Function>,
	pub name_to_uuid: HashMap<String, Uuid>,
	pub links: Vec<XdpLink>,
}

impl ChainData {
	pub fn into_single_message(self) -> Arc<ServerToClient> {
		Arc::new(ServerToClient::Chain(PChain {
			links: self.links,
			nfs: self.binaries,
		}))
	}
}
