use embassy_stm32::gpio::Output;

pub type Channel = crate::channel::Channel<Cmd, 2>;

pub enum Cmd {
	On,
	Off,
}

pub struct Config {
	pub psu_on: Output<'static>,
}

#[embassy_executor::task]
pub async fn run(rx: &'static Channel, config: Config) -> ! {
	let Config { mut psu_on } = config;

	loop {
		match rx.receive().await {
			Cmd::On => {
				defmt::debug!("turning PSU on");
				psu_on.set_high();
				crate::vars::STAT_PSU_ON.set(true);
			}
			Cmd::Off => {
				defmt::debug!("turning PSU off");
				psu_on.set_low();
				crate::vars::STAT_PSU_ON.set(false);
			}
		}
	}
}
