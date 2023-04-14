use std::{
	collections::{BTreeMap, HashMap},
	fmt::Write as _,
	path::PathBuf,
};

use convert_case::{Case, Casing};
use protocol::{EbpfFunction, Function as PFunction, LinkAction, XdpLink, XdpLinkState};
use serde::Deserialize;
use syn::{FnArg, Ident, Item, ReturnType, Type};
use tokio::{
	fs::{self, File},
	io::{self, AsyncWriteExt},
	process::Command,
};
use uuid::Uuid;

use super::error::*;

pub struct FnAnalysis {
	ret_ty: NfReturnType,
	map_ty_name: Option<String>,
}

#[derive(Debug)]
pub enum NfReturnType {
	Enum(String, Vec<String>),
	Empty,
}

impl NfReturnType {
	fn len(&self) -> usize {
		match self {
			Self::Enum(_, v) => v.len(),
			Self::Empty => 1,
		}
	}
}

#[derive(Clone, Debug, Deserialize)]
pub struct Chain {
	pub functions: BTreeMap<String, Function>,
	pub links: Vec<Link>,
	#[serde(default)]
	pub maps: BTreeMap<String, Map>,
}

impl Chain {
	pub async fn generate_xdp_cargo_toml(&self, mut base_dir: PathBuf) -> anyhow::Result<()> {
		let mut deps = vec![];
		let mut bins = vec![];

		for (name, props) in &self.functions {
			if props.disable_xdp {
				continue;
			}

			let path = props.path.as_ref().unwrap_or(name);
			deps.push(format!(
				"{0} = {{ version = \"*\", path = \"../../{1}\", features = [\"xdp\"] }}\n",
				name, path,
			));
			bins.push(format!(
				r#"
[[bin]]
name = "{0}"
path = "src/{0}/main.rs""#,
				name,
			));
			bins.push(format!(
				r#"
[[bin]]
name = "{0}-chain"
path = "src/{0}/chain.rs""#,
				name,
			));
		}

		base_dir.push("Cargo.toml");

		let mut cargo = File::create(&base_dir).await?;

		cargo
			.write_all(include_bytes!("../include/Cargo.xdp.in.toml"))
			.await?;
		for dep in deps {
			cargo.write_all(dep.as_bytes()).await?;
		}

		for bin in bins {
			cargo.write_all(bin.as_bytes()).await?;
		}

		cargo.write_all(b"\n").await?;

		Ok(())
	}

	pub async fn generate_userland_cargo_toml(&self, mut base_dir: PathBuf) -> anyhow::Result<()> {
		base_dir.push("Cargo.toml");

		let mut cargo = File::create(&base_dir).await?;

		cargo
			.write_all("[workspace]\nmembers = [\n".as_bytes())
			.await?;

		for name in self.functions.keys() {
			cargo.write_all(b"\"").await?;
			cargo.write_all(name.as_bytes()).await?;
			cargo.write_all(b"\",\n").await?;
		}

		cargo.write_all("]\n".as_bytes()).await?;

		cargo
			.write_all(
				b"
[profile.release]
strip = true
lto = true
codegen-units = 1
",
			)
			.await?;

		Ok(())
	}

