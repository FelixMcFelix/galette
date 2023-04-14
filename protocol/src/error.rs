use postcard::Error as PostcardError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeserError {
	#[error("received an illegal message -- all messages must be binary")]
	NonBinaryWs,
	#[error("failed to deserialise  message into needed type")]
	Fail(#[source] PostcardError),
}
