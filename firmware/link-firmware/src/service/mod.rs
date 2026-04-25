pub mod dev_blinken_light;
pub mod dev_exteth;
pub mod dev_leds;
pub mod dev_oled;
pub mod dev_power_monitor;
pub mod dev_sdcard;
pub mod dev_syseth;
pub mod dev_usb;
// pub mod dev_usart; // TODO
// pub mod dev_uart; // TODO
pub mod failsafe_aux_vbus_oc;
pub mod failsafe_board_oc;
pub mod failsafe_sd_oc;
pub mod failsafe_ulpi_oc;
pub mod svc_leds;
pub mod svc_main;
pub mod svc_oled;
pub mod svc_oled_pwr;
pub mod svc_psu;
pub mod svc_qup;
pub mod svc_vbus_power;
pub mod svc_wol;

include!("./_macro.inc.rs");

#[rustfmt::skip]
services! {
                     #[bus(false)]                            dev_blinken_light,
                     #[bus(false)] #[rx(false)]               dev_exteth,
                     #[bus(false)]                            dev_leds,
                     #[bus(false)]                            dev_oled,
                     #[bus(false)] #[rx(false)]               dev_power_monitor,
                     #[bus(false)] #[rx(false)] #[skip(true)] dev_sdcard,
                     #[bus(false)] #[rx(false)] #[skip(true)] dev_syseth,
                     #[bus(false)] #[rx(false)]               dev_usb,
                                   #[rx(false)]               svc_main,
    #[config(false)]                                          svc_oled_pwr,
    #[config(false)]                                          svc_oled,
    #[config(false)]                                          svc_leds,
                                                              svc_vbus_power,
                     #[bus(false)]                            svc_psu,
    #[config(false)]                                          svc_wol,
                     #[bus(false)] #[rx(false)]               svc_qup,
                     #[bus(false)] #[rx(false)]               failsafe_board_oc,
                     #[bus(false)] #[rx(false)]               failsafe_aux_vbus_oc,
                     #[bus(false)] #[rx(false)]               failsafe_ulpi_oc,
                     #[bus(false)] #[rx(false)]               failsafe_sd_oc,
}

#[macro_export]
macro_rules! bus {
	($bus:expr, $service:ident, $cmd:ident{$($tt:tt)*} $(,)?) => (
		$bus.$service.send($crate::service::$service::Cmd::$cmd{$($tt)*}).await
	);

	($bus:expr, $service:ident, $cmd:ident($($tt:tt)*) $(,)?) => (
		$bus.$service.send($crate::service::$service::Cmd::$cmd($($tt)*)).await
	);

	($bus:expr, $service:ident, $cmd:ident $(,)?) => (
		$bus.$service.send($crate::service::$service::Cmd::$cmd).await
	);
}
