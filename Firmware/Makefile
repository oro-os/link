.PHONY: all clean lint clippy fmt doc stm32f479vg stm32f479vg.run

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
ifeq ($(CARGO_MODE),release)
LEVEL := info
else
LEVEL := trace
endif
endif

LEVEL := $(LEVEL),embassy_hal_internal=warn

ifndef PROBE_RUN
PROBE_RUN := probe-run
endif

ifeq ($(CARGO_MODE),debug)
PROBE_RUN_FLAGS := --log-format "{t} {L} {s} <{f}:{l}>" --verbose --verify
else
PROBE_RUN_FLAGS := --log-format "{t} {L} {s}" --verify
endif

ifdef DEV_IP
CARGO_FLAGS += --features oro-connect-to-ip
endif

export DEFMT_LOG = $(LEVEL)
export ORO_CONNECT_TO_IP = $(DEV_IP)

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

stm32f479vg.run: stm32f479vg
	$(PROBE_RUN) $(PROBE_RUN_FLAGS) --speed 40000 --chip stm32f479vg target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware

stm32f479vg: target/$(CARGO_MODE)/oro-link-stm32f479vg.bin
.PHONY: target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware
target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware:
	env cargo build $(CARGO_FLAGS) --features stm32f479vg --target variant/stm32f479vg/thumbv7em-none-eabihf.json -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem
target/$(CARGO_MODE)/oro-link-stm32f479vg.bin: target/thumbv7em-none-eabihf/$(CARGO_MODE)/oro-link-firmware
	@mkdir -p "$(dir $@)"
	env arm-none-eabi-objcopy --output binary "$<" "$@"
