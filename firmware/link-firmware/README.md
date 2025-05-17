# x86 Oro Link

This folder houses the firmware for the x86 version of the Oro Link,
meant for testing x86 and x86_64 machines.

## Block Diagram

The x86 Oro Link has an STM32 F479VGT6 microcontroller unit (MCU) that
operates several external controllers for interacting with the SUT.

Most notably, the dual-ethernet configuration allows for a completely
isolated network environment for the SUT whilst still being able to
communicate with the outside world (e.g. to stream test results to
the GitHub Actions runner), along with the ability to PXE boot the
newly built Kernel images as part of a release or pull request CI/CD
pipeline.

The Oro Link also provides a USB HID device interface for testing
mouse, keyboard, and other HID input devices.

The Oro Link is intended to test all external user interaction,
including the power and reset buttons, which are controlled via
MOSFETs via the SUT's motherboard's front panel bus header.

Along with the ability to control the power and reset buttons,
the Link can also cut power directly to the system via the `PS_ON`
line of the Power Supply Unit (PSU) in cases where tests have failed,
timed out, or where the SUT is otherwise un-responsive.

The link also sniffs and traces all packets sent/received by the
Link/SUT ethernet controller (using the `link-rpcap` utility),
allowing for applications like WireShark to connect and sniff
all packets sent between the two for debugging purposes.

```mermaid
%%{ init: { 'flowchart': { 'curve': 'linear' } } }%%
flowchart TD
    subgraph "Oro Link"
       linkmcu["MCU (STM32)"]
       linksyseth["ETH to SUT"]
       linkexteth["ETH to LAN"]
       linkusb["USB OTG HID Device"]
       linkusart["USART"]
       linkfp["Power & PWR/RST/PS_ON MOSFETs"]
       linkecon["Edge Connector"]
       linkauxuart["Auxilary UART"]

       linkmcu<-->linksyseth
       linkexteth<-->linkmcu
       linkmcu<-->linkusb
       linkmcu<-->linkusart
       linkmcu<--->linkfp
       linkmcu<-->linkauxuart
       linkmcu<-->|SWD|linkecon
       linkauxuart<-->|"Remote PCAP\n(SUT/Link packet sniffing\nvia Wireshark)"|linkecon
    end

    subgraph "System Under Test (SUT)"
        sutusb["USB Host (USB header)"]
        suteth["Ethernet"]
        sutfp["Front Panel Interface (PWR/RST)"]
        sutusart["USART (COM header)"]
        sut5vsb["5VSB Line (proxied via Oro Link)"]
    end

    subgraph "Coordinator Server / WWW"
        direction LR
        gr[GitHub Actions Runner]
        gh[GitHub]
        gh<-->gr
    end

    psu["Power Supply Unit (PSU)"]
    stlink["ST-Link"]
    devmachine["Dev Machine"]

    psu----->|5VSB / PSOK / PS_ON / COM|linkfp
    linkfp-->|5VSB|sut5vsb
    linksyseth<-->|PXE Boot / TFTP|suteth
    linkusb<-->|Mouse / Keyboard HID Devices|sutusb
    linkusart<-->|Oro Test Protocol|sutusart
    linkfp--->|PWR / RST|sutfp
    gr<-->linkexteth
    linkecon<-->stlink
    stlink<-->|USB|devmachine
```
