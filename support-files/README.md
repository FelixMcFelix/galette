# Evaluation support

This directory contains a few useful tools and scripts for setting up an evaluation environment:

* `.config`: Kernel config used to compile Rpi Linux v5.15.74-v8+ (aarch64) with XDP, eBPF, and BTF support.
* `*.tar.*`: Known-good compile-time dependency sources for libbpf v0.8.1.
* `install-cross-deps.sh`: Simple script to automate installation of dependencies into the cross-compiler sysroot.
* `vmlinux`: BTF vmlinux instance for the above raspberry pi kernel version.

## Cross-compile toolchain

### Runtime

If your target SBC requires cross-compiling, you can specify this using the `--target <x>` and `--vmlinux <x>` options:

```sh
cargo r --release --bin chainsmith -- examples/01-macswap-xdp/ --target aarch64-unknown-linux-gnu --vmlinux support-files/vmlinux
```

Similarly, the cargo make targets allow you to do this for any examples:

```sh
cargo make ex-01 --target aarch64-unknown-linux-gnu --vmlinux support-files/vmlinux
```

### Setup

You will need a toolchain with glibc version <= that of your target device.
I found my raspberry pi OS glibc version to be 2.31 via `ldd --version`, so on WSL arch the following sufficed:

```sh
# install glibc for the target OS
sudo pacman -U https://archive.archlinux.org/packages/a/aarch64-linux-gnu-glibc/aarch64-linux-gnu-glibc-2.31-1-any.pkg.tar.zst
# install a version of GCC known to be linked to the selfsame glibc
sudo pacman -U https://archive.archlinux.org/packages/a/aarch64-linux-gnu-gcc/aarch64-linux-gnu-gcc-9.3.0-1-x86_64.pkg.tar.zst
```

You will also need the relevant target for rustc:

```sh
rustup target add aarch64-unknown-linux-gnu
```

Depending on your cross-compiler version, you may also need to pass in flags such as `CFLAGS=-finline-atomics` for linking of libbpf to succeed.
Not all crates will recompile on change to CFLAGS, so this may require a `cargo clean`.
