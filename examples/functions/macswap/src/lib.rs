#![no_std]
pub use nf::*;

pub enum Action {
	Yes
}

pub fn packet(mut pkt: impl Packet) -> Action {
	if let Some(bytes) = pkt.slice(12) {
		let (src_mac, rest) = bytes.split_at_mut(6);
		src_mac.swap_with_slice(&mut rest[..]);
    }

	Action::Yes
}