	pub async fn get_nf_return_types(
		&self,
		chain_toml_parent_dir: PathBuf,
	) -> Result<Vec<FnAnalysis>, SourceParseError> {
		let mut fn_retvals = vec![];

		for (name, info) in &self.functions {
			// load source file of crate
			// syn parse
			// check items for 'packet'
			// check return type of packet
			// look for (maybe missing) map type
			// cache enum variants
			// count enum variants, round to POT? as size for table

			let mut librs_dir = chain_toml_parent_dir.clone();
			// path to crate root... relative to above!
			let path = info.path.as_ref().unwrap_or(name);
			librs_dir.push(path);
			librs_dir.push("src/lib.rs");

			let src_file = fs::read_to_string(librs_dir)
				.await
				.map_err(|e| SourceParseError::CrateLibRsRead(name.clone(), e))?;

			let syn = syn::parse_file(&src_file)
				.map_err(|e| SourceParseError::CrateLibRsParse(name.clone(), e))?;

			let mut no_ret = false;

			let packet_fn = syn
				.items
				.iter()
				.filter_map(|el| if let Item::Fn(f) = el { Some(f) } else { None })
				.find(|f| {
					f.sig.ident
						== syn::parse_str::<Ident>("packet")
							.expect("Constructing this ident should be fine.")
				})
				.ok_or_else(|| SourceParseError::MissingPacketFn(name.clone()))?;

			// assumes maps passed by move
			// TODO: maybe also handle annoying Reference type case.
			let map_ty_ident = packet_fn
				.sig
				.inputs
				.iter()
				.nth(1)
				.and_then(|fn_arg| {
					if let FnArg::Typed(a) = fn_arg {
						Some(a)
					} else {
						None
					}
				})
				.and_then(|arg| {
					if let Type::Path(ref p) = *arg.ty {
						Some(p)
					} else {
						None
					}
				})
				.and_then(|val| val.path.segments.last().map(|seg| seg.ident.clone()));
			// .and_then(|val| val.path.get_ident());

			let map_ty_name = map_ty_ident.map(|v| v.to_string());

			let fn_ret_ty_ident = (if let ReturnType::Type(_, ref ty) = packet_fn.sig.output {
				Some(ty)
			} else {
				no_ret = true;
				None
			})
			.and_then(|val| {
				if let Type::Path(ref p) = **val {
					Some(p)
				} else {
					None
				}
			})
			.and_then(|val| val.path.get_ident());

			let ret_ty = if let Some(ident) = fn_ret_ty_ident {
				let enum_item = syn
					.items
					.iter()
					.filter_map(|el| {
						if let Item::Enum(e) = el {
							Some(e)
						} else {
							None
						}
					})
					.find(|el| &(el.ident) == ident)
					.ok_or_else(|| SourceParseError::EnumNotDefinedInRoot {
						nf_name: name.clone(),
						enum_name: ident.to_string(),
					})?;

				let variants = enum_item
					.variants
					.iter()
					.map(|val| val.ident.to_string())
					.collect::<Vec<_>>();

				NfReturnType::Enum(ident.to_string(), variants)
			} else if no_ret {
				NfReturnType::Empty
			} else {
				return Err(SourceParseError::CantResolveReturnType(name.clone()));
			};

			fn_retvals.push(FnAnalysis {
				ret_ty,
				map_ty_name,
			});
		}

		Ok(fn_retvals)
	}

