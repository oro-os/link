.PHONY: all
all: wasm firmware cli

.PHONY: wasm
wasm:
	wasm-pack build --target web --out-dir ../../configurator/wasm --out-name linkproto link-protocol --features typescript

.PHONY: firmware
firmware:
	cargo build-firmware

.PHONY: run
run:
	cargo run-firmware

.PHONY: run-release
run-release:
	cargo run-firmware --release

.PHONY: cli
cli:
	cargo build -p link-cli
