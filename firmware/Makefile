.PHONY: all
all: firmware

.PHONY: firmware
firmware:
	cargo build-firmware

.PHONY: run
run:
	cargo run-firmware

.PHONY: run-release
run-release:
	cargo run-firmware --release