	pub async fn write_xdp_programs(
		&self,
		variants: &Vec<FnAnalysis>,
		src_path: &mut PathBuf,
	) -> Result<(), WriteXdpError> {
		// TODO: map transforms further up the chain so that they can all be included
		// in a fairly generic way? I.e., specify how each adds an entry to toml AND
		// selects a template to interp into.
		for ((name, info), fn_analysis) in self.functions.iter().zip(variants) {
			let canon_name = name.replace('-', "_");

			src_path.push(name);
			fs::create_dir(&src_path)
				.await
				.map_err(WriteXdpError::CreateDir)?;
			src_path.push("main.rs");

			let mut main_file = File::create(&src_path)
				.await
				.map_err(WriteXdpError::CreateFile)?;

			// TODO: define these based on the actual NF definition
			let mut map_defs = String::new();
			let mut map_struct_def = String::new();
			let mut map_param = "";

			// non-empty maps + map type def'd is an err -- lack of each also fine.
			if !(info.maps.is_empty() ^ fn_analysis.map_ty_name.is_none()) {
				let mut fields = vec![];

				for (map_i, (map_name, maybe_data)) in info.maps.iter().enumerate() {
					let data = match maybe_data {
						LocalMap::Owned(m) => m,
						// TODO: handle cleanly (non-panic)
						// FIXME: maybe use the shared str param as a rename mechanism?
						LocalMap::Shared(_) => self.maps.get(map_name).unwrap(),
					};

					let def_name = map_name.to_case(Case::ScreamingSnake);

					data.r#type
						.define_xdp(&mut map_defs, &def_name, map_i, data.size, &canon_name)
						.expect("String append should be infallible.");

					fields.push(format!("{map_name}: &mut {def_name}"));
				}

				if !fields.is_empty() {
					map_struct_def = format!(
						"let chain_map_def = unsafe {{{}::{} {{\n\t\t",
						canon_name,
						fn_analysis.map_ty_name.as_ref().unwrap()
					) + &fields.join(",\n\t\t")
						+ "\n\t}};";

					map_param = " chain_map_def";
				}
			} else {
				// TODO: make err variant somewhere.
				panic!("need both or none of: map type in output, maps assigned to fn");
			}

			main_file
				.write_all(
					format!(
						include_str!("../include/xdp_wrapper.in.rs"),
						canon_name, info.slice, map_defs, map_struct_def, map_param
					)
					.as_bytes(),
				)
				.await
				.map_err(WriteXdpError::WriteFile)?;

			src_path.pop();
			src_path.push("chain.rs");

			let needed_slots = fn_analysis.ret_ty.len().next_power_of_two();

			let my_links = if let Some(ml) = self.links.iter().find(|v| &v.from == name) {
				ml
			} else {
				return Err(WriteXdpError::BranchMismatch {
					nf: name.clone(),
					given_branches: 0,
					needed_branches: fn_analysis.ret_ty.len(),
				});
			};

			match &fn_analysis.ret_ty {
				NfReturnType::Enum(_retval_name, variants) => {
					if variants.len() != my_links.to.len() {
						return Err(WriteXdpError::BranchMismatch {
							nf: name.clone(),
							given_branches: my_links.to.len(),
							needed_branches: variants.len(),
						});
					}
				},
				NfReturnType::Empty => {},
			}

			let mut chain_file = File::create(&src_path)
				.await
				.map_err(WriteXdpError::CreateFile)?;
			chain_file
				.write_all(
					format!(
						include_str!("../include/xdp_wrapper_chain.in.rs"),
						canon_name, info.slice, needed_slots, map_defs, map_struct_def, map_param
					)
					.as_bytes(),
				)
				.await
				.map_err(WriteXdpError::WriteFile)?;

			src_path.pop();
			src_path.pop();
		}

		Ok(())
	}

