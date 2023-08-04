.PHONY: all clean lint clippy fmt doc stm32f479vg

ifdef DEBUG
CARGO_MODE := debug
else
CARGO_MODE := release
CARGO_FLAGS := --release
endif

ifndef EXTMAC
export EXTMAC=44:45:56
endif

export ORO_EXT_MAC_ID = "$(EXTMAC)"

all: stm32f479vg

clean:
	rm -rf target

fmt:
	env cargo +nightly fmt --all

lint:
	env cargo +nightly fmt -- --check --verbose

clippy:
	env cargo +nightly clippy $(CARGO_FLAGS) --all --target=variant/stm32f479vg/thumbv7em-none-eabihf.json --features stm32f479vg -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem -- -D clippy::all

doc:
	env cargo +nightly doc $(CARGO_FLAGS) --all --target=variant/stm32f479vg/thumbv7em-none-eabihf.json --features stm32f479vg -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem --open

stm32f479vg: target/$(CARGO_MODE)/oro-link-stm32f479vg.bin
.PHONY: target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware
target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware:
	env cargo +nightly build $(CARGO_FLAGS) --features stm32f479vg --target variant/stm32f479vg/thumbv7em-none-eabihf.json -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem
target/$(CARGO_MODE)/oro-link-stm32f479vg.bin: target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware
	@mkdir -p "$(dir $@)"
	env arm-none-eabi-objcopy --output binary "$<" "$@"
