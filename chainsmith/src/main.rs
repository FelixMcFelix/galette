use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use chainsmith::config::{Cli, TlsMode};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use protocol::*;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{
	rustls::{
		server::AllowAnyAuthenticatedClient, //ClientCertVerifier,
		Certificate,
		PrivateKey,
		ServerConfig,
	},
	TlsAcceptor,
};

static SUPPORTED_ARCHES: phf::Map<&'static str, Option<&'static str>> = phf::phf_map! {
	"x86_64-unknown-linux-gnu" => None,
	"aarch64-unknown-linux-gnu" => Some("support-files/vmlinux"),
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let config = Cli::parse();

	let mut chain_datas = HashMap::with_capacity(SUPPORTED_ARCHES.len());
	for (target, vmlinux) in &SUPPORTED_ARCHES {
		eprintln!("--- Preparing target {target}");

		let mut l_config = config.clone();
		l_config.vmlinux = vmlinux.map(|v| v.to_string());
		l_config.target = Some(target.to_string());

		let chain_data = chainsmith::compile_chain(&l_config)
			.await?
			.into_single_message();

		chain_datas.insert(*target, chain_data);
	}
	let chain_datas = Arc::new(chain_datas);

	// start simple WS server or something.
	let socket = TcpListener::bind(config.conn_string)
		.await
		.expect("Failed to bind server?");

	if config.tls_mode.is_tls_used() {
		let mut crp_dat = if config.tls_mode.generate_crps() {
			println!("Building CRP list...");

			let crps = protocol::KeySource::new_random();

			let test_crpstore_ser = postcard::to_stdvec(&crps)?;
			tokio::fs::write("testcrps.post", test_crpstore_ser).await?;

			let custom_cert = crps.gen_cert();

			let cert_der = custom_cert.serialize_der().unwrap();
			let key_der = custom_cert.serialize_private_key_der();

			Some((Some(crps), Some((cert_der, key_der))))
		} else {
			None
		};

		let mut trust = tokio_rustls::rustls::RootCertStore { roots: vec![] };
		trust.add_parsable_certificates(&[
			include_bytes!("../../certs/client/certs/cert.der").to_vec()
		]);

		let part_tls_config = ServerConfig::builder().with_safe_defaults();

		let csd_tls_config = match config.tls_mode {
			TlsMode::ClientServerAuth =>
				part_tls_config.with_client_cert_verifier(AllowAnyAuthenticatedClient::new(trust)),
			TlsMode::PufTls => {
				let crps = crp_dat.as_mut().unwrap().0.take().unwrap();

				let verifier = CrpClientTlsVerifier {
					base: AllowAnyAuthenticatedClient::new(trust),
					crps,
				};

				part_tls_config.with_client_cert_verifier(Arc::new(verifier))
			},
			_ => part_tls_config.with_no_client_auth(),
		};

		let tls_config = if config.tls_mode.generate_crps() {
			println!("Own cert: CRP'd cert.");
			let (cert_der, key_der) = crp_dat.as_mut().unwrap().1.take().unwrap();

			csd_tls_config.with_single_cert(vec![Certificate(cert_der)], PrivateKey(key_der))?
		} else {
			println!("Own cert: pre-shared.");

			csd_tls_config.with_single_cert(
				vec![Certificate(
					include_bytes!("../../certs/server/certs/cert.der").to_vec(),
				)],
				PrivateKey(include_bytes!("../../certs/server/certs/key.der").to_vec()),
			)?
		};

		let tls_config = Arc::new(tls_config);

		while let Ok((stream, addr)) = socket.accept().await {
			println!("New TLS Conn.");
			tokio::spawn(handle_connection(
				stream,
				addr,
				chain_datas.clone(),
				tls_config.clone(),
			));
		}
	} else {
		while let Ok((stream, addr)) = socket.accept().await {
			println!("New Non-TLS Conn.");
			tokio::spawn(handle_connection_no_tls(stream, addr, chain_datas.clone()));
		}
	}

	Ok(())
}

async fn handle_connection(
	raw_stream: TcpStream,
	addr: SocketAddr,
	c_dat: Arc<HashMap<&str, Arc<ServerToClient>>>,
	tls: Arc<ServerConfig>,
) {
	let tls = TlsAcceptor::from(tls);
	let stream = tls.accept(raw_stream).await;

	if let Err(e) = &stream {
		println!("{e:?} {e}");
	}

	let stream = stream.expect("Failed TLS handshake!");
	// let stream = raw_stream;

	let ws_stream = tokio_tungstenite::accept_async(stream)
		.await
		.unwrap_or_else(|_| panic!("Failed WS handshake with {addr}!"));

	let (mut ws_tx, mut ws_rx) = ws_stream.split();

	while let Some(Ok(msg)) = ws_rx.next().await {
		// msg \in tungstenite::protocol::Message
		match protocol::deser::<ClientToServer>(&msg) {
			Ok(Some(ClientToServer::RequestChain(target))) => {
				let msg = if let Some(c_dat) = c_dat.get(target.as_str()) {
					ser(&**c_dat)
				} else {
					ser(&ServerToClient::RequestChainError(format!(
						"Could not fetch user chains: target {target} unsupported."
					)))
				};

				ws_tx.send(msg).await.expect("Connection died?");
			},
			Ok(None) => {},
			Err(e) => {
				eprintln!("Error decoding message from {addr}: {e:?}");
				break;
			},
		}
	}
}

async fn handle_connection_no_tls(
	stream: TcpStream,
	addr: SocketAddr,
	c_dat: Arc<HashMap<&str, Arc<ServerToClient>>>,
) {
	let ws_stream = tokio_tungstenite::accept_async(stream)
		.await
		.unwrap_or_else(|_| panic!("Failed WS handshake with {addr}!"));

	let (mut ws_tx, mut ws_rx) = ws_stream.split();

	while let Some(Ok(msg)) = ws_rx.next().await {
		// msg \in tungstenite::protocol::Message
		match protocol::deser::<ClientToServer>(&msg) {
			Ok(Some(ClientToServer::RequestChain(target))) => {
				let msg = if let Some(c_dat) = c_dat.get(target.as_str()) {
					ser(&**c_dat)
				} else {
					ser(&ServerToClient::RequestChainError(format!(
						"Could not fetch user chains: target {target} unsupported."
					)))
				};

				ws_tx.send(msg).await.expect("Connection died?");
			},
			Ok(None) => {},
			Err(e) => {
				eprintln!("Error decoding message from {addr}: {e:?}");
				break;
			},
		}
	}
}
