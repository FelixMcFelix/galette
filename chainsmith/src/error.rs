use std::{io::Error as IoError, process::Output};

use syn::Error as SynError;
use thiserror::Error;

use crate::chain::Link;

#[derive(Debug, Error)]
pub enum SourceParseError {
	#[error("failed to open lib.rs for the NF `{0}`")]
	CrateLibRsRead(String, #[source] IoError),
	#[error("failed to read lib.rs for the NF `{0}`")]
	CrateLibRsParse(String, #[source] SynError),
	#[error("NF `{0}` does not expose the function `packet(..)`")]
	MissingPacketFn(String),
	#[error("NF `{nf_name}` does not define return type `{enum_name}` in the crate root")]
	EnumNotDefinedInRoot { nf_name: String, enum_name: String },
	#[error("coulfn't resolve return type for `packet(..)` in NF `{0}`")]
	CantResolveReturnType(String),
}

#[derive(Debug, Error)]
pub enum WriteXdpError {
	#[error("couldn't create source directory")]
	CreateDir(#[source] IoError),
	#[error("couldn't create NF source file")]
	CreateFile(#[source] IoError),
	#[error("couldn't write to NF source file")]
	WriteFile(#[source] IoError),
	#[error("invocation of NF {nf} is configured over {given_branches} branches, expected {needed_branches}")]
	BranchMismatch {
		nf: String,
		given_branches: usize,
		needed_branches: usize,
	},
}

#[derive(Debug, Error)]
pub enum CompileError {
	#[error("failed to call cargo-bpf compiler -- is it installed?")]
	CallCompile(#[source] IoError),
	#[error("cargo-bpf failed to compile source:\n{:?}", std::str::from_utf8(&.0.stderr))]
	DoCompile(Output),
	#[error("failed to read compiled binary {0}")]
	ReadElf(String, #[source] IoError),
}

#[derive(Debug, Error)]
pub enum ChainBuildError {
	#[error("chain had no link from the special `rx` NF")]
	NoRxHandler,
	#[error("chain had more than one link from the special `rx` NF")]
	TooManyRxHandlers,
	#[error("link from {1}: source {1} unknown")]
	UndefinedSource(Link, String),
	#[error("link from {}: target {} unknown", .0.from, .1)]
	UndefinedTarget(Link, String),
}
