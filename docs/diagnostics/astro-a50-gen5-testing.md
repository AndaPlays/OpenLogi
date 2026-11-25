# Testing ASTRO A50 Gen 5 battery integration

The implementation recognizes only the measured `046d:0b1c`, usage
`ff32:0074` vendor collection. This is **A50 Gen 5**, not A50 X. Audio
continues to use the system USB Audio driver; OpenLogi changes no audio routes.

Standalone enumeration sends one 64-byte battery request, with a two-second
deadline covering open, write and response. The agent refreshes it during its
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

## Local Linux hardware check

Read `AGENTS.md` and the device-diagnosis skill first. Confirm which app owns
the headset before starting a development agent. Coordinate any service stop
with the owner; do not run competing Logitech managers for this check.

Build the CLI, agent and desktop from the same commit:

```sh
cargo build --locked -p openlogi -p openlogi-agent -p openlogi-desktop
```

Use the same `OPENLOGI_PROFILE=dev` and XDG environment in all terminals.
Agent and clients now use IPC version 47; do not reuse an older debug agent.
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
whether playback was running before, during and after the query. The original
active capture was made while both playback outputs were suspended: uninterrupted
playback during active queries and charging behavior remain unverified on hardware.
Do not treat the replay or a successful build as evidence for either behavior.

## Offline evidence

`astro-a50-gen5-battery.json` contains the measured request and reply. Tests
exercise those exact bytes through the injected duplex backend, the timeout
and write-failure paths, wrong collections and ambiguous devices. Separate
tests cover IPC encoding, retention after a failed poll, CLI formatting and
the desktop device record's battery field. No new dependency is required.
