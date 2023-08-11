use pixglyph::Glyph;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use std::{env, fs, path::Path};
use syn::{punctuated::Punctuated, token::Comma, Ident, LitChar, LitInt};
use ttf_parser::Face;

const TERM_CHARMAP: &str = "?abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!@#$%^&*()`,.<>/[]-_~'\"=+;:©µ¿{}ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÐÑÒÓÔÕÖ×ØÙÚÛÜÝÞßàáâãäåæçèéêëìíîïðñòóôõö÷øùúûüýþÿ";
const HEIGHT: f32 = 16.0;
const BASE_TOP: f32 = HEIGHT - 1.0;
const BASE_LEFT: f32 = 1.0;
const THRESHOLD: u8 = 250;

fn render_font(id: &str, path: &str, charmap: &str) -> TokenStream {
	let face_data = fs::read(path).unwrap();
	let face = Face::parse(&face_data[..], 0).unwrap();
	println!("cargo:rerun-if-changed={}", path);

	let mut charmap = charmap.chars().collect::<Vec<_>>();
	charmap.sort();
	let mut char_syn = Punctuated::<LitChar, Comma>::new();
	let mut idx_syn = Punctuated::<LitInt, Comma>::new();
	let mut bytes_syn = Punctuated::<LitInt, Comma>::new();

	for c in charmap {
		char_syn.push(LitChar::new(c, Span::call_site()));
		idx_syn.push(LitInt::new(&bytes_syn.len().to_string(), Span::call_site()));

		let glyph_normal = face
			.glyph_index(c)
			.unwrap_or_else(|| panic!("normal face has no glyph for character {}: {c}", c as u16));

		let glyph = Glyph::load(&face, glyph_normal).unwrap();
		let bmp = glyph.rasterize(BASE_LEFT, BASE_TOP, HEIGHT);

		let total_bytes = (((bmp.width * bmp.height) + 7) / 8) as usize;
		let mut bytes = vec![0u8; total_bytes];

		for y in 0..bmp.height {
			for x in 0..bmp.width {
				let index = (y * bmp.width + x) as usize;
				let byte_index = index / 8;
				let shift = index % 8;
				let bitval = ((bmp.coverage[index] >= THRESHOLD) as u8) << shift;
				bytes[byte_index] |= bitval;
			}
		}

		assert!(bmp.width <= 255);
		assert!(bmp.height <= 255);
		assert!(bmp.top <= 255);

		let width = bmp.width as u8;
		let height = bmp.height as u8;
		let top = bmp.top as u8;

		bytes_syn.push(LitInt::new(&top.to_string(), Span::call_site()));
		bytes_syn.push(LitInt::new(&width.to_string(), Span::call_site()));
		bytes_syn.push(LitInt::new(&height.to_string(), Span::call_site()));

		for b in bytes {
			bytes_syn.push(LitInt::new(&b.to_string(), Span::call_site()));
		}
	}

	let id_syn = Ident::new(id, Span::call_site());
	let num_chars = char_syn.len();
	let num_bytes = bytes_syn.len();

	quote! {
		pub struct #id_syn;
		impl Font<#num_chars, #num_bytes> for #id_syn {
			const CHARMAP: [char; #num_chars] = [ #char_syn ];
			const IDXMAP: [u16; #num_chars] = [ #idx_syn ];
			const DATA: [u8; #num_bytes] = [ #bytes_syn ];
		}
	}
}

pub fn main() {
	let mut font_source = quote! { use crate::font::Font; };
	font_source
		.extend(render_font("TermNormal", "font/EnterCommand.ttf", TERM_CHARMAP).into_iter());
	font_source
		.extend(render_font("TermBold", "font/EnterCommand-Bold.ttf", TERM_CHARMAP).into_iter());

	let source = font_source.to_string();

	let out_dir = env::var_os("OUT_DIR").unwrap();
	let dest_path = Path::new(&out_dir).join("oro-link-fontdata.rs");
	fs::write(dest_path, source).unwrap();

	println!("cargo:rerun-if-changed=build.rs");
}
