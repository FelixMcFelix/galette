#![no_std]
#![no_main]
use redbpf_probes::{{maps::*, xdp::prelude::*}};

program!(0xFFFFFFFE, "GPL");

#[map(link_section = "maps")]
static mut my_id_map: Array<u32> = Array::with_max_entries(1);

{2}

#[xdp]
fn xdp_sock_prog(ctx: XdpContext) -> XdpResult {{
	// TODO: optionally limit using sz param {1:?}
	{3}

	let out = {0}::packet(&ctx,{4});

	Ok(XdpAction::Tx)
}}
