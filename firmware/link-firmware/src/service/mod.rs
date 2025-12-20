pub mod cicd;
pub mod dev_blinken_light;
pub mod dev_exteth;
pub mod dev_leds;
pub mod dev_oled;
pub mod dev_power_monitor;
pub mod dev_sdcard;
pub mod dev_syseth;
pub mod dev_uart;
pub mod dev_usart;
pub mod dev_usb;

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
		BlinkenLight(dev_blinken_light::Message),
		LedController(dev_leds::Message),
		Oled(dev_oled::Message),
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
	blinken_light:  dev_blinken_light::Channel,
	led_controller: dev_leds::Channel,
	oled:           dev_oled::Channel,
}

pub struct ServiceChannels {
	pub master_bus:        &'static MasterBusChannels,
	pub blinken_light_rx:  <dev_blinken_light::Channel as ChannelExt>::Receiver,
	pub led_controller_rx: <dev_leds::Channel as ChannelExt>::Receiver,
	pub oled_rx:           <dev_oled::Channel as ChannelExt>::Receiver,
}

/// # Panics
/// Panics if called more than once.
pub fn services_channels() -> ServiceChannels {
	static MASTER_BUS: static_cell::StaticCell<MasterBusChannels> = static_cell::StaticCell::new();
	let master_bus = MASTER_BUS.init(MasterBusChannels {
		master:         MasterBus::new(),
		blinken_light:  dev_blinken_light::Channel::new(),
		led_controller: dev_leds::Channel::new(),
		oled:           dev_oled::Channel::new(),
	});

	ServiceChannels {
		master_bus,
		blinken_light_rx: master_bus.blinken_light.receiver(),
		led_controller_rx: master_bus.led_controller.receiver(),
		oled_rx: master_bus.oled.receiver(),
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
				Message::Oled(m) => {
					self.oled.send(m).await;
				}
			}
		}
	}
}
