#![no_std]
pub use nf::*;

pub enum Action {
	NoOp
}

pub fn packet(mut _pkt: impl Packet) -> Action {
	Action::NoOp
}
