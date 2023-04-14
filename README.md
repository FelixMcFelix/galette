# Galette
Galette is an experimental XDP/AF_XDP-based dataplane designed to run service function chains on single-board compute devices, for inexpensive installation and defence of edge networks.
You can [read more in our paper](https://mcfelix.me/docs/papers/ifip-2023-galette.pdf), due to be presented at IFIP Networking '23.

```bibtex
@INPROCEEDINGS{Simp2306:Galette,
	AUTHOR="Kyle A. Simpson and Chris Williamson and Douglas J. Paul and Dimitrios P.
Pezaros",
	TITLE="Galette: A Lightweight {XDP} Dataplane on Your Raspberry Pi",
	BOOKTITLE="International Federation for Information Processing (IFIP) Networking 2023 Conference (IFIP Networking 2023)",
	ADDRESS="Barcelona, Spain",
	DAYS="11",
	MONTH=jun,
	YEAR=2023,
}
```

## Components and How-to
Galette is built from some key components:
 * 'chainsmith' -- the compile server responsible for compiling and serving *service function chains* to clients.
 * `pulley` -- the XDP/AF_XDP runtime installed on a host machine doing packet processing.
 * `nf` -- a support crate containing types and traits necessary to write dual eBPF/native-code network functions.

Any examples can be run or built using `cargo make ex-01`, `02`, etc.
Examples are laid out to show how NFs can be written and composed together with little effort.

### TLS
As part of the TruSDEd project, Galette includes mockup code for PUF-based authentication of self-signed certificates via a fork of webpki. The pulley client requires access to a (random) prebuilt challenge-response pair database if connecting over `wss://` -- connecting over `ws://` does not impose this requirement.

### Limitations
Galette does not currently include the capability to dynamically reconfigure SFCs served by chainsmith, or to serve variable map contents. Some map KV pairs are special-cased for insertion from `pulley` to show the operation of some `Map`-based NFs.

## Requirements

### Dependencies
* libelf (via elfutils)
* zlib

If you are cross-compiling---either to compile `pulley` for your target SBC device, or to compile userland NFs for another target---you *must* also install these libraries to your cross-compiler's SYSROOT (i.e., `aarch64-linux-gnu-gcc --print-sysroot`) in addition to your host setup.

To build eBPF NFs using `chainsmith`, you will need to install `cargo bpf` via the redbpf tool suite.
Due to its (current) LLVM version restrictions, you will need to have Rust v1.59 installed via rustup.

### Runtime

* must have vmlinux for target linux kernel?

* Kernel on target machine must be built with XDP, eBPF, and BTF support.

* Target machine must have `libelf` and `zlib` installed, as these are linked dynamically to `libbpf-sys`.

## Paper source and experimental results.
The evaluation harness for galette [can be found here](https://github.com/FelixMcFelix/galette-paper/). The evaluation repository should be cloned into this repository if you intend to run any of the experimental runners we have developed.
