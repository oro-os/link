.PHONY: all clean lint clippy fmt doc x86.stm32f479vg x86.stm32f479vg.run rpcap daemon docker udeps

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
LEVEL := trace,embassy_net=debug
endif

ifndef PROBE_RUN
PROBE_RUN := probe-run
endif

PROBE_RUN_FLAGS := --log-format "{t} {L} {s} <{f}:{l}>" --verbose --verify

ifdef DEV_IP
CARGO_FLAGS += --features oro-connect-to-ip
endif

ifndef PLINK
PLINK := plink.exe
endif

ifndef COM
COM = COM16
endif

export DEFMT_LOG = $(LEVEL),embassy_hal_internal=warn
export ORO_CONNECT_TO_IP = $(DEV_IP)

# The default here is for my own machine. Change it to refer to
# the serial device being used by your STLink. On Windows/WSL,
# it's COM16, thus /dev/ttyS16.
ifndef LOGDEV
LOGDEV := /dev/ttyS16
endif

all: x86.stm32f479vg

clean:
	rm -rf target

fmt:
	env cargo fmt --all

lint:
	env cargo fmt --all -- --check --verbose

rpcap:
	$(PLINK) -sercfg 38400,8,n,1,X -serial $(COM) | env LEVEL=debug cargo run $(CARGO_FLAGS) -p link-rpcapd

daemon:
	cargo run -p link-daemon $(CARGO_FLAGS)

clippy:
	env cargo clippy $(CARGO_FLAGS) -p link-firmware-x86 --target=variant/stm32f479vg/thumbv7em-none-eabihf.json --no-default-features --features stm32f479vg -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem -- -D clippy::all
	env cargo clippy $(CARGO_FLAGS) -p link-rpcapd -p link-protocol -p link-daemon -p link-tftp-server -p link-test -- -D clippy::all

doc:
	env cargo doc $(CARGO_FLAGS) -p link-firmware-x86 --target=variant/stm32f479vg/thumbv7em-none-eabihf.json --no-default-features --features stm32f479vg -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem --open

docker:
	@(docker build --rm -f docker/Dockerfile .)

.PHONY: x86.stm32f479vg.udeps other-udeps
udeps: other-udeps # stm32f479vg.udeps

x86.stm32f479vg.udeps:
	# NOTE: does not currently work: https://github.com/est31/cargo-udeps/issues/153
	env cargo udeps $(CARGO_FLAGS) -p link-firmware-x86 --no-default-features --features stm32f479vg --target variant/stm32f479vg/thumbv7em-none-eabihf.json

other-udeps:
	env cargo udeps $(CARGO_FLAGS) -p link-daemon -p link-protocol -p link-protocol-binser -p link-protocol-binser-proc -p link-rpcapd -p link-tftp-server

x86.stm32f479vg.run: x86.stm32f479vg
	$(PROBE_RUN) $(PROBE_RUN_FLAGS) --speed 40000 --chip stm32f479vg target/thumbv7em-none-eabihf/$(CARGO_MODE)/link-firmware-x86

x86.stm32f479vg: target/$(CARGO_MODE)/link-firmware-x86-stm32f479vg.bin
.PHONY: target/thumbv7em-none-eabihf/$(CARGO_MODE)/link-firmware-x86
target/thumbv7em-none-eabihf/$(CARGO_MODE)/link-firmware-x86:
	env cargo build $(CARGO_FLAGS) -p link-firmware-x86 --no-default-features --features stm32f479vg --target variant/stm32f479vg/thumbv7em-none-eabihf.json -Zunstable-options -Zbuild-std=core,compiler_builtins -Zbuild-std-features=compiler-builtins-mem
target/$(CARGO_MODE)/link-firmware-x86-stm32f479vg.bin: target/thumbv7em-none-eabihf/$(CARGO_MODE)/link-firmware-x86
	@mkdir -p "$(dir $@)"
	env arm-none-eabi-objcopy --output binary "$<" "$@"
