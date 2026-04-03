use cortex_m::delay::Delay;
use embassy_stm32::{
	gpio::{Level, OutputOpenDrain, Speed},
	peripherals as per, spi,
	time::Hertz,
};
use embedded_hal_bus::spi::ExclusiveDevice;
use w5500_ll::{Interrupt, Protocol, Sn, SocketCommand, SocketMode, eh1::vdm::W5500};

/// Puts the board to sleep and wakes it up using Wake-on-LAN.
///
/// This effectively resets the board. It completely blocks
/// the CPU upon call, disallowing any further async
/// execution.
///
/// # Safety
///
/// > **⚠️⚠️⚠️⚠️ THIS MAKES THE CHIP UNFLASHABLE. ⚠️⚠️⚠️⚠️**
/// >
/// > If this is called within a short period of the chip coming online,
/// > YOU CAN BRICK THE ORO LINK. _Put this behind a timer of at least a few seconds._
///
/// Blocks the CPU indefinitely, puts the system into a WoL
/// state, and relies on an external WoL packet to wake it up.
pub unsafe fn go_to_sleep_and_wait_for_wol() -> ! {
	// NOTE: **NEVER** make this function async.

	// First, steal away the SPI4 peripheral. We can do this unsafely.
	let exteth: spi::Spi<'_, embassy_stm32::mode::Blocking> = unsafe {
		spi::Spi::new_blocking(
			per::SPI4::steal(),
			per::PE2::steal(),
			per::PE14::steal(),
			per::PE13::steal(),
			{
				let mut config = spi::Config::default();
				config.frequency = Hertz(20_000_000);
				config
			},
		)
	};

	// Wrapper around exteth to bridge the SpiDevice traits that the wiznet needs.
	let mut cm_per = unsafe { cortex_m::Peripherals::steal() };

	// Then, create a UDP socket and put the W5500 into WoL mode.
	let exteth_cs =
		OutputOpenDrain::new(unsafe { per::PE11::steal() }, Level::High, Speed::VeryHigh);
	let exteth = ExclusiveDevice::new_no_delay(exteth, exteth_cs).unwrap();
	let mut wiznet = W5500::new(exteth);
	initialize_wiznet_wol_socket(&mut wiznet).expect("failed to initialize W5500 WoL socket");

	// Enter Standby
	embassy_stm32::pac::PWR.csr1().modify(|w| {
		w.set_ewup(true); // Enable PA0 rising edge = wakeup functionality
	});
	embassy_stm32::pac::PWR.cr1().modify(|w| {
		w.set_cwuf(true); // Clear wakeup flag
		w.set_pdds(embassy_stm32::pac::pwr::vals::Pdds::STANDBY_MODE);
	});

	// Perform a read from CR in order to ensure the chip has committed the
	// writes above. Otherwise, a WFI might just do a low power stop.
	let _ = embassy_stm32::pac::PWR.cr1().read();

	cm_per.SCB.set_sleepdeep();

	// Set the polarity back to high
	unsafe {
		OutputOpenDrain::new(per::PB6::steal(), Level::High, Speed::Low).set_high();
	}

	// Wait a moment for stability
	let mut delay = Delay::new(cm_per.SYST, 180000000);
	delay.delay_ms(10);

	cortex_m::asm::wfi();
	panic!("WFI should have deep-sleeped but somehow we continued");
}

fn initialize_wiznet_wol_socket<R: w5500_ll::Registers>(regs: &mut R) -> Result<(), R::Error> {
	const WOL_SOCKET: Sn = Sn::Sn1; // safe choice when Sn0 = MACRAW
	const ANY_PORT: u16 = 9; // datasheet says "any source port number" is fine

	// 1. Clear any pending global interrupts (good hygiene)
	let ir = regs.ir()?;
	regs.set_ir(ir)?;

	// 2. Configure the socket as plain UDP (per WoL section of the datasheet)
	regs.set_sn_mr(WOL_SOCKET, SocketMode::DEFAULT.set_protocol(Protocol::Udp))?;
	regs.set_sn_port(WOL_SOCKET, ANY_PORT)?;

	// 3. Open the UDP socket
	regs.set_sn_cr(WOL_SOCKET, SocketCommand::Open)?;

	// 4. Enable Wake-on-LAN in the common Mode Register
	let mr = regs.mr()?;
	regs.set_mr(mr.enable_wol())?;

	// 5. Mask **ALL** interrupts except the Magic Packet interrupt
	regs.set_simr(0)?; // disable every socket interrupt
	regs.set_imr(Interrupt::DEFAULT.set_mp())?; // only MP can trigger INTn

	// Disable all interrupts for individual sockets
	regs.set_simr(0)?;

	Ok(())
}