	pub async fn write_userland_programs(
		&self,
		variants: &Vec<FnAnalysis>,
		src_path: &mut PathBuf,
	) -> Result<(), WriteXdpError> {
		for ((name, info), fn_analysis) in self.functions.iter().zip(variants) {
			let canon_name = name.replace('-', "_");

			src_path.push(name.as_str());
			fs::create_dir(&src_path)
				.await
				.map_err(WriteXdpError::CreateDir)?;

			// --- CARGO ---
			src_path.push("Cargo.toml");
			let mut cargo_file = File::create(&src_path)
				.await
				.map_err(WriteXdpError::CreateFile)?;
			cargo_file
				.write_all(format!(include_str!("../include/Cargo.user.in.toml"), name,).as_bytes())
				.await
				.map_err(WriteXdpError::WriteFile)?;

			let path = info.path.as_ref().unwrap_or(name);
			cargo_file
				.write_all(
					format!(
						"{0} = {{ version = \"*\", path = \"../../../{1}\", features = [\"user\"] }}\n",
						name, path,
					)
					.as_bytes(),
				)
				.await
				.map_err(WriteXdpError::WriteFile)?;
			src_path.pop();
			// --- CARGO ---

			// --- LIB ---
			src_path.push("src");
			fs::create_dir(&src_path)
				.await
				.map_err(WriteXdpError::CreateDir)?;

			src_path.push("lib.rs");
			let mut lib_file = File::create(&src_path)
				.await
				.map_err(WriteXdpError::CreateFile)?;

			// TODO: define these based on the actual NF definition
			let mut map_struct_def = String::new();
			let mut map_param = "";

			// non-empty maps + map type def'd is an err -- lack of each also fine.
			if !(info.maps.is_empty() ^ fn_analysis.map_ty_name.is_none()) {
				let mut fields = vec![];
				let mut defs = vec![];

				for (map_i, (map_name, maybe_data)) in info.maps.iter().enumerate() {
					let _data = match maybe_data {
						LocalMap::Owned(m) => m,
						// TODO: handle cleanly (non-panic)
						// FIXME: maybe use the shared str param as a rename mechanism?
						LocalMap::Shared(_) => self.maps.get(map_name).unwrap(),
					};

					fields.push(format!("{map_name}: m{map_i}"));
					defs.push(format!("m{map_i}"));
				}

				if !fields.is_empty() {
					// Want to have:
					// if let [m0, m1, m2, ..] = maps { .. } else { panic!() }
					map_struct_def = "let chain_map_def = if let [".to_owned()
						+ &defs.join(",") + &format!(
						"] = maps {{ {}::{} {{\n\t\t",
						canon_name,
						fn_analysis.map_ty_name.as_ref().unwrap()
					) + &fields.join(",\n\t\t")
						+ "\n\t} } else { return usize::MAX };";

					map_param = " chain_map_def";
				}
			} else {
				// TODO: make err variant somewhere.
				panic!("need both or none of: map type in output, maps assigned to fn");
			}

			lib_file
				.write_all(
					format!(
						include_str!("../include/lib.in.rs"),
						canon_name, map_struct_def, map_param,
					)
					.as_bytes(),
				)
				.await
				.map_err(WriteXdpError::WriteFile)?;

			src_path.pop();
			src_path.pop();
			// --- LIB ---

			src_path.pop();
		}

		Ok(())
	}

	pub async fn compile_xdp_binaries(
		&self,
		mut src_path: PathBuf,
		vmlinux: &Option<String>,
	) -> Result<(HashMap<Uuid, PFunction>, HashMap<String, Uuid>), CompileError> {
		// TODO: build binaries on Windows
		// ...cross-compile? Still need to target BPF headers of target OS.
		// NOTE: look at redbpf-probes docs, which support this!
		let mut binaries = HashMap::new();
		let mut fn_map = HashMap::new();

		src_path.pop();
		if !cfg!(target_os = "windows") {
			print!("Compiling binaries...");
			let _ = io::stdout().flush().await;

			let mut envs = HashMap::new();
			if let Some(vmlinux_loc) = vmlinux {
				envs.insert("ENV_VMLINUX_PATH", vmlinux_loc.clone());
			}

			let o = Command::new("cargo")
				.args(["+1.59", "bpf", "build", "--target-dir", "../../target"])
				.current_dir(&src_path)
				.envs(envs)
				.output()
				.await
				.map_err(CompileError::CallCompile)?;

			if !o.status.success() {
				return Err(CompileError::DoCompile(o));
			}

			println!(" Done!");

			src_path.pop();
			src_path.pop();
			src_path.push("target/bpf/programs");
			for (name, props) in self.functions.iter() {
				let uuid = uuid::Uuid::new_v4();

				let ebpf = if props.disable_xdp {
					None
				} else {
					let elf_dir = format!("{name}.elf");
					let chain_name = format!("{name}-chain");
					let chain_elf = format!("{chain_name}.elf");

					// can this be abstracted away? in line w/ above?
					// i.e., early-ident

					src_path.push(name);
					src_path.push(elf_dir);
					let end = fs::read(&src_path)
						.await
						.map_err(|e| CompileError::ReadElf(name.clone(), e))?;
					src_path.pop();
					src_path.pop();

					src_path.push(&chain_name);
					src_path.push(chain_elf);
					let link = fs::read(&src_path)
						.await
						.map_err(|e| CompileError::ReadElf(name.clone(), e))?;
					src_path.pop();
					src_path.pop();

					Some(EbpfFunction { link, end })
				};

				let fun = PFunction {
					uuid,
					elf: None,
					ebpf,
				};

				fn_map.insert(name.clone(), uuid);
				binaries.insert(uuid, fun);
			}
		} else {
			eprintln!("Skipping eBPF NF compilation.");
			for name in self.functions.keys() {
				let uuid = uuid::Uuid::new_v4();

				let ebpf = Some(EbpfFunction {
					link: vec![],
					end: vec![],
				});

				let fun = PFunction {
					uuid,
					elf: None,
					ebpf,
				};

				fn_map.insert(name.clone(), uuid);
				binaries.insert(uuid, fun);
			}
		}

		Ok((binaries, fn_map))
	}

