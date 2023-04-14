#![no_std]
pub use nf::*;

#[maps]
pub struct FilterMaps {
	blocked_ips: (u32, bool),
	shared_counter: (u32, u32),
}

pub enum Action {
	Allow,
	Block,
}

pub fn packet<M1, M2>(mut pkt: impl Packet, mut maps: FilterMaps<M1, M2>) -> Action
where
	M1: Map<u32, bool>,
	M2: Map<u32, u32>,
{
	// eth: 0..14
	// ip: 14.. (v4 proto: 9 ttl: 8, v6 nexthead: 6 hop_limit: 7)
	let src_addr_idx = 14 + match pkt.slice_from(12,2) {
		Some(&mut [0x08, 0x00]) => {
			//ipv4
			12
		},
		Some(&mut [0x86, 0xDD]) => {
			//ipv6
			// 7
			return Action::Block;
		},
		_ => {return Action::Block},
	};

	// let addr = u32::from_be_bytes(bytes[src_addr_idx..][..4].try_into().unwrap());
	let addr = u32::from_be_bytes(pkt.slice_from(src_addr_idx, 4).unwrap().try_into().unwrap());

	match maps.blocked_ips.get(&addr) {
		Some(true) => Action::Block,
		_ => Action::Allow,
	}
}
