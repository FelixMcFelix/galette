use serde::{Deserialize, Serialize};
use tungstenite::protocol::Message;

use super::*;

#[derive(Debug, Deserialize, Serialize)]
pub enum ClientToServer {
	RequestChain(String),
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ServerToClient {
	Chain(Chain),
	RequestChainError(String),
}

// TODO: modify below to account for falcon and AES state.
pub fn ser<T: Serialize>(dat: &T) -> Message {
	let o = postcard::to_stdvec(&dat).expect("All local types should be postcard-friendly.");
	Message::Binary(o)
}

pub fn deser<'a, T: Deserialize<'a>>(dat: &'a Message) -> Result<Option<T>, DeserError> {
	match dat {
		Message::Binary(b) => postcard::from_bytes(b)
			.map_err(DeserError::Fail)
			.map(Option::Some),
		Message::Ping(_) | Message::Pong(_) => Ok(None),
		_ => Err(DeserError::NonBinaryWs),
	}
}
