#![no_std]
pub use nf::*;

#[maps]
pub struct Maps {
	upcall_likelihood: (u32, u32),
}

pub enum Action {
	KeepXdp,
	Upcall,
}

pub fn packet<M1>(mut pkt: impl Packet, mut maps: Maps<M1>) -> Action
where
	M1: Map<u32, u32>,
{ 
	match maps.upcall_likelihood.get(&0) {
		Some(v) if v == u32::MAX || v > random::random_u32() => Action::Upcall,
		_ => Action::KeepXdp,
	}
}
