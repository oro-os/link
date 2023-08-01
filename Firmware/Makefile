.PHONY: all clean lint clippy fmt stm32f479

ifdef DEBUG
CARGO_MODE := debug
else
CARGO_MODE := release
CARGO_FLAGS := --release
endif

all: stm32f479

clean:
	rm -rf target

fmt:
	cargo fmt --all

lint:
	cargo fmt -- --check --verbose

clippy:
	env cargo clippy $(CARGO_FLAGS) --all --target=variant/stm32f479/thumbv7em-none-eabihf.json --features stm32f479 -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem -- -D clippy::all

stm32f479: target/$(CARGO_MODE)/oro-link-stm32f479.bin
.PHONY: target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware
target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware:
	cargo build $(CARGO_FLAGS) --features stm32f479 --target variant/stm32f479/thumbv7em-none-eabihf.json -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem
target/$(CARGO_MODE)/oro-link-stm32f479.bin: target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware
	@mkdir -p "$(dir $@)"
	arm-none-eabi-objcopy --output binary "$<" "$@"
