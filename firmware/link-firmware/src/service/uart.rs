use embassy_stm32::{mode::Async, usart::Uart};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;

#[embassy_executor::task]
pub async fn uart_service(mut uart: Uart<'static, Async>) -> ! {
	loop {
		Timer::after(Duration::from_millis(1000)).await;
		uart.write_all(b"Hello from the Link\r\n").await.unwrap();
	}
}
