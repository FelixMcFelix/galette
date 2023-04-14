use clap::{Parser, ValueEnum};

#[derive(Clone, Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
	#[clap(default_value_t = String::from("."), value_parser)]
	/// Path to a folder containing a `chain.toml` configuration.
	pub path: String,

	#[clap(default_value_t = String::from("127.0.0.1:8080"), value_parser, long)]
	/// Connection string to bind the WebSocket server to.
	pub conn_string: String,

	#[clap(value_parser, long)]
	/// Target architecture to build user-space NFs for (e.g., "aarch64-unknown-linux-gnu").
	///
	/// If specified, this should be a rust compiler tuple. Otherwise, this implicitly
	/// matches the host.
	pub target: Option<String>,

	#[clap(value_parser, long)]
	/// Vmlinux BTF path to use when building eBPF NFs.
	///
	/// This sets the `ENV_VMLINUX_PATH` environment variable used in downstream calls
	/// to `cargo bpf`.
	pub vmlinux: Option<String>,

	#[arg(value_enum, default_value_t = TlsMode::NoTls, long)]
	/// Configures how `chainsmith` and `pulley` authenticate with one another.
	///
	/// All non-tls modes use pre-shared keys, currently taken from the `certs` folder
	/// at compile time.
	/// `puf-tls` writes out a challenge-response database at runtime which must be served to
	/// the `pulley` client.
	pub tls_mode: TlsMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[clap(rename_all = "kebab_case")]
pub enum TlsMode {
	NoTls,
	ServerAuth,
	ClientServerAuth,
	PufTls,
}

impl TlsMode {
	pub fn is_tls_used(&self) -> bool {
		!matches!(self, Self::NoTls)
	}

	pub fn should_install_client_cert(&self) -> bool {
		matches!(self, Self::PufTls | Self::ClientServerAuth)
	}

	pub fn generate_crps(&self) -> bool {
		matches!(self, Self::PufTls)
	}
}
