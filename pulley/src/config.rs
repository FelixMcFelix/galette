use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
	#[clap(default_value_t = String::from("wss://localhost:8080"), value_parser)]
	/// URL for a `chainsmith` compiler server, i.e. "wss://" or "ws://127.0.0.1:8080".
	pub server_addr: String,

	#[clap(value_parser, long, short = 'i', required = true, num_args=1..)]
	/// Ethernet interface[s] to attach XDP programs to.
	pub interface: Vec<String>,

	#[clap(value_parser, long)]
	/// Sets the number of XDP processing threads to spawn.
	///
	/// Defaults to `(n_cores - 1)`, but will be capped to a maximum of 8.
	pub xdp_cores: Option<u32>,

	#[arg(value_enum, long)]
	/// If set, `AF_XDP` threads will share a single umem pool rather than
	/// each managing their own umem handles.
	///
	/// This flag *must* be set if the chosen/derived value of `--xdp-cores`
	/// is greater than 1.
	pub share_umem: bool,

	#[arg(value_enum, long, default_value_t = UmemDisposalMode::FirstThread)]
	/// Configures how dataplane threads will dispose and recycle used umem
	/// frame descriptors.
	///
	/// FQ/CQ handling can either be handled by the first XDP thread (`first-thread`), or
	/// by spawning an additional thread solely responsible for managing frame descriptors
	/// (`extra-thread`).
	pub umem_mode: UmemDisposalMode,

	#[clap(value_parser, long, default_value_t = 5)]
	/// Sets the timeout (ms) to use when each thread polls its XDP socket.
	///
	/// Defaults to 5ms.
	pub upcall_poll_timeout: usize,

	#[clap(value_parser, long, default_value_t = 0.5)]
	/// Sets the likelihood to upcall, as used in the 'upcall_likelihood' map for the
	/// 'load-balance' NF.
	///
	/// Testing value for evaluation, not intended for production use.
	///
	/// Defaults to 0.5 (50%).
	pub loadbalance_chance: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[clap(rename_all = "kebab_case")]
pub enum UmemDisposalMode {
	FirstThread,
	ExtraThread,
}
