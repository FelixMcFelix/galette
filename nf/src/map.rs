#[cfg(feature = "libbpf-rs")]
use core::{ffi::c_void, mem::MaybeUninit};

#[cfg(feature = "libbpf-rs")]
use libbpf_rs::{libbpf_sys, Map as HostMap, MapFlags};
#[cfg(feature = "redbpf-probes")]
use redbpf_probes::maps::*;

pub trait Map<K, V> {
	fn get(&mut self, key: &K) -> Option<V>;
	fn put(&mut self, key: &K, value: &V);
}

#[cfg(feature = "libbpf-rs")]
impl<K, V> Map<K, V> for &mut &mut HostMap {
	#[inline]
	fn get(&mut self, key: &K) -> Option<V> {
		// The builtin methods sadly make a box alloc, which we
		// REALLY don't want to pay for packet processing code.
		let val_space: MaybeUninit<V> = MaybeUninit::uninit();

		let err_code = unsafe {
			libbpf_sys::bpf_map_lookup_elem(
				self.fd(),
				(key as *const K) as *const c_void,
				val_space.as_ptr() as *mut c_void,
			)
		};

		if err_code == 0 {
			Some(unsafe { val_space.assume_init() })
		} else {
			None
		}
	}

	#[inline]
	fn put(&mut self, key: &K, value: &V) {
		let key_as_bytes = as_byte_slice(key);
		let val_as_bytes = as_byte_slice(value);

		let _ = self.update(key_as_bytes, val_as_bytes, MapFlags::ANY);
	}
}

// Note: this is something of a test to get slices.
#[cfg(feature = "libbpf-rs")]
#[derive(Clone, Copy)]
pub struct RawMap {
	fd: i32,
}

#[cfg(feature = "libbpf-rs")]
impl RawMap {
	/// # Safety
	/// This does NOT handle safe dropping of the underlying FDs: the callee must ensure
	/// that RawMap does not outlive its parent Map (and resp. Objects).
	///
	/// The callee must also make sure that this RawMap is only passed into the place
	/// of maps which contain the same (K, V) types.
	pub unsafe fn new(map: &HostMap) -> Self {
		Self { fd: map.fd() }
	}
}

#[cfg(feature = "libbpf-rs")]
impl<K, V> Map<K, V> for &mut RawMap {
	#[inline]
	fn get(&mut self, key: &K) -> Option<V> {
		// The builtin methods sadly make a box alloc, which we
		// REALLY don't want to pay for packet processing code.
		let val_space: MaybeUninit<V> = MaybeUninit::uninit();

		let err_code = unsafe {
			libbpf_sys::bpf_map_lookup_elem(
				self.fd,
				(key as *const K) as *const c_void,
				val_space.as_ptr() as *mut c_void,
			)
		};

		if err_code == 0 {
			Some(unsafe { val_space.assume_init() })
		} else {
			None
		}
	}

	#[inline]
	fn put(&mut self, key: &K, value: &V) {
		/*let key_as_bytes = as_byte_slice(key);
		let val_as_bytes = as_byte_slice(value);

		self.update(key_as_bytes, val_as_bytes, MapFlags::ANY);*/
		let _err_code = unsafe {
			libbpf_sys::bpf_map_update_elem(
				self.fd,
				(key as *const K) as *const c_void,
				(value as *const V) as *const c_void,
				MapFlags::ANY.bits(),
			)
		};
	}
}

#[cfg(feature = "libbpf-rs")]
fn as_byte_slice<T>(val: &T) -> &[u8] {
	unsafe {
		core::slice::from_raw_parts((val as *const T) as *const u8, core::mem::size_of::<T>())
	}
}

#[cfg(feature = "redbpf-probes")]
impl<V: Clone> Map<u32, V> for &mut Array<V> {
	#[inline]
	fn get(&mut self, key: &u32) -> Option<V> {
		Array::get(*self, *key).cloned()
	}

	#[inline]
	fn put(&mut self, key: &u32, value: &V) {
		self.set(*key, value)
	}
}

#[cfg(feature = "redbpf-probes")]
impl<K, V> Map<K, V> for &mut HashMap<K, V> {
	#[inline]
	fn get(&mut self, key: &K) -> Option<V> {
		self.get_val(key)
	}

	#[inline]
	fn put(&mut self, key: &K, value: &V) {
		self.set(key, value)
	}
}
