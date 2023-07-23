.PHONY: all clean stm32f7

ifdef DEBUG
CARGO_MODE := debug
else
CARGO_MODE := release
endif

all: stm32f7

clean:
	rm -rf target

stm32f7: target/$(CARGO_MODE)/oro-link-stm32f7.bin
.PHONY: target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-stm32f7
target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-stm32f7:
	cargo build --$(CARGO_MODE) -p $(@F)
target/$(CARGO_MODE)/%.bin: target/thumbv7em-none-eabihf/$(CARGO_MODE)/%
	arm-none-eabi-objcopy --output binary "$<" "$@"