	pub async fn compile_userland_binaries(
		&self,
		mut workspace_path: PathBuf,
		binaries: &mut HashMap<Uuid, PFunction>,
		fn_map: &mut HashMap<String, Uuid>,
		target: &Option<String>,
	) -> Result<(), CompileError> {
		if !cfg!(target_os = "windows") {
			print!("Compiling binaries...");
			let _ = io::stdout().flush().await;

			let mut extra_args = vec![];
			let mut extra_env = HashMap::new();
			if let Some(target) = target {
				extra_args.push("--target");
				extra_args.push(target);

				let compiler_triple = match target.as_str() {
					env!("TARGET") => None,
					"aarch64-unknown-linux-gnu" => Some("aarch64-linux-gnu"),
					_ => panic!("Cannot currently cross compile to this target."),
				};

				if let Some(triple) = compiler_triple {
					extra_env.insert("RUSTFLAGS", format!("-C linker={triple}-gcc"));
				}
			}

			let o = Command::new("cargo")
				.args(["build", "--release", "--target-dir", "../../target"])
				.args(extra_args)
				.envs(extra_env)
				.current_dir(&workspace_path)
				.output()
				.await
				.map_err(CompileError::CallCompile)?;

			if !o.status.success() {
				return Err(CompileError::DoCompile(o));
			}

			println!(" Done!");

			workspace_path.pop();
			workspace_path.pop();
			workspace_path.push("target");

			if let Some(target) = target {
				workspace_path.push(target);
			}

			workspace_path.push("release");

			// target/release/lib{name}_user.so

			// TODO: WRITE INTO BINARIES, FN_MAP -- check write dest.
			for name in self.functions.keys() {
				let dylib_path = format!("lib{}_user.so", name.replace('-', "_"));

				// can this be abstracted away? in line w/ above?
				// i.e., early-ident

				workspace_path.push(dylib_path);
				let dylib = fs::read(&workspace_path)
					.await
					.map_err(|e| CompileError::ReadElf(name.clone(), e))?;
				workspace_path.pop();

				let uuid = fn_map
					.entry(name.clone())
					.or_insert_with(uuid::Uuid::new_v4);
				binaries
					.entry(*uuid)
					.or_insert(PFunction {
						uuid: *uuid,
						elf: None,
						ebpf: None,
					})
					.elf = Some(dylib);
			}
		} else {
			eprintln!("Skipping userland NF compilation.");
		}
		Ok(())
	}

