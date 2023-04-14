#![no_std]
pub use nf::*;

// use pnet::packet::{
// 	ethernet::{EtherTypes, MutableEthernetPacket},
// 	ipv4::MutableIpv4Packet,
// 	ipv6::MutableIpv6Packet,
// 	MutablePacket,
// };

pub enum Action {
	Yes
}

pub fn packet(mut pkt: impl Packet) -> Action {
	// NOTE: this is blocked on pnet pulling their finger out and impl'ing
	// no_std WITHOUT ALLOC.

	// let mut eth = if let Some(e) = MutableEthernetPacket::new(bytes) {e} else {return Action::Yes};
	// let eth_type = eth.get_ethertype();
	// let payload = eth.payload_mut();
	
	// match eth_type {
	// 	EtherTypes::Ipv4 => {
	// 		let mut ip = if let Some(i) = MutableIpv4Packet::new(payload) {i} else {return Action::Yes};
	// 		let ttl = ip.get_ttl().saturating_sub(1);
	// 		ip.set_ttl(ttl);
	// 	},
	// 	EtherTypes::Ipv6 => {
	// 		let mut ip = if let Some(i) = MutableIpv6Packet::new(payload) {i} else {return Action::Yes};
	// 		let hop_limit = ip.get_hop_limit().saturating_sub(1);
	// 		ip.set_hop_limit(hop_limit);
	// 	},
	// 	_ => {},
	// }

	/*if bytes.len() < 23 {
		return Action::Yes;
	}*/
    
    //if black_box((bytes.as_ptr() as usize) + 23 < end) {
    //    return Action::Yes;
    //}

	// eth: 0..14
	// ip: 14.. (v4 proto: 9 ttl: 8, v6 nexthead: 6 hop_limit: 7)
	if let Some(bytes) = pkt.slice(23) {
		let ttl_idx = 14 + match &bytes[12..14] {
			&[0x08, 0x00] => {
				//ipv4
				8
			},
			&[0x86, 0xDD] => {
				//ipv6
				7
			},
			_ => {return Action::Yes},
		};

		bytes[ttl_idx] = bytes[ttl_idx].saturating_sub(1);
	}

	Action::Yes
}
