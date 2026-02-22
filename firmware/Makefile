.PHONY: all
all: wasm

.PHONY: wasm
wasm:
	wasm-pack build --target web --out-dir ../../configurator/wasm --out-name linkproto link-protocol --features typescript