	pub fn make_concrete(
		&self,
		fn_map: &HashMap<String, Uuid>,
	) -> Result<Vec<XdpLink>, ChainBuildError> {
		let mut new_fns: HashMap<&String, XdpLink> = fn_map
			.iter()
			.map(|(name, uuid)| {
				(
					name,
					XdpLink {
						uuid: *uuid,
						state: XdpLinkState::Body(vec![]),
						root: false,
						disable_xdp: self.functions.get(name).unwrap().disable_xdp,
						map_names: self.functions[name].maps.keys().cloned().collect(),
					},
				)
			})
			.collect();

		let mut rx_recv_count = 0;
		for link in &self.links {
			if link.from.as_str() == "rx" {
				// if source is rx, make dest root.
				for dest in &link.to {
					let t_dest = dest.clone();
					let first_hop = new_fns.get_mut(&t_dest).ok_or_else(|| {
						ChainBuildError::UndefinedTarget(link.clone(), dest.clone())
					})?;
					first_hop.root = true;
					rx_recv_count += 1;
				}
			} else {
				let (tail, dests) = if link.to.len() == 1 && link.to[0].as_str() == "tx" {
					// source_link.state = XdpLinkState::Tail;
					(true, vec![])
				} else {
					let dest_links: Result<Vec<LinkAction>, ChainBuildError> = link
						.to
						.iter()
						.map(|name| {
							let force_upcall = name.starts_with('!');

							new_fns
								.get(&name[(force_upcall as usize)..].to_string())
								.ok_or_else(|| ChainBuildError::UndefinedTarget(link.clone(), name.clone()))
								// TODO: figure out when to upcall?
								.map(|link| if force_upcall || link.disable_xdp { LinkAction::Upcall } else { LinkAction::Tailcall }(link.uuid) )
								// translate special names to actions
								.or_else(|e| {
									if let ChainBuildError::UndefinedTarget(_, ref s) = e {
										match s.as_str() {
											"tx" => Ok(LinkAction::Tx),
											"drop" => Ok(LinkAction::Drop),
											"pass" => Ok(LinkAction::Pass),
											"abort" => Ok(LinkAction::Abort),
											_ => Err(e),
										}
									} else {
										unreachable!()
									}
								})
						})
						.collect();

					(false, dest_links?)
				};

				let source_link = new_fns.get_mut(&&link.from).ok_or_else(|| {
					ChainBuildError::UndefinedSource(link.clone(), link.from.clone())
				})?;

				if tail {
					source_link.state = XdpLinkState::Tail;
				} else if let XdpLinkState::Body(ref mut ids) = &mut source_link.state {
					*ids = dests;
				}
			}

			if rx_recv_count > 1 {
				return Err(ChainBuildError::TooManyRxHandlers);
			}
		}

		if rx_recv_count == 0 {
			return Err(ChainBuildError::NoRxHandler);
		}

		Ok(new_fns.into_values().collect())
	}
}

#[derive(Clone, Debug, Deserialize)]
pub struct Function {
	/// Override path to crate.
	///
	/// If `None`, the key attached to this entry in [`Chain::functions`] is
	/// used instead.
	pub path: Option<String>,
	#[serde(default)]
	pub disable_xdp: bool,
	pub slice: Option<usize>,
	#[serde(default)]
	pub maps: BTreeMap<String, LocalMap>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case", untagged)]
pub enum LocalMap {
	Shared(String),
	Owned(Map),
}

#[derive(Clone, Debug, Deserialize)]
pub struct Map {
	pub r#type: MapType,
	pub size: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MapType {
	Array,
	HashMap,
}

impl MapType {
	pub fn define_xdp(
		&self,
		target: &mut String,
		def_name: &str,
		map_i: usize,
		map_sz: u64,
		canon_name: &str,
	) -> std::fmt::Result {
		writeln!(target, "#[map(link_section = \"maps\")]")?;
		write!(target, "static mut {def_name}: ")?;

		match self {
			Self::Array => writeln!(
				target,
				"Array<{2}::NfValTy{0}> = Array::with_max_entries({1});",
				map_i, map_sz, canon_name
			),
			Self::HashMap => writeln!(
				target,
				"HashMap<{2}::NfKeyTy{0}, {2}::NfValTy{0}> = HashMap::with_max_entries({1});",
				map_i, map_sz, canon_name
			),
		}
	}
}

#[derive(Clone, Debug, Deserialize)]
pub struct Link {
	pub from: String,
	pub to: Vec<String>,
}
