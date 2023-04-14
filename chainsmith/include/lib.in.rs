// TODO: find a better longterm soln for accessing `nf` in this binary.
// `pub use nf::*` really sucks ergonomically, but also don't want to deal with
// pernickity task of finding `nf`'s folder wrt target and crate spec, and also prevent
// nfs from playing around with namespaces to break the function signature.
use {0}::RawMap;

#[no_mangle]
pub fn user_nf_program(pkt: &mut [u8], maps: &mut [RawMap]) -> usize {{
	// don't need to hardcast Maps, but do need to wrap them I assume?
	{1}

	{0}::packet(pkt,{2}) as usize
}}
