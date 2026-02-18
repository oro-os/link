pub type Channel = crate::channel::Channel<Cmd, 4>;

pub enum Cmd {
	PowerReading { ma: u16 },
}

#[embassy_executor::task]
pub async fn run(rx: &'static Channel) -> ! {
	loop {
		match rx.receive().await {
			Cmd::PowerReading { ma } => {
				defmt::trace!("power reading: {}mA", ma);
			}
		}
	}
}
