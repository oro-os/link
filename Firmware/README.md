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
utilities installed, run `make`.

Flashable binaries will show up in `target/release`, named by their uC model.

To build in debug mode, use `make DEBUG=1`. Binaries will thus show up in
`target/debug`.

# License

See repository root for copyright information and license.
