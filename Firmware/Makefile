.PHONY: all clean lint clippy fmt stm32f479

ifdef DEBUG
CARGO_MODE := debug
else
CARGO_MODE := release
CARGO_FLAGS := --release
endif

ifdef SERIAL
CARGO_FLAGS += --features debug_serial
endif

all: stm32f479

clean:
	rm -rf target

fmt:
	cargo fmt --all

lint:
	cargo fmt -- --check --verbose

clippy:
	env cargo clippy $(CARGO_FLAGS) --all --target=thumbv7em-none-eabihf -- -D clippy::all

stm32f479: target/$(CARGO_MODE)/oro-link-stm32f479.bin
.PHONY: target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-stm32f479
target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-stm32f479:
	cargo build $(CARGO_FLAGS) --target=thumbv7em-none-eabihf -p $(@F)
target/$(CARGO_MODE)/%.bin: target/thumbv7em-none-eabihf/$(CARGO_MODE)/%
	arm-none-eabi-objcopy --output binary "$<" "$@"
