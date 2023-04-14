use std::{
	num::NonZeroUsize,
	sync::{
		mpsc::{self, TryRecvError},
		Arc,
	},
	time::Duration,
};

#[cfg(unix)]
use bus::BusReader;
use clap::Parser;
use crossbeam_channel::RecvTimeoutError;
use pulley::{
	config::{Cli, UmemDisposalMode},
	DylibStore,
	ProgId,
};
#[cfg(unix)]
use pulley::{ChainState, MapHaxType, UmemMediate, XskData};
use ringbuf::{HeapConsumer, HeapProducer, SharedRb};
#[cfg(unix)]
use xsk_rs::umem::frame::FrameDesc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
	let mut config = Cli::parse();

	let xdp_estimate_par = std::thread::available_parallelism()
		.map(NonZeroUsize::get)
		.map(|v| (v - 1) as u32)
		.unwrap_or(1);

	let xdp_ct = config.xdp_cores.get_or_insert(xdp_estimate_par);

	*xdp_ct = (*xdp_ct).max(1);
	*xdp_ct = (*xdp_ct).min(8);

	if *xdp_ct > 1 && !config.share_umem {
		Err(anyhow::anyhow!("Config error: derived num of cores was {xdp_ct}. Please enable `--share-umem`, or reduce core count via `--xdp-cores 1`."))?;
	}

	let xdp_ct = *xdp_ct;

	let chain = pulley::get_chain(config.server_addr.as_str()).await?;

	// TODO: move some expects from install_chain into a Chain::verify -> VerifyError?

	let mut xsks = pulley::create_upcall_sockets(&config);
	let g_live_fds = Arc::new(pulley::install_chain(&chain, &config, &xsks)?);

	let mut dylibs = DylibStore::new().await?;
	dylibs.load_dylib_nfs(&chain).await?;

	let g_dylibs = Arc::new(dylibs);

	let mut bus = bus::Bus::new(xsks.len());

	let needed_rings =
		(xdp_ct - u32::from(config.umem_mode == UmemDisposalMode::FirstThread)) as usize;
	let mut ring_senders = Vec::with_capacity(needed_rings);
	let mut ring_receivers = Vec::with_capacity(needed_rings);
	for i in 0..needed_rings {
		let (tx, rx) = ringbuf::SharedRb::new(2048).split();
		ring_senders.push(tx);
		ring_receivers.push(rx);
	}

	let mut ring_receivers = Some(ring_receivers);

	if config.umem_mode == UmemDisposalMode::ExtraThread {
		let mut rx = bus.add_rx();

		let mut my_frames = xsks[0].frames.clone();
		let mut mediate = xsks[0].mediate.take().unwrap();
		let mut ring_receivers = ring_receivers
			.take()
			.expect("FATAL: extra disposal thread requires exclusive access to pkt rings");

		let _hangup = std::thread::spawn(move || {
			// wait unconditionally on: packets sent via crossbeam [long timeout]
			// between timeouts, check bus rx; if bad, kill
			loop {
				let mut handled = 0;

				// Packet drop handling.
				for ring in &mut ring_receivers {
					if ring.len() == 0 {
						continue;
					}

					let (s1, s2) = ring.as_slices();
					let this_handled = s1.len() + s2.len();
					unsafe {
						mediate.fq.produce(s1);
						mediate.fq.produce(s2);

						// SAFETY: can `advance` as FrameDesc does not have custom
						// drop logic.
						ring.advance(this_handled);
					}

					handled += this_handled;
				}

				unsafe {
					let additional_pkts = mediate.cq.consume(&mut my_frames[..]);
					mediate.fq.produce(&my_frames[..additional_pkts]);
					handled += additional_pkts;
				}

				if rx.try_recv().is_ok() {
					break;
				}

				if handled == 0 {
					std::thread::sleep(Duration::from_millis(5));
				}
			}
		});
	}

	let n_cores = xsks.len();

	// TODOS:
	//  Expand to multiple cores?
	//   Multiple XSKs: modify eBPF handlers to use p_random load balance
	//   Umem sharing options: expose shared vs owned as a config option
	//  Keep a slice of maps for each NF.
	//  Pre-cache (instance_id,act) -> next_uuid lookups?
	//  Mechanism to store & communicate spent descs to thread 0 in shared_umem mode
	#[cfg(unix)]
	for (t_id, mut xsk) in xsks.drain(..).enumerate() {
		let dylibs = g_dylibs.clone();
		let mut kill_rx = bus.add_rx();

		let live_fds = g_live_fds.clone();
		let mut map_hax = live_fds.raw_maps.clone();

		let maybe_rx_set = ring_receivers.take();
		let maybe_sender = if config.umem_mode == UmemDisposalMode::ExtraThread || t_id != 0 {
			ring_senders.pop()
		} else {
			None
		};

		let _hangup = std::thread::spawn(move || {
			core_affinity::set_for_current(core_affinity::CoreId { id: t_id + 1 });

			if maybe_sender.is_some() {
				let sender = maybe_sender.expect("FATAL: thread was not given valid ringbuf tx");
				dataplane_other_mediate(
					xsk,
					live_fds,
					dylibs,
					kill_rx,
					config.upcall_poll_timeout,
					sender,
				);
			} else if n_cores == 1 {
				let mediate = xsk
					.mediate
					.take()
					.expect("FATAL: thread 0 lacks Umem mediate info");
				dataplane_self_mediate_solo(
					xsk,
					live_fds,
					dylibs,
					kill_rx,
					config.upcall_poll_timeout,
					mediate,
				);
			} else {
				let receivers =
					maybe_rx_set.expect("FATAL: thread 0 was not given valid ringbuf rxs");
				let mediate = xsk
					.mediate
					.take()
					.expect("FATAL: thread 0 lacks Umem mediate info");
				dataplane_self_mediate(
					xsk,
					live_fds,
					dylibs,
					kill_rx,
					config.upcall_poll_timeout,
					mediate,
					receivers,
				);
			}
		});
	}

	println!("Press ctrl+c to exit.");
	tokio::signal::ctrl_c().await?;

	let _ = bus.broadcast(());

	g_dylibs.cleanup().await?;

	Ok(())
}

