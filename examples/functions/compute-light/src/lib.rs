#![no_std]
pub use nf::*;

pub enum Action {
	NoOp
}

pub fn packet(mut _pkt: impl Packet) -> Action {
	let mut x: u64 = 0;
	for i in 0..1_000 {
		x += black_box(1);
	}

	Action::NoOp
}
