#![no_std]
#![no_main]
use cty::*;
use redbpf_probes::{{
	bindings::bpf_func_id_BPF_FUNC_xdp_adjust_meta,
	maps::*,
	xdp::prelude::*,
}};

type ProgId = u32;

#[repr(C)]
struct DataplaneState {{
	prog_id: ProgId,
	num_cores: u32,
}}

program!(0xFFFFFFFE, "GPL");

#[map(link_section = "maps")]
static mut acts_map: Array<u8> = Array::with_max_entries({2});

#[map(link_section = "maps")]
static mut progs_map: ProgramArray = ProgramArray::with_max_entries({2});

#[map(link_section = "maps")]
static mut my_state_map: Array<DataplaneState> = Array::with_max_entries(1);

#[map(link_section = "maps")]
static mut xsk_map: XskMap = XskMap::with_max_entries(8);

{3}

#[xdp]
fn xdp_sock_prog(mut ctx: XdpContext) -> XdpResult {{
	// TODO: automatically limit using size param {1:?}
	{4}

	let out: u32 = {0}::packet(&ctx,{5}) as u32;

	match unsafe {{ acts_map.get(out) }} {{
		// tx
		Some(0) => Ok(XdpAction::Tx),
		// drop
		Some(1) => Ok(XdpAction::Drop),
		// abort
		Some(2) => Ok(XdpAction::Aborted),
		// upcall
		Some(3) => {{
            const EXTRA_BYTES: c_int = (core::mem::size_of::<ProgId>() + core::mem::size_of::<u32>()) as c_int;
			// expand meta
			let r_val = unsafe {{
				let bpf_xdp_adjust_meta: unsafe extern "C" fn(
					ctx: *mut c_void,
					delta: c_int,
				) -> i64 = core::mem::transmute(bpf_func_id_BPF_FUNC_xdp_adjust_meta as usize);
				bpf_xdp_adjust_meta(ctx.ctx as *mut c_void, -EXTRA_BYTES)
			}};

            let s_ptr = unsafe {{ (*ctx.ctx).data_meta }} as usize;
            let n_ptr = s_ptr + (EXTRA_BYTES as usize);
            let e_ptr = unsafe {{ (*ctx.ctx).data }} as usize;

			if r_val >= 0 && n_ptr <= e_ptr {{
				Ok(unsafe {{
					let state = my_state_map.get(0).unwrap();

					let target_core = if state.num_cores != 1 {{
						bpf_get_prandom_u32() % state.num_cores
					}} else {{
						0
					}};
					
					core::ptr::write(s_ptr as *mut ProgId, state.prog_id);
					core::ptr::write((s_ptr + core::mem::size_of::<ProgId>()) as *mut u32, out);

					// redirect into xsk_map
					xsk_map
						.redirect(target_core)
						.map(|_| XdpAction::Redirect)
						.unwrap_or(XdpAction::Aborted)
				}})
			}} else {{
				Ok(XdpAction::Aborted)
			}}
		}},
		// tailcall
		Some(4) => {{
			unsafe {{
				progs_map.tail_call(ctx.ctx, out)
			}};

			Ok(XdpAction::Aborted)
		}},
		Some(5) => {{
			Ok(XdpAction::Pass)
		}}
		_ => unsafe {{ Err(NetworkError::OutOfBounds) }},
	}}
}}
