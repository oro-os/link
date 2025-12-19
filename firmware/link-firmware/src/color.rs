#[derive(defmt::Format, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
	#[allow(unused)]
	pub r: u8,
	#[allow(unused)]
	pub g: u8,
	#[allow(unused)]
	pub b: u8,
}

impl Rgb {
	#[allow(unused)]
	pub const fn new(r: u8, g: u8, b: u8) -> Self {
		Self { r, g, b }
	}

	#[allow(unused)]
	pub const fn hex(mut hex: u32) -> Self {
		let b = (hex & 0xFF) as u8;
		hex >>= 8;
		let g = (hex & 0xFF) as u8;
		hex >>= 8;
		let r = (hex & 0xFF) as u8;
		Self { r, g, b }
	}

	#[allow(unused)]
	pub const fn grey(scale: u8) -> Self {
		Self {
			r: scale,
			g: scale,
			b: scale,
		}
	}

	#[allow(unused)]
	pub const fn lerp(&self, other: Rgb, t: f32) -> Rgb {
		let r = self.r as f32 + (other.r as f32 - self.r as f32) * t;
		let g = self.g as f32 + (other.g as f32 - self.g as f32) * t;
		let b = self.b as f32 + (other.b as f32 - self.b as f32) * t;
		Rgb {
			r: r.clamp(0.0, 255.0) as u8,
			g: g.clamp(0.0, 255.0) as u8,
			b: b.clamp(0.0, 255.0) as u8,
		}
	}

	pub fn white_component(&self) -> u8 {
		self.r.min(self.g).min(self.b)
	}

	pub fn without_white_component(&self) -> Rgb {
		let w = self.white_component();
		Rgb {
			r: self.r.saturating_sub(w),
			g: self.g.saturating_sub(w),
			b: self.b.saturating_sub(w),
		}
	}
}

impl From<(u8, u8, u8)> for Rgb {
	fn from(components: (u8, u8, u8)) -> Self {
		Self {
			r: components.0,
			g: components.1,
			b: components.2,
		}
	}
}

impl From<Rgb> for (u8, u8, u8) {
	fn from(color: Rgb) -> Self {
		(color.r, color.g, color.b)
	}
}

impl From<Rgb> for (Option<u8>, Option<u8>, Option<u8>) {
	fn from(color: Rgb) -> Self {
		(
			(color.r != 0).then_some(color.r),
			(color.g != 0).then_some(color.g),
			(color.b != 0).then_some(color.b),
		)
	}
}

#[allow(unused)]
pub const BLACK: Rgb = Rgb::grey(0);
#[allow(unused)]
pub const WHITE: Rgb = Rgb::grey(255);
#[allow(unused)]
pub const RED: Rgb = Rgb::new(255, 0, 0);
#[allow(unused)]
pub const GREEN: Rgb = Rgb::new(0, 255, 0);
#[allow(unused)]
pub const BLUE: Rgb = Rgb::new(0, 0, 255);
#[allow(unused)]
pub const YELLOW: Rgb = Rgb::new(255, 255, 0);
#[allow(unused)]
pub const CYAN: Rgb = Rgb::new(0, 255, 255);
#[allow(unused)]
pub const MAGENTA: Rgb = Rgb::new(255, 0, 255);
