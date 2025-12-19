pub mod blinken_light;
pub mod cicd;
pub mod exteth;
pub mod led_controller;
pub mod oled;
pub mod power_monitor;
pub mod sdcard;
pub mod syseth;
pub mod uart;
pub mod usart;
pub mod usb;

use crate::channel::{Channel, ChannelExt};

macro_rules! def_message {
	($(#[$attr:meta])* $vis:vis enum $name:ident { $($v_name:ident($v_ty:ty)),* $(,)? }) => {
		$(#[$attr])*
		$vis enum $name {
			$(
				$v_name($v_ty),
			)+
		}

		$(impl From<$v_ty> for $name {
			fn from(value: $v_ty) -> Self {
				Self::$v_name(value)
			}
		})+
	};
}

def_message! {
	#[derive(defmt::Format)]
	pub enum Message {
		BlinkenLight(blinken_light::Message),
		LedController(led_controller::Message),
	}
}

pub type MasterBus = Channel<Message, 64>;
pub type Bus = <MasterBus as ChannelExt>::Sender;

pub trait Dispatch {
	async fn dispatch(&mut self, msg: impl Into<Message>);
}

impl Dispatch for Bus {
	async fn dispatch(&mut self, msg: impl Into<Message>) {
		self.send(msg.into()).await;
	}
}

pub struct MasterBusChannels {
	master:         MasterBus,
	blinken_light:  blinken_light::Channel,
	led_controller: led_controller::Channel,
}

pub struct ServiceChannels {
	pub master_bus:        &'static MasterBusChannels,
	pub blinken_light_rx:  <blinken_light::Channel as ChannelExt>::Receiver,
	pub led_controller_rx: <led_controller::Channel as ChannelExt>::Receiver,
}

/// # Panics
/// Panics if called more than once.
pub fn services_channels() -> ServiceChannels {
	static MASTER_BUS: static_cell::StaticCell<MasterBusChannels> = static_cell::StaticCell::new();
	let master_bus = MASTER_BUS.init(MasterBusChannels {
		master:         MasterBus::new(),
		blinken_light:  blinken_light::Channel::new(),
		led_controller: led_controller::Channel::new(),
	});

	ServiceChannels {
		master_bus,
		blinken_light_rx: master_bus.blinken_light.receiver(),
		led_controller_rx: master_bus.led_controller.receiver(),
	}
}

impl MasterBusChannels {
	pub fn sender(&'static self) -> <MasterBus as ChannelExt>::Sender {
		self.master.sender()
	}

	pub async fn run_master_bus(&'static self) -> ! {
		loop {
			let msg = self.master.receive().await;
			match msg {
				Message::BlinkenLight(m) => {
					self.blinken_light.send(m).await;
				}
				Message::LedController(m) => {
					self.led_controller.send(m).await;
				}
			}
		}
	}
}
