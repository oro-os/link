use embassy_futures::select::select3;
use embassy_stm32::{exti::ExtiInput, gpio::OutputOpenDrain, mode::Async, spi::Spi};
use embassy_time::{Delay, Duration, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;

#[embassy_executor::task]
pub async fn exteth_service(
	driver: Spi<'static, Async>,
	cs: OutputOpenDrain<'static>,
	mut rst: OutputOpenDrain<'static>,
	exti: ExtiInput<'static>,
	seed: u64,
) {
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

fn unique_id() -> [u8; 32] {
	use sha2::Digest;

	let mut sha256 = sha2::Sha256::new();

	for i in 0..3 {
		sha256.update(stm32_metapac::UID.uid(i).read().to_be_bytes());
	}

	sha256.finalize().into()
}

pub fn get_exteth_mac() -> [u8; 6] {
	let hash = unique_id();

	let mut macaddr = [0u8; 6];
	macaddr[0] = b'.';
	macaddr[1] = b'o';
	macaddr[2] = b'O';
	macaddr[3] = hash[29];
	macaddr[4] = hash[30];
	macaddr[5] = hash[31];

	macaddr
}
