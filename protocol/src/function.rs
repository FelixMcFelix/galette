use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
pub struct Function {
	pub uuid: Uuid,
	pub elf: Option<Vec<u8>>,
	pub ebpf: Option<EbpfFunction>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EbpfFunction {
	pub link: Vec<u8>,
	pub end: Vec<u8>,
}
