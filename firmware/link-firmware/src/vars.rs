use heapless::String;
use qup_embassy::{Key, Perm};

#[macro_export]
macro_rules! vars {
	($($idx:literal => $perm:tt<$T:ty> $id:ident($name:literal)),* $(,)?) => {
		$(
			pub static $id: Key<$T, {Perm::$perm}> = Key::new($name, $idx);
		)*

		macro_rules! run_qup_for_all_vars {
			($listener:expr) => (
				qup_embassy::run!($listener, $crate::unique_id(), [
					$(&$crate::vars::$id),*
				])
			)
		}

		pub(crate) use run_qup_for_all_vars;
	}
}

vars! {
	0 => RN<i64> STAT_BOARD_CURRENT_MA("board_current_ma"),
	1 => RN<i64> STAT_BOARD_CURRENT_UA("board_current_ua"),
	2 => RN<String<8>> STAT_VBUS_POWER_STATE("vbus_power_state"),
	3 => RN<String<4>> STAT_OLED_POWER_STATE("oled_power_state"),
	4 => RN<String<4>> STAT_OLED_POWER_TARGET_STATE("oled_power_target_state"),
	5 => R<i64> STAT_BOARD_OC_MA("board_failsafe_oc_ma"),
	6 => RN<String<16>> STAT_BLINKEN_LIGHT_CMD("blinken_light_cmd"),
	7 => RN<bool> STAT_OLED_POWER_VREG("oled_power_vreg"),
	8 => RN<i64> STAT_OLED_BRIGHTNESS("oled_brightness"),
	9 => RN<bool> STAT_LEDS_CHIP_ENABLED("leds_chip_enabled"),
	10 => RN<String<16>> STAT_LEDS_STATE("leds_state"),
	11 => RN<String<16>> STAT_LEDS_TARGET_STATE("leds_target_state"),
	12 => RN<bool> STAT_PSU_ON("psu_on"),
	13 => RN<bool> STAT_INITIALIZED("initialized"),
	14 => R<i64> STAT_VERSION_MAJOR("version_major"),
	15 => R<i64> STAT_VERSION_MINOR("version_minor"),
	16 => R<i64> STAT_VERSION_PATCH("version_patch"),
	17 => R<String<128>> STAT_LAST_BOOT_FAILURE("last_boot_failure"),
	18 => R<bool> STAT_AUX_VBUS_SENSE("aux_vbus_sense"),
	19 => RWN<bool> CFG_PR_RUN("pr_run"),
	20 => RWN<String<128>> CFG_PR_TITLE("pr_title"),
	21 => RWN<String<128>> CFG_PR_AUTHOR("pr_author"),
	22 => RWN<i64> CFG_PR_NUMBER("pr_number"),
	23 => RWN<PowerType> CFG_SUT_POWER_TYPE("sut_power_type"),
	24 => W<bool> CFG_CONFIGURED("configured"),
	25 => RWN<Wol> CFG_WOL("wol"),
	26 => RWN<UsbIface> CFG_SUT_USB_IFACE("sut_usb_iface"),
	27 => RWN<BootSource> CFG_SUT_BOOT_SOURCE("sut_boot_source"),
	28 => RWN<bool> CFG_SUT_REQUIRE_4A_VBUS("sut_require_4a_vbus"),
}

#[derive(Clone, Copy, PartialEq, Eq, defmt::Format, qup_embassy::Value)]
pub enum PowerType {
	Usb,
	UsbVbus,
	#[qup(default)]
	Psu,
}

#[derive(Clone, Copy, PartialEq, Eq, defmt::Format, qup_embassy::Value)]
pub enum Wol {
	#[qup(default, name = "never")]
	Off,
	#[qup(name = "5m")]
	Mins5,
	#[qup(name = "10m")]
	Mins10,
	#[qup(name = "30m")]
	Mins30,
}

#[derive(Clone, Copy, PartialEq, Eq, defmt::Format, qup_embassy::Value)]
pub enum UsbIface {
	#[qup(default)]
	Port,
	Header,
}

#[derive(Clone, Copy, PartialEq, Eq, defmt::Format, qup_embassy::Value)]
pub enum BootSource {
	#[qup(name = "usb")]
	UsbMsd,
	#[qup(default)]
	Sd,
}
