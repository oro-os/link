<div align="center">
	<img src="https://github.com/oro-os/link/raw/master/Asset/screenshot.png" />
</div>

# Oro Link

This is the Oro Link project, a set of custom PCBs, firmware, and server
infrastructure for the testing of the Oro Operating System via CI/CD pipelines.

The link is a physical board that interfaces with the ports and headers
of supported computers, devices, and boards, and runs firmware to obtain builds of
the Oro kernel and associated modules to send to the System Under Test (SUT)
for automated testing and reporting.

The Oro Link allows contributors to the Oro project direct access via
GitHub Actions the ability to test changes directly on real hardware
alongside emulated environments, allowing for changes to be tested
against niche or problematic hardware configurations.

## Supported Devices / Architectures

The supported architectures are listed here. In the case that several revisions or variants
of a single device/arch board exist, links go to the currently used/developed/supported version.

- x86/x86_64 [[pcb (open bench table)]](pcb/link-x86-obt) [[firmware]](firmware/link-firmware-x86)

# License

<div align="center">
	<img src="https://github.com/oro-os/link/raw/master/Asset/oro-banner.svg?sanitize=true" />
</div>

Part of the Oro Operating System project.

The Oro Link is released to the public, WARRANTY FREE
and provided AS IS. The Oro project and its contributors are NOT
responsible for any damages caused by the purchase, assembly, or operation
of the device, under any circumstances.

The Oro Link's PCB and associated custom footprints are released
under the [MIT License](LICENSE).

Unless otherwise specified, all Oro Link firmware (software code)
and all other materials are licensed under the same license.

Firmware dependencies are under their respective licenses.

The _Enter Command_ font used in part for text display is released CC-BY-4.0
by [Font End Dev (jeti)](https://fontenddev.com).
