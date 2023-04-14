#![no_std]
pub use nf::*;

pub enum Action {
	Left,
	Right,
	Up,
	Down,
}

pub fn packet(mut pkt: impl Packet) -> Action {
	// eth: 0..14
	// ip: 14.. (v4 proto: 9 ttl: 8, v6 nexthead: 6 hop_limit: 7)

	// switch on LSB of dest addr.
	// v4: (14 +) 16..20 [19]
	// v6: (14 +) 24..40 [39]
	// Only do IPv4 for now while I work out better slice handling.
	
	let addr_lsb_idx = 14 + match pkt.slice_from(12, 2) {
		//ipv4
		Some(&mut [0x08, 0x00]) => 19,
		//ipv6
		Some(&mut [0x86, 0xDD]) => 39,
		_ => {return Action::Left},
	};

	match pkt.slice_from(addr_lsb_idx, 1).map(|v| v[0] % 2) {
		Some(0) => Action::Left,
		Some(1) => Action::Right,
		Some(2) => Action::Up,
		Some(3) => Action::Down,
		_ => unreachable!(),
	}
}
