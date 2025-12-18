# Oro Link Firmware / Daemon / Utilities

This folder holds the firmware for the Oro Link along with the
daemon and associated utilities for interacting with and debugging
the Link.

- `link-firmware` - the firmware code for the Oro Link
- `link-daemon` - which is the Oro Link broker daemon (used by CI/CD)
- `link-rpcapd` - which is the remote PCAP daemon for debugging Link&lt;-&gt;SUT (system) ethernet packets

## Building / Running

```shell
cargo build              # build non-firmware crates
cargo build-firmware     # build link firmware
cargo run -p link-daemon # run the Link broker daemon
```

### Running under WSL

If you're under WSL, you'll need to symlink `probe-rs.exe` as `/usr/local/bin/probe-rs` or
somewhere else on your `$PATH`.

### Running `rpcapd`

`rpcapd` expects standard input to be the incoming stream from the _auxiliary_ serial port
exposed by the STLINK-V3MINIE (_not_ the one that the STLINK-V3MINIE uses to program the
chip itself).

Under Linux, this can be achieved using `stty` and passing the path to the corresponding
serial port. This is often `/dev/ttyUSB0`, but might differ on your machine.

```shell
stty -F /dev/ttyUSB0 115200 cs8 -parenb -cstopb ixon -ixany -ixoff | env LEVEL=debug cargo run -p link-rpcapd
```

Under WSL or on Windows, you'll need to pipe in the output of `plink.exe` (from the PuTTY project).
This is often `COM16`, but might differ on your machine.

```shell
plink.exe -sercfg 115200,8,n,1,X -serial COM16 | env LEVEL=debug cargo run -p link-rpcapd
```

Then, you can connect to `rpcapd` using Wireshark by telling it the Oro Link interface exists via
the CLI. This will start Wireshark with the endpoint listed in the interface list. Starting a session
on that interface will display all packets between the Link and the SUT (system under test).

```shell
wireshark --interface rpcap://127.0.0.1:2002/oro
```
