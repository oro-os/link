extern crate proc_macro;

use proc_macro2::{Span, TokenStream};
use quote::{TokenStreamExt, quote};
use std::collections::HashMap;
use syn::{
	Error, Fields, Ident, ItemEnum, LitInt, Meta,
	parse::{Parse, ParseStream},
	parse_macro_input,
	punctuated::Punctuated,
	token::{Comma, Eq},
};

#[derive(Default)]
struct ProtoMeta {
	id: Option<u8>,
}

enum ProtoMetaKV {
	Id(u8),
}

impl Parse for ProtoMetaKV {
	fn parse(input: ParseStream<'_>) -> Result<Self, Error> {
		let ident: Ident = input.parse()?;
		let _: Eq = input.parse()?;
		match ident.to_string().as_str() {
			"id" => {
				let n: LitInt = input.parse()?;
				Ok(ProtoMetaKV::Id(n.base10_parse::<u8>()?))
			}
			_ => Err(Error::new(
				ident.span(),
				"unknown link protocol `proto()` field",
			)),
		}
	}
}

impl Parse for ProtoMeta {
	fn parse(input: ParseStream<'_>) -> Result<Self, Error> {
		let kvs = Punctuated::<ProtoMetaKV, Comma>::parse_terminated(input)?;
		let mut meta = ProtoMeta::default();

		for kv in kvs {
			match kv {
				ProtoMetaKV::Id(id) => {
					meta.id = Some(id);
				}
			}
		}

		Ok(meta)
	}
}

fn paste<A: ToString, B: ToString>(a: &A, b: &B) -> Ident {
	let a = a.to_string();
	let b = b.to_string();
	Ident::new(&format!("{a}{b}"), Span::call_site())
}

#[proc_macro_derive(LinkMessage, attributes(proto))]
pub fn derive_link_protocol_message(item: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let ast = parse_macro_input!(item as ItemEnum);

	let enum_ident = ast.ident.clone();

	let mut known_discriminants = HashMap::<u8, Ident>::new();

	let mut serialize_matches = Vec::new();
	let mut deserialize_matches = Vec::new();

	for variant in ast.variants {
		let ident = variant.ident;

		let mut serialize_statements = Vec::new();

		let mut proto = None;
		for attr in variant.attrs {
			if let Meta::List(attr) = attr.meta {
				if attr.path.segments.len() == 1 && attr.path.segments[0].ident == "proto" {
					match attr.parse_args_with(ProtoMeta::parse) {
						Ok(p) => {
							proto = Some(p);
						}
						Err(err) => return err.into_compile_error().into(),
					}
				}
			}
		}

		let proto = match proto {
			Some(p) => p,
			None => {
				return Error::new(
					ident.span(),
					"oro link protocol enum variant missing #[proto(id = ...)] attribute",
				)
				.into_compile_error()
				.into();
			}
		};

		let discriminant = match proto.id {
			Some(id) => id,
			None => {
				return Error::new(
					ident.span(),
					"link protocol enum variant #[proto] attribute is missing required `id`",
				)
				.into_compile_error()
				.into();
			}
		};

		if let Some(existing_ident) = known_discriminants.get(&discriminant) {
			let existing_ident = existing_ident.to_string();
			return Error::new(
				ident.span(),
				format!(
					"link protocol enum variant has identical `id` as another variant `{existing_ident}`"
				),
			)
			.into_compile_error()
			.into();
		}

		if discriminant == 0 {
			return Error::new(
				ident.span(),
				"link protocol enum variant discriminants cannot be zero (0)",
			)
			.into_compile_error()
			.into();
		}

		known_discriminants.insert(discriminant, ident.clone());

		serialize_statements.push(quote! {
			<u8 as ::link_protocol_binser::Serialize>::serialize(&#discriminant, writer).await?;
		});

		let (destructure, construction) = match variant.fields {
			Fields::Named(named) => {
				let mut field_inits = Vec::new();
				let mut field_idents = Punctuated::<Ident, Comma>::new();

				for field in named.named {
					let ident = field.ident.unwrap();
					let fieldtype = &field.ty;

					field_idents.push(ident.clone());

					serialize_statements.push(quote! {
						::link_protocol_binser::Serialize::serialize(#ident, writer).await?;
					});

					field_inits.push(quote! {
						#ident : <(#fieldtype) as ::link_protocol_binser::Deserialize>::deserialize(reader).await?,
					});
				}

				let mut field_inits_stream = TokenStream::new();
				field_inits_stream.append_all(field_inits.into_iter());

				(
					quote! {
						{#field_idents}
					},
					quote! {
						{#field_inits_stream}
					},
				)
			}
			Fields::Unnamed(fields) => {
				let mut field_inits = Vec::new();
				let mut field_idents = Punctuated::<Ident, Comma>::new();

				for (i, field) in fields.unnamed.iter().enumerate() {
					let ident = paste(&"f", &i);
					let fieldtype = &field.ty;

					field_idents.push(ident.clone());

					serialize_statements.push(quote! {
						::link_protocol_binser::Serialize::serialize(#ident, writer).await?;
					});

					field_inits.push(quote! {
						<(#fieldtype) as ::link_protocol_binser::Deserialize>::deserialize(reader).await?,
					});
				}

				let mut field_inits_stream = TokenStream::new();
				field_inits_stream.append_all(field_inits.into_iter());

				(
					quote! {
						(#field_idents)
					},
					quote! {
						(#field_inits_stream)
					},
				)
			}
			Fields::Unit => (quote! {}, quote! {}),
		};

		let mut serialize_statements_stream = TokenStream::new();
		serialize_statements_stream.append_all(serialize_statements.into_iter());

		serialize_matches.push(quote! {
			#enum_ident :: #ident #destructure => {
				#serialize_statements_stream
			}
		});

		deserialize_matches.push(quote! {
			#discriminant => {
				#enum_ident :: #ident #construction
			}
		});
	}

	let ident = ast.ident;
	let (generics_pre, generics_mid, generics_post) = ast.generics.split_for_impl();

	let mut serialize_matches_stream = TokenStream::new();
	serialize_matches_stream.append_all(serialize_matches);
	let mut deserialize_matches_stream = TokenStream::new();
	deserialize_matches_stream.append_all(deserialize_matches);

	quote! {
		const _: () = {
			#[automatically_derived]
			impl #generics_pre ::link_protocol_binser::Serialize for #ident #generics_mid #generics_post {
				async fn serialize<W: ::link_protocol_binser::Write>(&self, writer: &mut W) -> Result<(), ::link_protocol_binser::Error<W::Error>> {
					Ok(match self {
						#serialize_matches_stream
					})
				}
			}

			#[automatically_derived]
			impl #generics_pre ::link_protocol_binser::Deserialize for #ident #generics_mid #generics_post {
				async fn deserialize<R: ::link_protocol_binser::Read>(reader: &mut R) -> Result<Self, ::link_protocol_binser::Error<R::Error>> {
					let msg_code = <u8 as ::link_protocol_binser::Deserialize>::deserialize(reader).await?;

					Ok(
						match msg_code {
							#deserialize_matches_stream
							unknown => {
								return Err(::link_protocol_binser::Error::InvalidMessageCode(unknown));
							}
						}
					)
				}
			}
		};
	}
	.into()
}
