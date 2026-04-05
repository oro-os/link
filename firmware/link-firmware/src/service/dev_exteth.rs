use embassy_futures::select::select;
use embassy_stm32::{
	exti::ExtiInput,
	gpio::OutputOpenDrain,
	mode::Async,
	spi::{Spi, mode::Master},
};
use embedded_hal_bus::spi::ExclusiveDevice;

pub type WiznetRunner = embassy_net_wiznet::Runner<
	'static,
	embassy_net_wiznet::chip::W5500,
	ExclusiveDevice<Spi<'static, Async, Master>, OutputOpenDrain<'static>, embassy_time::Delay>,
	ExtiInput<'static, Async>,
	OutputOpenDrain<'static>,
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
	panic!("exteth service ended unexpectedly");
}
