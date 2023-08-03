# Firmware for the Oro Link (x86)

This houses the Rust firmware for the Oro Link (x86 board).

It is always updated for the latest revision on `master`, but due to
supply chain issues, various boards may be designed with different uC.

Given that the (latest revision, uC model) tuple indicates a singular
design, the firmware is structured in a way where the uC model directly
correlates with pin assignments. Corrollary, there are no two board
designs with the same uC model and different pin configs. Keep this in mind
when perusing the sources.

## Building

With a sufficiently up-to-date Rust nightly toolchain and `arm-none-eabi-*`
utilities installed, run `make`:

```shell
make \
	# (OPTIONAL) The last three octets of the
	# external ethernet interface MAC address
	# (default: "44:45:56" ('DEV'))
	EXTMAC="78:9A:BC" \
	# (OPTIONAL) if provided, builds in debug mode
	# (default: release mode)
	DEBUG=1
```

Flashable binaries will show up in `target/debug` or `target/release`,
named by their uC model.

# License

See repository root for copyright information and license.
