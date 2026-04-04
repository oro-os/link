pub mod dev_blinken_light;
pub mod dev_exteth;
pub mod dev_leds;
pub mod dev_oled;
pub mod dev_power_monitor;
pub mod dev_sdcard;
pub mod dev_syseth;
pub mod dev_usb;
pub mod svc_failsafe;
// pub mod dev_usart; // TODO
// pub mod dev_uart; // TODO
pub mod svc_main;
pub mod svc_mqtt;
pub mod svc_oled;
pub mod svc_oled_pwr;

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
                     #[bus(false)] #[rx(false)] #[skip(true)] dev_usb,
    #[config(false)] #[bus(false)]              #[skip(true)] svc_failsafe,
                                   #[rx(false)]               svc_main,
    #[config(false)]                                          svc_oled_pwr,
    #[config(false)]                                          svc_oled,
                     #[bus(false)] #[rx(false)]               svc_mqtt,
}
