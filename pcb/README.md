# Oro Link PCBs

The Oro Link PCBs are custom boards meant to interface with the
CI/CD daemon, which itself works with (at the moment, solely) GitHub
Actions runners that it manages.

These PCBs are open source and licensed under the license found in the
root of this repository.

All PCBs are designed in KiCad 7, with some technical sketches
designed in FreeCad.

There are some "legacy" PCBs here that haven't been converted to
self-standing projects (that use the `.lib/` folder). Further,
some of these PCBs are specific to individual rigs and may not
be interesting to most people.

## `link-rpi-base`

This is the base board for Raspberry Pi test rigs. It hosts
the microcontroller, network interface, etc. as well as a
board-to-board connector that interfaces with one of the
daughter boards (R-Pi version specific).

## `link-x86-obt`

> **NOTE:** This project file is a _legacy_ PCB that hasn't
> been converted to use the common components/footprints library
> in `.lib/`. You will be able to view the schematic and layout,
> but changes are more difficult to make and manage. Due to this,
> PRs that include changes to this PCB might take longer to merge.

This is the main board for testing x86/x86_64 machines using
the Oro Link CI/CD pipeline. Kernel builds are booted over PXE,
and communication is done primarily through the serial ports.

The board's form factor is designed to be mounted to the front
of an [Open Bench Table](https://openbenchtable.com/) - hence
`*-obt`.

As such, the board has two ethernet connections - one for communication
with the SUT, and the other for communication with the Link daemon.

Please see the [firmware directory](../firmware/link-firmware-x86)
for a block diagram and further explanations.

## `rj45-inverter-h410mb`

> **NOTE:** This is a rig-specific board that is likely uninteresting
> to you. It's meant to fit and cater to a very specific hardware
> configuration that most likely won't match yours. As such, this PCB
> is not required to operate a Link test rig, and probably won't
> be of use unless you match the test rig's build exactly.

> **NOTE:** This project file is a _legacy_ PCB that hasn't
> been converted to use the common components/footprints library
> in `.lib/`. You will be able to view the schematic and layout,
> but changes are more difficult to make and manage. Due to this,
> PRs that include changes to this PCB might take longer to merge.

This board has a board-mounted RJ-45 plug (_not_ socket - THT
mounted plugs were really tricky to find!) that is wired same-side
to an RJ-45 port. It's used on one of the x86 test rigs to keep the
left side of the OBT setup flush, without having an ethernet cable
poking out to the left and causing clearance issues.

## `vga-display-card-h410mb`

> **NOTE:** This is a rig-specific board that is likely uninteresting
> to you. It's meant to fit and cater to a very specific hardware
> configuration that most likely won't match yours. As such, this PCB
> is not required to operate a Link test rig, and probably won't
> be of use unless you match the test rig's build exactly.

> **NOTE:** This project file is a _legacy_ PCB that hasn't
> been converted to use the common components/footprints library
> in `.lib/`. You will be able to view the schematic and layout,
> but changes are more difficult to make and manage. Due to this,
> PRs that include changes to this PCB might take longer to merge.

This board provides power to a [VGA display](https://www.amazon.de/-/en/gp/product/B09ZDK5DMT/ref=ppx_yo_dt_b_search_asin_title?ie=UTF8&psc=1) via a PCIe x1
port on the motherboard. It also doubles as a VGA port mount
for the graphics card (which uses a cable to interface between the
card and the port), which further allows the left side of the OBT
to remain flush.

If you're interested in using the VGA display linked above, I
recommend to take it apart, desolder the transducer (beeper)
in the top corner of the board, and replace it with a resistor,
lest you go insane.

# Making a new board

If you want to add a project here, you'll need to do a few things:

1. Make the folder here. Use kebab-case, such as `link-rpi-2` instead of
   `LinkRpi2`.
2. Open KiCad and create the project in the folder. Uncheck the box that
   asks if you want to create a project folder. Preferably, name the project
   using TitleCase, such as `LinkRpi2`.
3. From any **non-legacy** project (see the above list), copy `fp-lib-table`
   and `sym-lib-table` into the new project's directory. **Restart KiCad.**
4. In both the schematic and the PCB layout, go to _File > Page Setup..._
   and change the worksheet to `${KIPRJMOD}/../.lib/Oro.kicad_wks`.
5. If you're using common sheets to re-use some of the modular schematics
   from other boards, **always use relative paths**
   (e.g. `${KIPRJMOD}/../.lib/<something>`).

If you're adding a component, it is **highly** recommended to add it from
[Ultra Librarian](https://ultralibrarian.com).

Symbols and footprints must be added to `.lib/Oro.kicad_sym` and `.lib/Oro.pretty/`,
respectively. STEP models should be placed in `.lib/step` and properly linked up
from within the footprint (usually requires manually editing the footprint after
loading it into `Oro.pretty/`).

Further, make sure to update the footprint ID in the symbol after moving it into the
Oro library.

Most importantly, **you must make sure all common fields are populated and accurate**.
These are fields found on other symbols, the names of which must match **exactly**.
For example, Mouser ID, Mfr. ID, etc.
