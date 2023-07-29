use stm32f4xx_hal::i2c::{Instance, I2c};

pub fn init<I2C: Instance>(i2c: I2c<I2C>) {

}

macro_rules! init {
	($scl:expr, $sda:expr, $clocks:expr) => {

	}
}

pub(crate) use init;
