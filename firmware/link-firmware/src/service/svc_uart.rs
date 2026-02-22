use embassy_futures::select::{Either, select};
use link_protocol::{Error, Request, Response};

use super::{dev_uart::Response as UartResponse, svc_init::Cmd as InitCmd};
use crate::atomic::Relaxed;

pub type Channel = crate::channel::Channel<Cmd, 4>;

pub enum Cmd {
	SetInitMode { in_init_mode: bool },
}

#[embassy_executor::task]
pub async fn run(bus: super::Bus, rx: &'static Channel) -> ! {
	let mut in_init_mode = false;

	loop {
		let cmd = loop {
			let packet = match select(super::dev_uart::PACKET.wait(), rx.receive()).await {
				Either::First(packet) => packet,
				Either::Second(cmd) => {
					break cmd;
				}
			};

			defmt::trace!("servicing packet: {:?}", packet);
			let res = match packet {
				Request::FactoryReset if in_init_mode => {
					bus.svc_init.send(InitCmd::FactoryReset).await;
					Response::Ok.into()
				}
				Request::FactoryReset => Response::Err(Error::InitOnly).into(),
				Request::FinishInitMode if in_init_mode => {
					in_init_mode = false;
					bus.svc_init.send(InitCmd::Finish).await;
					Response::Ok.into()
				}
				Request::FinishInitMode => Response::Err(Error::InitOnly).into(),
				Request::IsInInitMode => Response::Uint(if in_init_mode { 1 } else { 0 }).into(),
				Request::GetVersionMajor => Response::Uint(crate::version::VERSION_MAJOR).into(),
				Request::GetVersionMinor => Response::Uint(crate::version::VERSION_MINOR).into(),
				Request::GetVersionPatch => Response::Uint(crate::version::VERSION_PATCH).into(),
				Request::GetFrameCount => {
					Response::Uint(u64::from(super::dev_oled::FRAME_COUNTER.get())).into()
				}
				Request::GetFrame => UartResponse::OledFrame,
				Request::GetLightState => {
					Response::LightState {
						debug_leds_max_duty: super::dev_blinken_light::CONFIG_DUTY_PERIOD,
						debug_leds:          [
							super::dev_blinken_light::DBG_LED1_DUTY.get(),
							super::dev_blinken_light::DBG_LED2_DUTY.get(),
							super::dev_blinken_light::DBG_LED3_DUTY.get(),
						],
						controller:          {
							let mut r = [0u32; 9];

							for i in (0..36).step_by(4) {
								r[i >> 2] = (u32::from(super::dev_leds::DBG_LIGHT_VALUES[i].get())
									<< 24) | (u32::from(
									super::dev_leds::DBG_LIGHT_VALUES[i + 1].get(),
								) << 16) | (u32::from(
									super::dev_leds::DBG_LIGHT_VALUES[i + 2].get(),
								) << 8) | u32::from(
									super::dev_leds::DBG_LIGHT_VALUES[i + 3].get(),
								);
							}

							r
						},
					}
					.into()
				}
				Request::EndLightProgram if in_init_mode => {
					bus.dev_blinken_light
						.send(super::dev_blinken_light::Cmd::Config)
						.await;
					bus.dev_leds.send(super::dev_leds::Cmd::AllOff).await;
					Response::Ok.into()
				}
				Request::EndLightProgram => Response::Err(Error::InitOnly).into(),
				Request::StartLightProgram { debug, controller } if in_init_mode => {
					bus.dev_blinken_light
						.send(super::dev_blinken_light::Cmd::Manual { states: debug })
						.await;
					let mut channels = [0u8; 36];
					for (i, b) in controller
						.into_iter()
						.flat_map(u32::to_be_bytes)
						.enumerate()
					{
						channels[i] = b;
					}
					bus.dev_leds
						.send(super::dev_leds::Cmd::SetManualState { state: channels })
						.await;
					Response::Ok.into()
				}
				Request::StartLightProgram { .. } => Response::Err(Error::InitOnly).into(),
			};

			defmt::trace!("sending serviced response: {:?}", res);
			bus.dev_uart.send(super::dev_uart::Cmd::Send(res)).await;
		};

		match cmd {
			Cmd::SetInitMode { in_init_mode: iim } => {
				in_init_mode = iim;
			}
		}
	}
}
