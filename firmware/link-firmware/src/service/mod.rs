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
pub mod svc_cicd;
pub mod svc_successful_boot;

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
		Usart(dev_usart::Message),
		Uart(dev_uart::Message),
		PowerMonitor(dev_power_monitor::Message),
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
	usart:          dev_usart::Channel,
	uart:           dev_uart::Channel,
}

pub struct ServiceChannels {
	pub master_bus:        &'static MasterBusChannels,
	pub blinken_light_rx:  <dev_blinken_light::Channel as ChannelExt>::Receiver,
	pub led_controller_rx: <dev_leds::Channel as ChannelExt>::Receiver,
	pub oled_rx:           <dev_oled::Channel as ChannelExt>::Receiver,
	pub usart_rx:          <dev_usart::Channel as ChannelExt>::Receiver,
	pub uart_rx:           <dev_uart::Channel as ChannelExt>::Receiver,
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
		usart:          dev_usart::Channel::new(),
		uart:           dev_uart::Channel::new(),
	});

	ServiceChannels {
		master_bus,
		blinken_light_rx: master_bus.blinken_light.receiver(),
		led_controller_rx: master_bus.led_controller.receiver(),
		oled_rx: master_bus.oled.receiver(),
		usart_rx: master_bus.usart.receiver(),
		uart_rx: master_bus.uart.receiver(),
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
				Message::Usart(dev_usart::Message::Send(m)) => {
					self.usart.send(dev_usart::Message::Send(m)).await;
				}
				Message::Usart(o) => {
					defmt::debug!("USART message: {:?}", o);
				}
				Message::Uart(dev_uart::Message::Send(m)) => {
					self.uart.send(dev_uart::Message::Send(m)).await;
				}
				Message::Uart(o) => {
					defmt::debug!("UART message: {:?}", o);
				}
				Message::PowerMonitor(dev_power_monitor::Message::PowerReading(_current)) => {
					// TODO
				}
			}
		}
	}
}