// Dataplane impls below are done to remove conditionals from main loop.

#[cfg(unix)]
/// Use if xdp cores == 1 and self.
fn dataplane_self_mediate_solo(
	mut xsk: XskData,
	chain: Arc<ChainState>,
	dylibs: Arc<DylibStore>,
	mut kill_rx: BusReader<()>,
	timeout: usize,
	mut mediate: UmemMediate,
) {
	let mut map_hax = chain.raw_maps.clone();
	loop {
		match kill_rx.try_recv() {
			Ok(()) | Err(TryRecvError::Disconnected) => break,
			_ => {},
		}

		let (num_tx, pkts_recvd) = dataplane_core(&mut xsk, &chain, &dylibs, timeout, &mut map_hax);

		// How to handle decisions?
		// tx all descs in `descs[..num_tx]`
		// non-tx => swap-remove, num_tx -= 1
		unsafe {
			// actual tx step -- *do* we want to batch these like this?
			// or intersperse sends above?
			xsk.tx.produce_and_wakeup(&xsk.frames[..num_tx]).unwrap();

			// if pkts_recvd != 0 {
			// 	eprintln!("Tx'd {num_tx} packets via AF_XDP.");
			// }

			let additional_pkts = mediate.cq.consume(&mut xsk.frames[pkts_recvd..]);
			mediate
				.fq
				.produce(&xsk.frames[num_tx..pkts_recvd + additional_pkts]);
		};
	}
}

#[cfg(unix)]
fn dataplane_self_mediate(
	mut xsk: XskData,
	chain: Arc<ChainState>,
	dylibs: Arc<DylibStore>,
	mut kill_rx: BusReader<()>,
	timeout: usize,
	mut mediate: UmemMediate,
	mut remote_descs: Vec<HeapConsumer<FrameDesc>>,
) {
	let mut map_hax = chain.raw_maps.clone();
	loop {
		match kill_rx.try_recv() {
			Ok(()) | Err(TryRecvError::Disconnected) => break,
			_ => {},
		}

		let (num_tx, pkts_recvd) = dataplane_core(&mut xsk, &chain, &dylibs, timeout, &mut map_hax);

		// How to handle decisions?
		// tx all descs in `descs[..num_tx]`
		// non-tx => swap-remove, num_tx -= 1
		unsafe {
			// actual tx step -- *do* we want to batch these like this?
			// or intersperse sends above?
			xsk.tx.produce_and_wakeup(&xsk.frames[..num_tx]).unwrap();

			// return all credits for dropped packets *and* those
			// the kernel has finished with.
			let additional_pkts = mediate.cq.consume(&mut xsk.frames[pkts_recvd..]);
			mediate
				.fq
				.produce(&xsk.frames[num_tx..pkts_recvd + additional_pkts]);
		};

		// rx from other threads as needed.
		for ring in &mut remote_descs {
			if ring.len() == 0 {
				continue;
			}

			let (s1, s2) = ring.as_slices();
			let this_handled = s1.len() + s2.len();
			unsafe {
				mediate.fq.produce(s1);
				mediate.fq.produce(s2);

				// SAFETY: can `advance` as FrameDesc does not have custom
				// drop logic.
				ring.advance(this_handled);
			}
		}
	}
}

