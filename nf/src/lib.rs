#![no_std]

pub use nf_macros::*;

pub mod example_map;
pub mod map;
pub mod packet;
pub mod random;

pub use self::{map::*, packet::*};

/// Effectively disables const/dead-code elimination and optimisations at a given
/// boundary. Useful for benchmarking or forcing busy code gen.
///
/// Taken from [criterion](https://docs.rs/criterion/latest/src/criterion/lib.rs.html#173-179).
pub fn black_box<T>(dummy: T) -> T {
	unsafe {
		let ret = core::ptr::read_volatile(&dummy);
		core::mem::forget(dummy);
		ret
	}
}
