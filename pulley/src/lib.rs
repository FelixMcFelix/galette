pub mod config;
pub mod error;
#[cfg(unix)]
pub mod prog;

#[cfg(unix)]
use std::{
	collections::HashMap,
	os::unix::io::{AsRawFd, RawFd},
};
use std::{io::Error as IoError, path::PathBuf, sync::Arc};

use config::Cli;
#[cfg(unix)]
use dlopen2::wrapper::{Container, WrapperApi};
use error::*;
use futures_util::{SinkExt, StreamExt};
#[cfg(unix)]
use libbpf_rs::{MapFlags, ObjectBuilder};
#[cfg(unix)]
use nf::{Map as NfMapTrait, RawMap};
#[cfg(unix)]
use prog::Prog;
use protocol::{Chain, ClientToServer, CrpServerTlsVerifier, ServerToClient};
#[cfg(unix)]
use protocol::{LinkAction, XdpLinkState};
use tokio_rustls::rustls::{client::WebPkiVerifier, Certificate, PrivateKey};
#[cfg(unix)]
use uuid::Uuid;
#[cfg(unix)]
use xsk_rs::{
	config::{BindFlags, LibbpfFlags, SocketConfig, UmemConfig},
	socket::Socket,
	umem::Umem,
	CompQueue,
	FillQueue,
	FrameDesc,
	RxQueue,
	TxQueue,
};

pub async fn get_chain(server: &str) -> Result<Chain, ChainGetError> {
	let mut trust = tokio_rustls::rustls::RootCertStore { roots: vec![] };
	trust
		.add_parsable_certificates(&[include_bytes!("../../certs/server/certs/cert.der").to_vec()]);

	let crps = if let Ok(data) = tokio::fs::read("testcrps.post").await {
		postcard::from_bytes(&data).unwrap()
	} else {
		eprintln!("Warning! Using random CRP store, not pre-shared!");

		protocol::KeySource::new_random()
	};

	let verifier = CrpServerTlsVerifier {
		base: Arc::new(WebPkiVerifier::new(trust, None)),
		crps,
	};

	let cfg = tokio_rustls::rustls::ClientConfig::builder()
		.with_safe_defaults()
		.with_custom_certificate_verifier(Arc::new(verifier))
		// .with_client_cert_resolver(client_auth_cert_resolver)
		.with_single_cert(
			vec![Certificate(
				include_bytes!("../../certs/client/certs/cert.der").to_vec(),
			)],
			PrivateKey(include_bytes!("../../certs/client/certs/key.der").to_vec()),
		)
		.expect("Failed to create own trust chain.");

	let connector = tokio_tungstenite::Connector::Rustls(cfg.into());

	println!("Connecting to: {server}");
	let (mut ws_stream, _) =
		tokio_tungstenite::connect_async_tls_with_config(server, None, Some(connector))
			.await
			.map_err(ChainGetError::Connect)?;

	// println!("{:?}", ws_stream);

	ws_stream
		.send(protocol::ser(&ClientToServer::RequestChain(
			env!("TARGET").into(),
		)))
		.await
		.map_err(ChainGetError::SendRequest)?;

	loop {
		match ws_stream.next().await {
			Some(Ok(msg)) =>
				match protocol::deser::<ServerToClient>(&msg).map_err(ChainGetError::Deserialize)? {
					Some(ServerToClient::Chain(v)) => return Ok(v),
					Some(ServerToClient::RequestChainError(err)) =>
						return Err(ChainGetError::ServerError(err)),
					_ => {},
				},
			Some(Err(msg)) => {
				return Err(ChainGetError::WsRecv(msg));
			},
			None => {
				return Err(ChainGetError::SessionClosed);
			},
		}
	}
}

#[cfg(not(unix))]
pub type XskFds = ();
#[cfg(unix)]
pub type XskFds = Vec<XskData>;

#[cfg(unix)]
pub struct XskData {
	pub fd: RawFd,
	pub tx: TxQueue,
	pub rx: RxQueue,
	pub frames: Vec<FrameDesc>,
	pub umem: Umem,
	pub mediate: Option<UmemMediate>,
}

