# ASTRO A50 (Gen 5) USB baseline on Arch Linux

Captured on 2026-11-25 while playback was in use. The owner confirmed the
retail model **Logitech G ASTRO A50 (Gen 5), black**. The attached USB
device identified itself as **Logitech A50**.

## USB enumeration

- VID/PID: `046d:0b1c`. Manufacturer/product strings: `Logitech`, `A50`.
- USB device descriptor: `bcdUSB 1.10`, `bcdDevice 1.03`, full speed
  (12 Mbit/s). The USB device revision field alone gives no headset firmware version.
- One configuration (value 1), with nine interfaces. Audio uses USB Audio
  Class 1.0 and the Linux `snd-usb-audio` driver. HID uses `usbhid` with
  `hid-generic` bound to the HID device.

| Interface | Role | Alternate setting shown | Endpoints and maximum packet sizes |
| --- | --- | --- | --- |
| 0 | AudioControl, A50 Voice | 0 | No interface endpoints |
| 1 | AudioStreaming, playback | 1 | Isochronous `0x02` OUT (196 B), `0x82` IN feedback (3 B) |
| 2 | AudioControl, A50 Mic | 0 | No interface endpoints |
| 3 | AudioStreaming, capture | 1 | Isochronous `0x81` IN (98 B) |
| 4 | AudioControl, A50 Game | 0 | No interface endpoints |
| 5 | AudioStreaming, playback | 1 | Isochronous `0x03` OUT (294 B), `0x83` IN feedback (3 B) |
| 6 | AudioControl, A50 Stream Out | 0 | No interface endpoints |
| 7 | AudioStreaming, capture | 1 | Isochronous `0x84` IN (196 B) |
| 8 | Vendor HID, subclass/protocol 0 | 0 | Interrupt `0x85` IN and `0x05` OUT (64 B each, 1 ms interval) |

All AudioStreaming interfaces also advertise alternate setting 0 with no endpoints.

## HID report descriptor

Linux exposed a single matching HID device and one `hidraw` node on
interface 8. The static descriptor read from the kernel's sysfs
`report_descriptor` attribute is 39 bytes, matching the USB HID
descriptor's advertised length. The current user could read the raw USB
device node and lacked write access. `lsusb -v` showed the cached
USB descriptors but could not retrieve the HID report descriptor directly.

The matching `hidraw` node had mode `0660`, owner/group `root:root`,
and the `seat` and `uaccess` tags. A named ACL granted the current
session user read/write access, verified with permission checks. The
installed OpenLogi udev rules have a Logitech vendor-wide `uaccess` match.
The ALSA control and four PCM nodes were also accessible read/write.
No HID node was opened for I/O, and no USB or HID commands were sent to
exercise the device's control protocol.

The descriptor defines a vendor Application collection (page `0xff32`,
usage `0x0074`) with a nested Report collection. Report ID `0x02`
has 63 one-byte data fields in each direction (64 bytes including the report
ID), input usage `0x0075` and output usage `0x0076`. It declares no Feature
reports. The hex block contains the exact static descriptor bytes and can
serve as input for a descriptor parser or discovery test. SHA-256 of the
decoded bytes is
`6e8f1e3e59bdf99d6041ca2285d406ad52385a4cd31f100d2084fcb54052eefa`.

```hex
06 32 ff 09 74 a1 01 a1 03 85 02 75 08 15 00 26
ff 00 95 3f 09 75 81 02 75 08 15 00 26 ff 00 95
3f 09 76 91 02 c0 c0
```

## Audio snapshot

Kernel `7.3.5-arch1-1`, PipeWire and its PulseAudio compatibility server
`1.6.11`, WirePlumber `0.5.19`. ALSA registered the `A50` USB-Audio card
with PCM devices 0 and 1. PipeWire exposed `alsa_card.usb-Logitech_A50-00`
with the `pro-audio` profile. The following suffixes identify that card's
PipeWire nodes. Both playback sinks also had monitor sources.

| Role | ALSA PCM device / USB interface | PipeWire node suffix | ALSA format at 48 kHz | Snapshot |
| --- | --- | --- | --- | --- |
| Voice playback | 0 playback / 1 | `pro-output-0` | Stereo `S16_LE` | Suspended |
| Game playback | 1 playback / 5 | `pro-output-1` | Stereo `S24_3LE` | Running |
| Microphone capture | 0 capture / 3 | `pro-input-0` | Mono `S16_LE` | Suspended |
| Stream Out capture | 1 capture / 7 | `pro-input-1` | Stereo `S16_LE` | Suspended |

Role labels come from the USB AudioControl interface strings. The PCM
mapping comes from ALSA's USB stream descriptions and PipeWire's ALSA path
properties. The snapshot demonstrates active Game playback and enumeration
of the other paths. It does not establish capture quality or physical
Game/Voice mixer behavior. No audio routes, settings, ACLs or services were
changed. Kernel metadata and audio server state were read as the session user.

## Model and protocol evidence

- **Measured locally:** `046d:0b1c`, USB product string `A50`, interface 8
  with usage page/usage `ff32:0074` and report ID `0x02`.
- **External implementation:** [HeadsetControl's A50 driver] identifies the
  exact measured tuple as ASTRO A50 Gen 5 and reports a hardware-tested vendor
  protocol. This matches the owner's retail model confirmation. Logitech
  lists the Gen 5 under PLAYSYNC AUDIO. [Logitech's model comparison]
- **OpenLogi at `da245558a4cafef5271480cc10e633c901f62a82`:** The observed
  collection matches neither the [HID++ long collection filter] nor a
  [standalone gaming-headset identity]. The latter already provides
  [identity-only inventory for the G735] and is a precedent for discovery.
  The existing [HID++ sidetone operation] was measured on G535 and supplies
  no evidence of compatibility with this device. The source tree has no
  ASTRO control implementation at this revision. The installed package was
  `openlogi-bin v0.9.0-1`. The agent service was inactive. CLI inventory and
  diagnostics were left unrun because direct fallback may send HID probes.
- **Unmeasured:** Control traffic, battery and charging semantics for this
  unit, headset and base firmware versions, other USB modes, Windows and
  macOS behavior, and where Dolby processing takes place.

## Next hardware check

Use the external Gen 5 implementation to plan a device-specific Linux
control capture. Start with a bounded, passive read of the vendor input
reports while audio is playing. If active traffic is needed, agree on one
supported query, such as battery status, and its expected response before
sending anything. Verify playback afterward and review captured reports for
device identifiers before committing them. Build report replay or a dummy
after real exchanges and independent expectations are available. Keep
Dolby as a separate study.

[HeadsetControl's A50 driver]: https://github.com/Sapd/HeadsetControl/blob/6b3f7788c9561a467cad7a90576ccec15e10f6bf/lib/devices/logitech_astro_a50.hpp
[Logitech's model comparison]: https://www.logitechg.com/en-us/discover/learn/console-gaming-gift-guide
[HID++ long collection filter]: ../../crates/openlogi-hid/src/transport.rs
[standalone gaming-headset identity]: ../../crates/openlogi-device-registry/src/headset.rs
[identity-only inventory for the G735]: ../../crates/openlogi-device/src/inventory/standalone.rs
[HID++ sidetone operation]: ../../crates/openlogi-device/src/write/sidetone.rs
