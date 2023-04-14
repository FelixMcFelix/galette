// how do I want this to work?
// if we have just one grant, this is easy -- swap and extend.
//  effectively zero-cost: don't need to track claim ids
// if we want to get an extra slice? new problem:
//  extra slice can also be extended: need to prevent aliasing or only allow growth of LAST slice.
//  can prevent by adding ids to both XdpContext and each Grant.
//  can we do clever typesystem work?
//
// just do `get` and `extend` for now.
// actually, don't need extend: drop get(n) -> stuff -> get(n+m) drops handle and &mut

#[cfg(feature = "redbpf-probes")]
use core::slice;

#[cfg(feature = "redbpf-probes")]
use redbpf_probes::{net::NetworkBuffer, xdp::XdpContext};

mod private {
	#[cfg(feature = "redbpf-probes")]
	use super::*;

	pub trait Sealed {}

	#[cfg(feature = "redbpf-probes")]
	impl Sealed for &XdpContext {}

	impl Sealed for &mut [u8] {}
}

/// A consistent packet access API for userland and XDP-offloaded NFs.
///
/// This trait is sealed to allow only in-library implementations.
#[allow(clippy::len_without_is_empty)]
pub trait Packet: private::Sealed {
	/// Request a packet slice of size `len`.
	///
	/// In general, it is valid and easy to simply ask for larger
	/// packet slices as needed. If you need to make use of a variable
	/// length size, then [Self::slice_from] should be used with the variable
	/// size used as an offset.
	#[inline]
	fn slice(&mut self, len: usize) -> Option<&mut [u8]> {
		self.slice_from(0, len)
	}
	/// Request a packet slice of size `len` from a given `offset` into the
	/// packet.
	///
	/// This function is responsible for checking pointer bounds in a way that
	/// the eBPF verifier accepts.
	fn slice_from(&mut self, offset: usize, len: usize) -> Option<&mut [u8]>;
	/// Returns the number of bytes accessible to the current NF.
	fn len(&self) -> usize;
}

#[cfg(feature = "redbpf-probes")]
impl Packet for &XdpContext {
	#[inline]
	fn slice(&mut self, len: usize) -> Option<&mut [u8]> {
		self.slice_from(0, len)
	}

	#[inline]
	fn slice_from(&mut self, offset: usize, len: usize) -> Option<&mut [u8]> {
		let base = self.data_start() + offset;
		(*self).check_bounds(base, base + len).ok()?;

		let ptr = base as *mut u8;

		Some(unsafe { slice::from_raw_parts_mut(ptr, len) })
	}

	#[inline]
	fn len(&self) -> usize {
		NetworkBuffer::len(*self)
	}
}

impl Packet for &mut [u8] {
	#[inline]
	fn slice(&mut self, len: usize) -> Option<&mut [u8]> {
		self.get_mut(0..len)
	}

	#[inline]
	fn slice_from(&mut self, offset: usize, len: usize) -> Option<&mut [u8]> {
		self.get_mut(offset..(offset + len))
	}

	#[inline]
	fn len(&self) -> usize {
		<[u8]>::len(self)
	}
}
