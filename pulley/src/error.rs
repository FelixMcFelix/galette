#[cfg(unix)]
use libbpf_rs::Error as BpfError;
#[cfg(unix)]
use nix::errno::Errno;
use protocol::DeserError;
use thiserror::Error;
use tokio_tungstenite::tungstenite::Error as WsError;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ChainGetError {
	#[error("failed to connect to server")]
	Connect(#[source] WsError),
	#[error("failed to send request to server")]
	SendRequest(#[source] WsError),
	#[error("error while receiving message from server")]
	WsRecv(#[source] WsError),
	#[error("message from server could not be deserialised")]
	Deserialize(#[source] DeserError),
	#[error("server could not fetch chain: {0}")]
	ServerError(String),
	#[error("server closed session prematurely")]
	SessionClosed,
}

#[derive(Debug, Error)]
pub enum ChainInstallError {
	#[cfg(unix)]
	#[error("failed to get interface {0}")]
	IfaceLookup(String, #[source] Errno),
	#[cfg(unix)]
	#[error("failed to update map \"{1}\" for NF {0}")]
	MapUpdateFail(Uuid, String, #[source] BpfError),

	// TODO: move to ahead-of-time verifier.
	#[error("NF {0} is missing in received chain")]
	MissingNf(Uuid),
	#[error("NF {0}'s code is missing eBPF payload")]
	MissingEbpfPayload(Uuid),
	#[error("NF {0} is missing eBPF entrypoint `outer_xdp_sock_prog`")]
	MissingEbpfEntry(Uuid),
	#[error("link from {0} to {1} illegal: missing destination")]
	BadNfLink(Uuid, Uuid),
	#[error("intermediate NF {0} is missing the required map {1}")]
	MissingMap(Uuid, String),
	#[error("chain has no root NF from `rx` -- cannot execute")]
	MissingRootNf,
}
