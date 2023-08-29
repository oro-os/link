.PHONY: all clean lint clippy fmt doc log stm32f479vg stm32f479vg.run

ifdef DEBUG
CARGO_MODE := debug
else
CARGO_MODE := release
CARGO_FLAGS := --release
endif

ifndef EXTMAC
export EXTMAC=44:45:56
endif

ifndef LEVEL
LEVEL := warn
endif

ifndef PROBE_RUN
PROBE_RUN := probe-run
endif

export DEFMT_LOG = $(LEVEL)

export ORO_EXT_MAC_ID = "$(EXTMAC)"

# The default here is for my own machine. Change it to refer to
# the serial device being used by your STLink. On Windows/WSL,
# it's COM16, thus /dev/ttyS16.
ifndef LOGDEV
LOGDEV := /dev/ttyS16
endif

all: stm32f479vg

clean:
	rm -rf target

fmt:
	env cargo fmt --all

lint:
	env cargo fmt -- --check --verbose

clippy:
	env cargo clippy $(CARGO_FLAGS) --all --target=variant/stm32f479vg/thumbv7em-none-eabihf.json --features stm32f479vg -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem -- -D clippy::all

doc:
	env cargo doc $(CARGO_FLAGS) --all --target=variant/stm32f479vg/thumbv7em-none-eabihf.json --features stm32f479vg -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem --open

log:
	socat FILE:/dev/ttyS16,b115200,nonblock,cs8,cstopb=1,raw,parenb=0 stdout | defmt-print -e target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware --show-skipped-frames

stm32f479vg.run: stm32f479vg
	$(PROBE_RUN) --chip stm32f479vg target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware

stm32f479vg: target/$(CARGO_MODE)/oro-link-stm32f479vg.bin
.PHONY: target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware
target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware:
	env cargo build $(CARGO_FLAGS) --features stm32f479vg --target variant/stm32f479vg/thumbv7em-none-eabihf.json -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem
target/$(CARGO_MODE)/oro-link-stm32f479vg.bin: target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware
	@mkdir -p "$(dir $@)"
	env arm-none-eabi-objcopy --output binary "$<" "$@"
