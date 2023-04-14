pub use nf::*;

pub enum Action {
	NoOp
}

pub fn packet(mut _pkt: impl Packet) -> Action {
	std::thread::sleep(std::time::Duration::from_millis(1));

	Action::NoOp
}
