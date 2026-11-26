# Testing ASTRO A50 Gen 5 readouts

The implementation recognizes only the measured `046d:0b1c`, usage
`ff32:0074` vendor collection. This is **A50 Gen 5**, not A50 X. Audio
continues to use the system USB Audio driver; OpenLogi changes no audio routes.

Standalone enumeration sends separate 64-byte battery, ChatMix and hardware
volume requests. Each query has a two-second deadline covering open, write and
response. The agent refreshes the readings during normal inventory
reconciliation, including the recovery scan (approximately 29 seconds without
another event). Malformed or unrelated reports are ignored within the deadline.
A failed query leaves the headset visible and does not block the other readings.
The agent retains the last successful battery reading. ChatMix and volume are
shown when their current queries succeed.

Only battery percentage is interpreted. Charging status and the coarse battery
level remain unknown. The adjacent raw byte and dock/charging byte are decoded
but are not used to infer charging, headset connectivity or an empty battery.
No EQ, sidetone, volume, lighting, noise-gate or Dolby writes are implemented.
The native diagnostic recorder does not yet capture duplex vendor reports;
recording mode leaves the vendor reports unavailable.

## Windows comparison

Windows 11 measurements on 2026-11-25 used headset firmware 1.3.24
and G HUB 2026.7.1011216. Offsets are zero-based and include the HID
report ID. Each observed battery reply below was 64 bytes, including
55 zero bytes after the listed prefix.

| Physical state | Battery reply prefix | G HUB |
| --- | --- | --- |
| Powered on, undocked | `02 0c 06 00 06 0c 5c 5f 00` | 95% |
| Powered on, docked | `02 0c 06 00 06 0c 5c 5f 01` | 95%, charging |
| Powered off, docked | `02 0c 06 00 06 0c 5c 5f 03` | 100% |

Byte 6 was 92 and byte 7 was 95 in all three states. Byte 7 matched
the powered-on G HUB display. The basis for the 100% display and the
meaning of status `0x03` remain unconfirmed.

## Arch Linux dock comparison

The headset stayed on the dock throughout this sequence. The owner reported
it was fully charged at the start. Byte 6 stayed at `0x5b` (91) and byte 7
at `0x5f` (95) across all three 64-byte reports.

| Headset power | Battery reply prefix |
| --- | --- |
| Off | `02 0c 06 00 06 0c 5b 5f 03` |
| On | `02 0c 06 00 06 0c 5b 5f 01` |
| Off again | `02 0c 06 00 06 0c 5b 5f 03` |

Only byte 8 changed when the headset was turned on. The two powered-off
reports were byte-identical. Earlier Arch measurements recorded `0x00` off
the dock and `0x01` during charging on the dock and over USB-C. They also
recorded `0x01` with the headset powered off on the dock at 90%.
Dock position alone cannot explain the transition, and powered-off docked
samples have shown both `0x01` and `0x03`. The actual charging current was
unmeasured during the power toggle, so the sequence leaves the charge state open.

OpenLogi displays the reported percentage from byte 6 and leaves charging
status unknown. The decoder preserves byte 7 and byte 8 independently for
diagnostic use. The 91% reading is a device-reported value. Its relationship
to the physical state of charge remains undetermined.

## Local Linux hardware check

Read `AGENTS.md` and the device-diagnosis skill first. Confirm which app owns
the headset before starting a development agent. Coordinate any service stop
with the owner; do not run competing Logitech managers for this check.

Build the CLI, agent and desktop from the same commit:

```sh
cargo build --locked -p openlogi -p openlogi-agent -p openlogi-desktop
```

Use the same `OPENLOGI_PROFILE=dev` and XDG environment in all terminals.
Agent and clients now use IPC version 49; do not reuse an older debug agent.
Start the real development agent, then the CLI and GUI:

```sh
OPENLOGI_PROFILE=dev target/debug/openlogi-agent
# In another terminal, with the same environment:
OPENLOGI_PROFILE=dev target/debug/openlogi list
OPENLOGI_PROFILE=dev OPENLOGI_DEV_AGENT=0 target/debug/openlogi-desktop
```

