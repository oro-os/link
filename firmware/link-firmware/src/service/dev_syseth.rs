use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_futures::select::select;
use embassy_stm32::{
	exti::ExtiInput,
	gpio::{Output, OutputOpenDrain},
	mode::Async,
	spi::Spi,
};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

pub type WiznetRunner = embassy_net_wiznet::Runner<
	'static,
	embassy_net_wiznet::chip::W5500,
	SpiDevice<'static, NoopRawMutex, Spi<'static, Async>, OutputOpenDrain<'static>>,
	ExtiInput<'static>,
	Output<'static>,
>;

pub type NetRunner = embassy_net::Runner<'static, embassy_net_wiznet::Device<'static>>;

pub struct Config {
	pub wiznet_runner: WiznetRunner,
	pub net_runner:    NetRunner,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Config {
		wiznet_runner,
		mut net_runner,
	} = config;

	select(wiznet_runner.run(), net_runner.run()).await;
	panic!("syseth service ended unexpectedly");
}