#[cfg(unix)]
pub struct UmemMediate {
	pub fq: FillQueue,
	pub cq: CompQueue,
}

#[cfg(not(unix))]
pub fn create_upcall_sockets(_config: &Cli) -> XskFds {
	eprintln!("Windows: no binaries loaded!");
}

#[cfg(unix)]
pub fn create_upcall_sockets(config: &Cli) -> XskFds {
	let mut skt_cfg = SocketConfig::builder();
	skt_cfg.libbpf_flags(LibbpfFlags::XSK_LIBBPF_FLAGS_INHIBIT_PROG_LOAD);
	skt_cfg.bind_flags(BindFlags::XDP_USE_NEED_WAKEUP);
	// no pref over (zero)copy mode, or native/drv.
	let skt_cfg = skt_cfg.build();
	let mut umem_cfg = UmemConfig::builder();
	umem_cfg.frame_headroom(8);

	let mut shared_umem = None;

	let mut out = vec![];

	for i in 0..config.xdp_cores.unwrap() {
		let (umem, descs) = if (!config.share_umem) || i == 0 {
			let (umem, descs) =
				Umem::new(umem_cfg.build().unwrap(), 2048.try_into().unwrap(), false)
					.expect("failed to create UMEM");

			if config.share_umem {
				shared_umem = Some((umem.clone(), descs.clone()));
			}

			(umem, descs)
		} else {
			shared_umem.clone().unwrap()
		};

		let (tx_q, rx_q, maybe_fq_and_cq) =
			Socket::new(skt_cfg, &umem, &config.interface[0].parse().unwrap(), 0)
				.expect("failed to create dev2 socket");

		let mediate = if let Some((mut fq, cq)) = maybe_fq_and_cq {
			unsafe {
				fq.produce(&descs);
			}

			Some(UmemMediate { fq, cq })
		} else {
			None
		};

		let xsk_fd = tx_q.fd().as_raw_fd();

		out.push(XskData {
			fd: xsk_fd,
			tx: tx_q,
			rx: rx_q,
			frames: descs,
			umem,
			mediate,
		})
	}

	out
}

#[cfg(not(unix))]
pub fn install_chain(
	_chain: &Chain,
	_config: &Cli,
	xsk_fd: &XskFds,
) -> Result<ChainState, ChainInstallError> {
	eprintln!("Windows: no binaries loaded!");

	Ok(ChainState {})
}

