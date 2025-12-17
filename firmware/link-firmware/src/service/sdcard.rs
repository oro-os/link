use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_stm32::{
	exti::ExtiInput,
	gpio::{Output, OutputOpenDrain},
	mode::Async,
	spi::Spi,
};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::{Duration, Timer};

#[embassy_executor::task]
pub async fn sdcard_service(
	mut sd: SpiDevice<'static, NoopRawMutex, Spi<'static, Async>, OutputOpenDrain<'static>>,
	mut sd_en: Output<'static>,
	mut sd_oc: ExtiInput<'static>,
	mut sd_sense: ExtiInput<'static>,
	mut sd_sense_cable: ExtiInput<'static>,
	mut sd_host_sut_sel: Output<'static>,
) -> ! {
	loop {
		sd_sense.wait_for_high().await;
		defmt::info!("SD card inserted");
		sd_sense.wait_for_low().await;
		defmt::info!("SD card removed");
	}
}
