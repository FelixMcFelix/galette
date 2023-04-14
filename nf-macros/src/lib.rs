use proc_macro::TokenStream;
use quote::{format_ident, quote, ToTokens};
use syn::{Fields, GenericParam, ItemStruct, Type, WhereClause, WherePredicate};

#[proc_macro_attribute]
/// Expands an input struct to replace all `(K,V)` pairs into appropriate Maps.
/// Also re-exports all key and value types at pub-level.
pub fn maps(_attr: TokenStream, item: TokenStream) -> TokenStream {
	let mut in_struct: ItemStruct =
		syn::parse(item).expect("`maps` attribute macro can only be used to transform a `struct`.");

	let mut xs = vec![];

	match &mut in_struct.fields {
		Fields::Named(fields) =>
			for (i, entry) in fields.named.iter_mut().enumerate() {
				let inner_types = if let Type::Tuple(tup) = &mut entry.ty {
					tup.elems.clone()
				} else {
					panic!(
						"Field {} is not a tuple type.",
						entry.ident.to_token_stream()
					)
				};

				let ident = format_ident!("NfMapField{}", i);
				entry.ty = Type::Verbatim(quote! {#ident});

				entry.vis = syn::parse_quote! {pub};

				xs.push((ident, inner_types));
			},
		Fields::Unnamed(_fields) => panic!("Fields on map struct must be named."),
		Fields::Unit => panic!("Unit Map structs are not accepted."),
	}

	if in_struct.generics.where_clause.is_none() {
		in_struct.generics.where_clause = Some(WhereClause {
			where_token: syn::parse_str("where").unwrap(),
			predicates: Default::default(),
		})
	}

	let mut redecls = vec![];

	for (i, (ident, kv_types)) in xs.iter().enumerate() {
		in_struct
			.generics
			.params
			.push(GenericParam::Type(ident.clone().into()));

		// Add re-exports to the list of Things We Need To Write.
		for (ty, prefix) in kv_types.iter().take(2).zip(["NfKeyTy", "NfValTy"]) {
			let my_new_ident = format_ident!("{}{}", prefix, i);
			redecls.push(quote! {pub type #my_new_ident = #ty;});
		}

		let where_clause = in_struct.generics.where_clause.as_mut().unwrap();
		where_clause.predicates.push(
			syn::parse_str::<WherePredicate>(&format!(
				"{}: Map<{}>",
				ident,
				kv_types.to_token_stream()
			))
			.unwrap(),
		);
	}

	let mut out = in_struct.into_token_stream();

	for el in redecls {
		el.to_tokens(&mut out);
	}

	out.into()
}
