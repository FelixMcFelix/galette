use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::*;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct XdpLink {
	pub uuid: Uuid,
	pub state: XdpLinkState,
	pub root: bool,
	pub disable_xdp: bool,
	pub map_names: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum XdpLinkState {
	Tail,
	Body(Vec<LinkAction>),
}

impl XdpLinkState {
	pub fn act(&self, action: u32) -> LinkAction {
		match self {
			Self::Tail => LinkAction::Tx,
			Self::Body(acts) => acts[action as usize],
		}
	}
}

#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum LinkAction {
	Tx,
	Drop,
	Abort,
	Upcall(Uuid),
	Tailcall(Uuid),
	Pass,
}

impl LinkAction {
	pub fn to_kind(&self) -> u8 {
		match self {
			Self::Tx => 0,
			Self::Drop => 1,
			Self::Abort => 2,
			Self::Upcall(_) => 3,
			Self::Tailcall(_) => 4,
			Self::Pass => 5,
		}
	}

	pub fn next_nf(&self) -> Option<Uuid> {
		match self {
			Self::Upcall(id) | Self::Tailcall(id) => Some(*id),
			_ => None,
		}
	}
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Chain {
	pub links: Vec<XdpLink>,
	pub nfs: HashMap<Uuid, Function>,
}
