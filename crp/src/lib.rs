use std::collections::HashMap;

use fixedbitset::FixedBitSet as BitSet;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum Response<T> {
	Unused(T),
	Used(T),
	Invalid,
}

pub trait CrpSource {
	type Challenge;
	type Response;

	fn respond(&self, challenge: Self::Challenge) -> Response<Self::Response>;

	fn mark_used(&mut self, challenge: Self::Challenge);
}

#[derive(Deserialize, Serialize)]
pub struct InMemory<T> {
	map: HashMap<u64, T>,
	used: BitSet,
}

impl<const N: usize> InMemory<[u8; N]> {
	pub fn new_random(n_challenges: usize) -> Self {
		let map = (0..n_challenges)
			.into_iter()
			.map(|i| (i as u64, rand::random()))
			.collect();

		let used = BitSet::with_capacity(n_challenges);

		InMemory { map, used }
	}
}

impl<T: Copy> CrpSource for InMemory<T> {
	type Challenge = u64;
	type Response = T;

	fn respond(&self, challenge: Self::Challenge) -> Response<Self::Response> {
		match self.map.get(&challenge).copied() {
			Some(a) if self.used.contains(challenge as usize) => Response::Used(a),
			Some(a) => Response::Unused(a),
			None => Response::Invalid,
		}
	}

	fn mark_used(&mut self, challenge: Self::Challenge) {
		if challenge < self.used.len() as u64 {
			self.used.insert(challenge as usize);
		}
	}
}