#[cfg(unix)]
fn dataplane_other_mediate(
	mut xsk: XskData,
	chain: Arc<ChainState>,
	dylibs: Arc<DylibStore>,
	mut kill_rx: BusReader<()>,
	timeout: usize,
	mut fd_sender: HeapProducer<FrameDesc>,
) {
	let mut map_hax = chain.raw_maps.clone();
	loop {
		match kill_rx.try_recv() {
			Ok(()) | Err(TryRecvError::Disconnected) => break,
			_ => {},
		}

		let (num_tx, pkts_recvd) = dataplane_core(&mut xsk, &chain, &dylibs, timeout, &mut map_hax);

		// How to handle decisions?
		// tx all descs in `descs[..num_tx]`
		// non-tx => swap-remove, num_tx -= 1
		unsafe {
			// actual tx step -- *do* we want to batch these like this?
			// or intersperse sends above?
			xsk.tx.produce_and_wakeup(&xsk.frames[..num_tx]).unwrap();
		}

		fd_sender.push_slice(&xsk.frames[num_tx..pkts_recvd]);
	}
}

#[cfg(unix)]
#[inline(always)]
/// Returns (send, total).
fn dataplane_core(
	xsk: &mut XskData,
	chain: &Arc<ChainState>,
	dylibs: &Arc<DylibStore>,
	timeout: usize,
	map_hax: &mut MapHaxType,
) -> (usize, usize) {
	// run-to-completion for each packet where possible.
	// check for ctl plane signalling every... 5ms?
	let mut pkts_recvd = unsafe {
		xsk.rx
			.poll_and_consume(&mut xsk.frames, timeout as i32)
			.unwrap()
	};
	let mut num_tx = pkts_recvd;

	// if pkts_recvd != 0 {
	// 	eprintln!("T{t_id}: Rx'd {pkts_recvd} packets via AF_XDP.");
	// }

	let mut i = 0;
	while i < num_tx {
		let recv_desc = &mut xsk.frames[i];
		// eprintln!("{recv_desc:?}");

		let pid_len = core::mem::size_of::<ProgId>();
		let act_len = core::mem::size_of::<u32>();
		let needed_len = pid_len + act_len;

		let headroom = unsafe { xsk.umem.headroom(recv_desc) };
		let contents = headroom.contents();
		let hr_ptr = contents.as_ptr();
		let dat = unsafe { xsk.umem.data(recv_desc) };
		let dat_ptr = dat.as_ptr();
		let avail_len = (dat_ptr as usize).checked_sub(hr_ptr as usize);

		// Truest headroom: XSK-rs assumes the space is not written to.
		let (src_nf, act) = if avail_len == Some(needed_len) {
			let headroom_slice =
				unsafe { core::slice::from_raw_parts(contents.as_ptr(), needed_len) };

			let src_nf = ProgId::from_ne_bytes(headroom_slice[0..pid_len].try_into().unwrap());
			let act = u32::from_ne_bytes(headroom_slice[pid_len..].try_into().unwrap());

			(src_nf, act)
		} else {
			// EMERGENCY: print first... 20 bytes?
			let dat = unsafe { xsk.umem.data(recv_desc) };
			let dl = dat.len();
			// eprintln!("WHOOPSIE: {:?}", &dat[0..20.min(dl)]);

			// TODO: flag dataplane err somehow? This should never occur!
			// Still, remove packet as below.
			num_tx -= 1;
			xsk.frames[..].swap(i, num_tx);
			continue;
		};

		// eprintln!("S, A: {src_nf} {act}");

		let mut data = unsafe { xsk.umem.data_mut(recv_desc) };
		let body = data.contents_mut();

		//  get fn, jumptable for entry pt
		//  loop:
		//   run fn on pkt.
		//   index retval into table -> enum of {Sk(Fd), Fn(usize), Drop}
		//   if not fn? send or not, then break
		//  return umem credit to cq/fq?

		let src_uuid = chain.instance_ids.get(&src_nf).unwrap();
		// eprintln!("called by {src_uuid}");
		// eprintln!("{:#?}", chain.link_states);
		let mut curr_uuid = chain
			.link_states
			.get(src_uuid)
			.unwrap()
			.act(act)
			.next_nf()
			.unwrap();

		let do_tx = loop {
			// TODO: select maps, put them in a slice somehow?
			//    should these be prebuilt?
			//    can we clone map fds freely?
			let mut maps = map_hax.get_mut(&curr_uuid);
			let lib = dylibs.dylibs.get(&curr_uuid).unwrap();
			let act = lib.user_nf_program(
				body,
				&mut maps.as_mut().map(|v| &mut v[..]).unwrap_or(&mut []),
			);

			// eprintln!("Got {act}, NF has choices {0:?}.", live_fds.link_states);

			match chain.link_states.get(&curr_uuid).unwrap().act(act as u32) {
				protocol::LinkAction::Tailcall(id) | protocol::LinkAction::Upcall(id) => {
					curr_uuid = id;
				},
				protocol::LinkAction::Tx => break true,
				// feed drops/aborts AND cq'd packets back into fq (inc. userland counters?)
				// TODO: increment atomic ctrs?
				_ => break false,
			}
		};

		if do_tx {
			// Packet Tx'd: leave in place.
			i += 1;
		} else {
			// Packet dropped: swap remove from "Tx block" of slice.
			num_tx -= 1;
			xsk.frames[..].swap(i, num_tx);
		}
	}

	(num_tx, pkts_recvd)
}
