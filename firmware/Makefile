.PHONY: all
all: wasm

.PHONY: wasm
wasm:
	wasm-pack build --target web --out-dir ../../configurator/wasm --debug --out-name linkproto link-protocol --features typescript