#[cfg(unix)]
pub fn install_chain(
	chain: &Chain,
	config: &Cli,
	xsks: &XskFds,
) -> Result<ChainState, ChainInstallError> {
	// TODO: allow multiple rx + tx.
	let iface_name = config.interface[0].clone();

	// todo: one uuid -> vec index?
	let mut linked_ebpfs = HashMap::new();
	let mut prog_fds = HashMap::new();
	let mut instance_ids = HashMap::new();
	let mut link_states = HashMap::new();
	let mut raw_maps = HashMap::new();

	let iface = nix::net::if_::if_nametoindex(iface_name.as_str())
		.map_err(|e| ChainInstallError::IfaceLookup(iface_name.clone(), e))?;

	// AF_XDP handling
	// Load prog code for all files in chain.
	let mut root_idx = None;
	for chain_link in &chain.links {
		println!("{chain_link:?}");
		let prog_data = chain
			.nfs
			.get(&chain_link.uuid)
			.ok_or(ChainInstallError::MissingNf(chain_link.uuid))?;

		if chain_link.root {
			root_idx = Some(chain_link.uuid);
		}

		let ebpf_elfs = prog_data.ebpf.as_ref();
		// .ok_or(ChainInstallError::MissingEbpfPayload(chain_link.uuid))?;

		let ebpf_elfs = if let Some(a) = ebpf_elfs { a } else { continue };

		let my_prog = match &chain_link.state {
			XdpLinkState::Tail => &ebpf_elfs.end,
			XdpLinkState::Body(_) => &ebpf_elfs.link,
		};
		let obj = ObjectBuilder::default()
			.open_memory("outer_xdp_sock_prog", my_prog)
			.map_err(|_| ChainInstallError::MissingEbpfEntry(chain_link.uuid))?;

		let load_obj = obj.load().unwrap();
		let fd = load_obj
			.prog("outer_xdp_sock_prog")
			.expect("Exists due to above open_memory.")
			.fd();

		prog_fds.insert(chain_link.uuid, fd);

		let mut my_maps = vec![];

		for name in chain_link.map_names.iter() {
			let code_name = name.to_ascii_uppercase();
			my_maps.push(unsafe {
				RawMap::new(
					load_obj
						.map(&code_name)
						.ok_or(ChainInstallError::MissingMap(chain_link.uuid, code_name))?,
				)
			});
		}

		raw_maps.insert(chain_link.uuid, my_maps);

		linked_ebpfs.insert(chain_link.uuid, Prog::Unlinked(load_obj));
	}

	// Build and patch prog maps to include the right jumps
	//for (uuid, prog) in linked_ebpfs.iter() {
	for (uuid, prog) in linked_ebpfs.iter_mut() {
		// let object = prog.object();
		let object = prog.object_mut();
		for map in object.maps_iter() {
			println!(
				"{} -- {}, {}, {}B per entry",
				uuid,
				map.name(),
				map.map_type(),
				map.value_size()
			);
		}

		// FIXME: left in as test code.
		if let Some(map) = object.map_mut("BLOCKED_IPS") {
			map.update(&[192, 168, 0, 69], &[1], MapFlags::ANY).unwrap();
		}

		if let Some(map) = object.map_mut("UPCALL_LIKELIHOOD") {
			let likelihood = ((config.loadbalance_chance * (u32::MAX as f64)) as u32).to_ne_bytes();
			map.update(&[0, 0, 0, 0], &likelihood, MapFlags::ANY)
				.unwrap();
		}
	}

	for (id, chain_link) in chain.links.iter().enumerate() {
		link_states.insert(chain_link.uuid, chain_link.state.clone());

		// Insert ID state for each program into it's eBPF maps, as needed.
		if let XdpLinkState::Body(els) = &chain_link.state {
			let prog = linked_ebpfs.get_mut(&chain_link.uuid);

			let prog = if let Some(a) = prog { a } else { continue };

			let obj = prog.object_mut();

			// TODO: better ID assignment when I add in support for duplicate NFs?
			let mut id_map = obj
				.map_mut("my_state_map")
				.ok_or(ChainInstallError::MissingMap(
					chain_link.uuid,
					"my_state_map".into(),
				))?;

			let state = DataplaneState {
				prog_id: id as u32,
				num_cores: config.xdp_cores.unwrap(),
			};

			(&mut id_map).put(&0, &state);
			// .update(
			// 	&0_u32.to_le_bytes(),
			// 	&(id as u32).to_le_bytes(),
			// 	MapFlags::ANY,
			// )
			// .map_err(|e| {
			// 	ChainInstallError::MapUpdateFail(chain_link.uuid, "my_id_map".into(), e)
			// })?;

			instance_ids.insert(id as u32, chain_link.uuid);

			// TODO: bind and give XSK to all.
			let xsk_map = obj.map_mut("xsk_map").ok_or(ChainInstallError::MissingMap(
				chain_link.uuid,
				"xsk_map".into(),
			))?;

			for (xsk_i, xsk) in xsks.iter().enumerate() {
				xsk_map
					.update(
						&(xsk_i as u32).to_le_bytes(),
						&(xsk.fd).to_le_bytes(),
						MapFlags::ANY,
					)
					.map_err(|e| {
						ChainInstallError::MapUpdateFail(
							chain_link.uuid,
							format!("xsk_map[{xsk_i}]"),
							e,
						)
					})?;
			}

			for (i, action) in els.iter().enumerate() {
				// None case means special case: the server program should have caught this
				// with a Tx, Drop, etc.

				let acts = obj
					.map_mut("acts_map")
					.ok_or(ChainInstallError::MissingMap(
						chain_link.uuid,
						"acts_map".into(),
					))?;

				acts.update(
					&(i as u32).to_le_bytes(),
					&action.to_kind().to_le_bytes(),
					MapFlags::ANY,
				)
				.map_err(|e| {
					ChainInstallError::MapUpdateFail(chain_link.uuid, "acts_map".into(), e)
				})?;

				// FIXME: now match on 'uuid'
				let tailcall_id = match action {
					LinkAction::Tailcall(uuid) => Some(uuid),
					_ => None,
				};

				if let Some(uuid) = tailcall_id {
					let fd = prog_fds
						.get(uuid)
						.ok_or(ChainInstallError::BadNfLink(chain_link.uuid, *uuid))?;

					let progmap = obj
						.map_mut("progs_map")
						.ok_or(ChainInstallError::MissingMap(
							chain_link.uuid,
							"progs_map".into(),
						))?;

					progmap
						.update(&(i as u32).to_le_bytes(), &fd.to_le_bytes(), MapFlags::ANY)
						.map_err(|e| {
							ChainInstallError::MapUpdateFail(chain_link.uuid, "progs_map".into(), e)
						})?;
				}
			}
		}
	}

	// Last step: link the true prog to the iface?
	let root_idx = root_idx.ok_or(ChainInstallError::MissingRootNf)?;
	if let Some(Prog::Unlinked(mut obj)) = linked_ebpfs.remove(&root_idx) {
		let link = obj
			.prog_mut("outer_xdp_sock_prog")
			.expect("Already verified presence of this program.")
			.attach_xdp(iface as i32)
			.unwrap();

		linked_ebpfs.insert(root_idx, Prog::Linked(obj, link));
	} else {
		unreachable!()
	}

	eprintln!("Chain linked and loaded -- packet mods should occur!");

	Ok(ChainState {
		linked_ebpfs,
		prog_fds,
		instance_ids,
		link_states,
		raw_maps,
	})
}

