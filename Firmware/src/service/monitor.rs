use crate::uc::Monitor;
use core::cell::RefCell;
use embassy_time::{Duration, Instant, Timer};

pub async fn run<M: Monitor>(monitor: &RefCell<M>) {
	loop {
		{
			if let Ok(mut monitor) = monitor.try_borrow_mut() {
				let millis = Instant::now().as_millis();
				monitor.tick(millis);
			}
		}

		Timer::after(Duration::from_millis(1000 / 240)).await;
	}
}
