# Testing ASTRO A50 Gen 5 battery integration

The implementation recognizes only the measured `046d:0b1c`, usage
`ff32:0074` vendor collection. This is **A50 Gen 5**, not A50 X. Audio
continues to use the system USB Audio driver; OpenLogi changes no audio routes.

Standalone enumeration sends separate 64-byte battery and ChatMix requests.
Each query has its own two-second deadline covering open, write and response. The agent refreshes it during its
normal inventory reconciliation, including the recovery scan (approximately
29 seconds without another event). There is no separate high-frequency poller.
Malformed or unrelated reports are ignored within that deadline. A failure
keeps the headset visible; the agent retains its last successful reading.
Direct CLI enumeration has no previous reading to retain.

Only battery percentage is interpreted. Charging status and the coarse battery
level remain unknown. The adjacent raw byte and dock/charging byte are decoded
but are not used to infer charging, headset connectivity or an empty battery.
No EQ, sidetone, volume, lighting, noise-gate or Dolby writes are implemented.
The native diagnostic recorder does not yet capture duplex vendor reports;
recording mode leaves this device's battery unavailable rather than silently
omitting its traffic from a recording.

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
Agent and clients now use IPC version 48; do not reuse an older debug agent.
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

## Offline evidence

`astro-a50-gen5-battery.json` contains the measured request and reply. Tests
exercise those exact bytes through the injected duplex backend, the timeout
and write-failure paths, wrong collections and ambiguous devices. Separate
tests cover IPC encoding, retention after a failed poll, CLI formatting and
the desktop device record's battery field. No new dependency is required.
