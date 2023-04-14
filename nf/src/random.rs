// TODO: impl RngCore backed by xdp's prng and
//
// pub struct Rng {
// 	#[cfg(feature = "user")]

// }

#[cfg(feature = "xdp")]
#[inline]
pub fn random_u32() -> u32 {
	redbpf_probes::helpers::bpf_get_prandom_u32()
}

#[cfg(feature = "user")]
#[inline]
pub fn random_u32() -> u32 {
	rand::random::<u32>()
}
