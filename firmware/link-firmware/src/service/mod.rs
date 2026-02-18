pub mod dev_blinken_light;
pub mod dev_exteth;
pub mod dev_leds;
pub mod dev_oled;
pub mod dev_power_monitor;
pub mod dev_sdcard;
pub mod dev_syseth;
pub mod dev_usb;
pub mod svc_failsafe;
pub mod svc_successful_boot;
// pub mod dev_uart; // TODO
// pub mod dev_usart; // TODO
pub mod svc_cicd;

include!("./_macro.inc.rs");

services! {
					 #[bus(false)]              dev_blinken_light,
					 #[bus(false)] #[rx(false)] dev_exteth,
					 #[bus(false)]              dev_leds,
					 #[bus(false)]              dev_oled,
								   #[rx(false)] dev_power_monitor,
					 #[bus(false)] #[rx(false)] dev_sdcard,
					 #[bus(false)] #[rx(false)] dev_syseth,
					 #[bus(false)] #[rx(false)] dev_usb,
					 #[bus(false)] #[rx(false)] svc_successful_boot,
	#[config(false)] #[bus(false)]              svc_failsafe,
}