pub type ProgId = u32;

#[repr(C)]
struct DataplaneState {
	prog_id: ProgId,
	num_cores: u32,
}

#[cfg(unix)]
#[derive(WrapperApi)]
pub struct NfUserApi {
	user_nf_program: fn(pkt: &mut [u8], maps: &mut [RawMap]) -> usize,
}

pub struct ChainState {
	#[cfg(unix)]
	pub linked_ebpfs: HashMap<Uuid, Prog>,
	#[cfg(unix)]
	pub prog_fds: HashMap<Uuid, i32>,
	#[cfg(unix)]
	pub instance_ids: HashMap<ProgId, Uuid>,
	#[cfg(unix)]
	pub link_states: HashMap<Uuid, XdpLinkState>,
	#[cfg(unix)]
	pub raw_maps: HashMap<Uuid, Vec<RawMap>>,
}

#[cfg(unix)]
pub type MapHaxType = HashMap<Uuid, Vec<RawMap>>;

unsafe impl Send for ChainState {}
unsafe impl Sync for ChainState {}

pub struct DylibStore {
	#[cfg(unix)]
	pub dylibs: HashMap<Uuid, Container<NfUserApi>>,
	pub temp_path: PathBuf,
}

impl DylibStore {
	pub async fn new() -> Result<Self, IoError> {
		let temp_path =
			tokio::task::spawn_blocking(|| tempfile::tempdir().map(|a| a.into_path())).await??;

		Ok(Self {
			#[cfg(unix)]
			dylibs: HashMap::new(),
			temp_path,
		})
	}

	pub async fn cleanup(&self) -> Result<(), IoError> {
		tokio::fs::remove_dir_all(&self.temp_path).await
	}

	#[cfg(not(unix))]
	pub async fn load_dylib_nfs(&mut self, chain: &Chain) -> Result<(), IoError> {
		eprintln!("WARNING [Windows]: no dylibs loaded");

		Ok(())
	}

	#[cfg(unix)]
	pub async fn load_dylib_nfs(&mut self, chain: &Chain) -> Result<(), IoError> {
		let elf_path = tempfile::tempdir()?.into_path();

		for (uuid, nf) in &chain.nfs {
			if let Some(elf) = &nf.elf {
				let fs_path = elf_path.join(format!("{uuid}"));
				tokio::fs::write(&fs_path, elf).await?;

				let dll: Container<NfUserApi> = unsafe { Container::load(fs_path).unwrap() };

				self.dylibs.insert(*uuid, dll);
			}
		}

		Ok(())
	}
}