Verify that the CLI says `inventory read from the running agent`, includes
`ASTRO A50 Gen 5` and prints a plausible percentage. The GUI should list the
headset and show the same percentage through its existing battery indicator.
A failed reading must not display 0% or imply that the device is charging.

Record the actual percentage, physical dock state, application versions and
whether playback was running before, during and after the query.

### ChatMix hardware result on Arch Linux

The Linux x86_64 test build (IPC 48) was tested against a physical
ASTRO A50 Gen 5 (`046d:0b1c`). CLI and native desktop both showed
`12/12 → 6/12 → 0/12 → 12/12` as the balance changed at the headset.
The battery reading stayed visible at 71% throughout.

Both ALSA playback outputs reported active in all 620 samples over roughly
two minutes. Vivaldi played through A50 Game and a silent test stream ran
through A50 Voice. Spotify stayed paused. This establishes sampled stream
activity during the queries. The silent Voice stream gives no evidence
about Voice audio audibility or dropouts. The separate [two-audible-stream
test](astro-a50-linux.md#measured-chatmix-positions) established the mix at
the three positions.

After the test, both apps were routed to A50 Game and the test agent was
stopped. The tested archive's SHA-256 was
`27e75e820b122492f909570b6782235aeed7e8b87d28945e1285db929ac12d0e`.

### Hardware volume on Arch Linux

On 2026-11-26 a direct HID probe sampled the volume wheel while the
PipeWire Game sink stayed at 100% (0.00 dB, both channels) and reported
`RUNNING` before and after all five probes, including at the silent hardware
minimum. Two upward wheel steps moved the reading from 17 to 19. At the
physical stops the headset reported 0 and 21.

The measured 64-byte GET starts with `02 0c 03 00 08 0c` and has 58 zero
bytes after that prefix. The 64-byte replies start with
`02 0c 06 00 08 0c`, then contain the three bytes below followed by
55 zero bytes. Byte offsets include the report ID at offset zero.

| Wheel observation | Bytes 6–8 (hex) | Step (byte 8) | Reply time |
| --- | --- | ---: | ---: |
| Baseline | `19 00 11` | 17 | 1.93 ms |
| Two steps up | `1c 00 13` | 19 | 1.78 ms |
| Maximum stop | `1f 00 15` | 21 | 1.41 ms |
| Silent minimum stop | `00 00 00` | 0 | 1.95 ms |
| Turned up from minimum | `11 00 0c` | 12 | 1.51 ms |

The reader uses byte 8 as the discrete headset step on the 0–21 scale,
independent of the OS volume. Byte 6 has no validated interpretation.
The passive observation in this run ended before the wheel was moved.
The spontaneous reports were captured separately below.

For a live OpenLogi retest, compare the **Headset volume** row in the device
details with `headset-volume=…/21` in `openlogi list` after a refresh.
Use a comfortable listening level. Compare the PipeWire Game sink volume
before and after turning the physical wheel. The Arch observations above
came from the direct probe, before this readback was integrated into OpenLogi.

### IPC 49 volume hardware test on Arch Linux

The Linux x86_64 test build read the volume of a physical ASTRO A50 Gen 5
through the running OpenLogi agent. Two downward steps on the headset wheel
changed the reading from `15/21` to `13/21`. Both the CLI and the native GUI
showed `13/21`. Battery (67%) and ChatMix (`6/12`) stayed visible.

The PipeWire A50 Game sink stayed at 100% (0.00 dB) on both channels.
The ALSA Game output was active in all 483 samples over roughly 97 seconds,
and the hardware playback pointer advanced between samples. This records
playback progress during polling. The temporary test processes were stopped
afterward.

The tested archive's SHA-256 was
`510e92d0096163148ef5cbb69b9e6319157d9f0dec17af2790836d175c7f4c06`.

### Spontaneous volume reports on Arch Linux

A 15-second passive read on interface 8 captured five 64-byte input
reports. No active HID query or OpenLogi agent was running. The tester
heard the volume change while operating the wheel.

| Seconds since capture start | Hardware step (byte 8) |
| ---: | ---: |
| 1.2054 | 12 |
| 1.7884 | 11 |
| 4.7904 | 12 |
| 5.3734 | 13 |
| 14.8744 | 12 |

Each report begins `02 0c 06 00 08 00`. Byte 5 is `0x00` in these reports
and was `0x0c` in the measured GET responses. The observed spontaneous
frames use the same volume command and step offset as those responses.
The [byte-exact capture](astro-a50-gen5-volume-wheel-arch.json) is used
by the decoder test.

The agent keeps the volume GET channel open between scans and publishes
matching handle-`0x00` wheel events to clients immediately. It closes the
reader before replacing the baseline in a scan or suspending device I/O.
The GET remains the initial and recovery reading. A failed listener is
reopened on the next successful scan.

### Live volume hardware test on Arch Linux

On 2026-11-26 the SHA-256-verified Linux x86_64 build from
`bfd540359844072e827cc6fd64599ff631da2539` (IPC 49) was tested with a
physical ASTRO A50 Gen 5 (`046d:0b1c`). The open device details showed
`14 → 13 → 12 → 13 → 14 → 15` as the wheel moved. Changes appeared in
the next screen captures. Exact GUI latency was unmeasured.

The CLI first showed the new values 9–202 ms after their corresponding
HID reports. Both clients ended at volume `15/21`, battery 82% and
ChatMix `6/12`.

The PipeWire A50 Game sink stayed at 100% (0.00 dB) on both channels.
The ALSA Game output was active in all 299 samples over roughly 60 seconds,
and the hardware playback pointer continued to advance. The temporary test
processes were stopped afterward, and the normal OpenLogi service returned
to its previous inactive state.

The tested archive's SHA-256 was
`80181ee0147b2628998b9c7f4e23836477f3cacc7ab6de610ede7488f2441794`.

### USB reconnect on Arch Linux

The same build recovered after the cable was unplugged and reinserted at
the PC port. The agent and GUI stayed running. From the udev USB add event,
HID was first observed readable after 16.2 ms. The first complete
agent-backed CLI snapshot appeared after 2.42 s with battery 76%, ChatMix
`6/12` and volume `13/21`. The device tile was visible by 3.57 s.

The agent-backed CLI subsequently showed `13 → 12 → 11 → 12 → 13` as
the wheel moved. The sampled GUI frames showed `13 → 12 → 13` and
agreed with the CLI on the final step. The test processes were stopped, the
normal service returned to its previous inactive state, and the base was
left on PC.

### Bluetooth connection reports on Arch Linux

With the headset on and undocked and the PC input selected, interface 8
remained reachable through a phone disconnect, connect and disconnect.
The connection byte at offset 6 read `00 → 01 → 00`. The Bluetooth LED
was lit with the phone connected and off after the second disconnect.
Both transitions also produced spontaneous reports in a separate passive
capture. The complete frames consist of these prefixes and zero padding
to 64 bytes.

| Report | Prefix (hex) |
| --- | --- |
| GET request | `02 0c 03 00 0e 0d` |
| GET reply, disconnected | `02 0c 04 00 0e 0d 00` |
| GET reply, connected | `02 0c 04 00 0e 0d 01` |
| Spontaneous connect | `02 0c 04 00 0e 00 01` |
| Spontaneous disconnect | `02 0c 04 00 0e 00 00` |

Two other reports were recorded by header only (`02 0c 07 00 0c 00`).
Their meaning is unknown. The confirmed Bluetooth reports describe link
state and contain no peer name or address.

### PLAYSYNC on Arch Linux

With Bluetooth disconnected and only the PC port cabled, the base
indicated `PC → XBOX → PS → PC`. Arch saw USB and HID removal on the
first transition, no device while Xbox or PlayStation was selected, and
enumeration as `046d:0b1c` on return to PC. No spontaneous PLAYSYNC
report was captured at the PC port. PC-side USB removal alone does not
identify which of the console inputs was selected.

## Offline evidence

`astro-a50-gen5-battery.json` contains the measured request and reply. Tests
exercise those exact bytes through the injected duplex backend, the timeout
and write-failure paths, wrong collections and ambiguous devices. Separate
tests cover IPC encoding, retention after a failed poll, CLI formatting and
the desktop device record's battery field. No new dependency is required.
