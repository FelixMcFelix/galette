use libbpf_rs::{Link, Object};

pub enum Prog {
	Linked(Object, Link),
	Unlinked(Object),
}

impl Prog {
	pub fn object(&self) -> &Object {
		match self {
			Self::Linked(o, _) => o,
			Self::Unlinked(o) => o,
		}
	}

	pub fn object_mut(&mut self) -> &mut Object {
		match self {
			Self::Linked(o, _) => o,
			Self::Unlinked(o) => o,
		}
	}
}
