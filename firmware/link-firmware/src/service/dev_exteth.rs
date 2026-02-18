use embassy_futures::select::select3;
use embassy_stm32::{exti::ExtiInput, gpio::OutputOpenDrain, mode::Async, spi::Spi};
use embassy_time::{Delay, Duration, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;

pub struct Config {
	pub driver: Spi<'static, Async>,
	pub cs:     OutputOpenDrain<'static>,
	pub rst:    OutputOpenDrain<'static>,
	pub exti:   ExtiInput<'static>,
	pub seed:   u64,
}

#[embassy_executor::task]
pub async fn run(config: Config) -> ! {
	let Config {
		driver,
		cs,
		mut rst,
		exti,
		seed,
	} = config;

	let extdev = ExclusiveDevice::new(driver, cs, Delay).unwrap();

	Timer::after_millis(100).await;
	rst.set_low();
	Timer::after_millis(100).await;
	rst.set_high();
	Timer::after_millis(100).await;

	static EXT_STATE: static_cell::StaticCell<embassy_net_wiznet::State<2, 2>> =
		static_cell::StaticCell::new();
	let ext_state = EXT_STATE.init(embassy_net_wiznet::State::<2, 2>::new());

	let (driver, ext_runner): (
		_,
		embassy_net_wiznet::Runner<'static, embassy_net_wiznet::chip::W5500, _, _, _>,
	) = embassy_net_wiznet::new(get_exteth_mac(), ext_state, extdev, exti, rst)
		.await
		.unwrap();

	let config = embassy_net::Config::dhcpv4(Default::default());

	static STACK: static_cell::StaticCell<embassy_net::StackResources<16>> =
		static_cell::StaticCell::new();
	let stack_resources = STACK.init(embassy_net::StackResources::<16>::new());

	let (_stack, mut runner) = embassy_net::new(driver, config, stack_resources, seed);

	select3(
		async move {
			loop {
				Timer::after(Duration::from_secs(60)).await;
			}
		},
		async move {
			ext_runner.run().await;
		},
		async move {
			runner.run().await;
		},
	)
	.await;
	panic!("exteth service ended unexpectedly");
}

pub fn get_exteth_mac() -> [u8; 6] {
	let hash = crate::unique_id::unique_id_sha256();

	let mut macaddr = [0u8; 6];
	macaddr[0] = b'.';
	macaddr[1] = b'o';
	macaddr[2] = b'O';
	macaddr[3] = hash[29];
	macaddr[4] = hash[30];
	macaddr[5] = hash[31];

	macaddr
}
