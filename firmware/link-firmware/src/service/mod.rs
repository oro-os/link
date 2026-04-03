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
// pub mod dev_usart; // TODO
pub mod dev_uart;
pub mod svc_cicd;
pub mod svc_daemon;
pub mod svc_init;
pub mod svc_main;
pub mod svc_oled;
pub mod svc_oled_pwr;
pub mod svc_uart;

include!("./_macro.inc.rs");

#[rustfmt::skip]
services! {
                     #[bus(false)]                            dev_blinken_light,
                     #[bus(false)] #[rx(false)]               dev_exteth,
                     #[bus(false)]                            dev_leds,
                     #[bus(false)]                            dev_oled,
                                   #[rx(false)] #[skip(true)] dev_power_monitor,
                     #[bus(false)] #[rx(false)] #[skip(true)] dev_sdcard,
                     #[bus(false)] #[rx(false)]               dev_syseth,
                     #[bus(false)]                            dev_uart,
                     #[bus(false)] #[rx(false)] #[skip(true)] dev_usb,
                     #[bus(false)] #[rx(false)] #[skip(true)] svc_successful_boot,
    #[config(false)] #[bus(false)]              #[skip(true)] svc_failsafe,
    #[config(false)]               #[rx(false)]               svc_cicd,
                                   #[rx(false)]               svc_main,
                                                              svc_init,
    #[config(false)]                                          svc_oled_pwr,
    #[config(false)]                                          svc_oled,
    #[config(false)]                                          svc_uart,
	                                                          svc_daemon,
}
