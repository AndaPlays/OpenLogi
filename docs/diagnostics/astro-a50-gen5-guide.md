# ASTRO A50 Gen 5 engineering reference

Source: Logitech's [A50 Wireless Headset Setup Guide (PDF)](https://www.logitech.com/assets/70203/2/a50_wireless_headset_web_qsg_amr.pdf).
Page numbers refer to the English section of the supplied PDF. The German
section begins on page 25. The guide describes controls and what users
can observe. The [Arch USB baseline](astro-a50-linux.md) and
[battery measurements](astro-a50-gen5-testing.md) describe the USB evidence
for the tested A50 Gen 5 (`046d:0b1c`). The guide occasionally says “A50 X”
or “HDMI Base Station”. Use the Gen 5 model identification and measured USB
interfaces when applying it to this device.

## Audio routing and controls

Logitech explains the two PC playback streams as a way to route applications
independently and balance game and chat audio at the headset (pp. 7–8, 12,
18–19). The guide specifies exactly the formats observed on Arch: **A50
Game** is stereo at up to 24-bit/48 kHz and **A50 Voice** is stereo at up
to 16-bit/48 kHz. It gives no design rationale for the different bit depths.
The microphone input is mono at up to 16-bit/48 kHz. StreamPort, also
called Stream Out, is a separate stereo **capture** stream of the mix for
recording or broadcasting. G HUB can adjust the contributions to that mix.

- The two paddles on the right earcup balance Game and Voice. Audible
  markers identify Game Max, center and Voice Max (p. 12). Use these markers
  and two separately routed USB streams to validate the three HID readings
  on Arch. The guide gives the physical landmarks. The reference driver
  and USB captures supply candidate report values.
- The right-earcup volume wheel controls the headset's own volume. On PC,
  this volume is independent of the OS mixer, with tones at minimum and
  maximum. Xbox is documented to synchronize volume and Game/Voice balance.
  Keep separate concepts for headset volume and system volume (pp. 13,
  15, 19).
- The microphone boom mutes at the headset when raised fully. The
  mechanical detent is a useful landmark when correlating a capture with
  a physical mute change (p. 12).
- Game/Voice balance is documented for the PC and Xbox ports. The guide
  excludes PlayStation, Switch and Bluetooth from this control (pp. 7,
  9–11, 17–19). Bluetooth audio arrives at the base station from a secondary
  device, which controls its own volume. The headset needs its link to the
  base to play that audio. USB Game and USB Voice can be tested with
  Bluetooth playback idle (pp. 8, 11, 19–20).
- PLAYSYNC changes the selected USB host. With three active hosts the
  order is Xbox → PlayStation → PC. The input indicator follows the
  selection. The headset must be on to switch while undocked. It can
  switch while docked with either power-switch position (p. 14). If this
  is tested later, record USB enumeration and audio routes before and after
  switching separately from the selected input indicator.
- The PC Dolby Atmos setup selects a spatial format for **A50 Game** in
  Windows Sound settings. The guide describes a hardware-bound license
  and an Xbox setup as well. It supplies no Linux activation path or HID
  command for Atmos (pp. 15–16, 18–19).

## Pairing and ordinary LED indications

For **Bluetooth pairing** (pp. 11, 19–20), power the base from its adapter,
turn on the headset and keep it within range of the base. The headset may
be docked or undocked. Hold the headset's Bluetooth button for **three
seconds**. **One dedicated Bluetooth indicator** on the front of the
base begins flashing. Select **A50** on the phone, tablet or other
Bluetooth host. The guide supplies neither a flash count or cadence nor
a confirmed connected LED pattern. It also notes that after powering off
the A50, the user should reconnect from the Bluetooth source. The guide
does not give a separate procedure or LED sequence for re-pairing the
headset to its base over LIGHTSPEED.

The four battery LEDs on the base indicate charge in **25% steps** (p. 21).
The separate power-switch LED on the headset has these documented states:

| Headset LED | Guide meaning |
| --- | --- |
| White | Powered on or battery full |
| Breathing white | Sleep |
| Orange | Charging |
| Steady red | Low battery |
| Breathing red | Critical, under 15 minutes of playback remaining |

For a battery capture, record the base LEDs and headset LED independently
alongside dock position, power switch, charging method and raw byte 8.
White can also mean powered on, and the battery LEDs give a coarse level.
Use an observed charging-to-full transition when investigating charge
completion. Keep the raw `0x00`, `0x01` and `0x03` values as separate
observations until they are correlated with those physical states.

## Docked power modes and the four-LED patterns

- **Off Mode** (p. 22). With the headset docked and its switch off, hold
  Bluetooth for three seconds until **all base LEDs go dark**. The guide
  says audio and PLAYSYNC stop in this mode. Undocking, power-cycling the
  base, or reconnecting USB wakes it. Normal operation may take a few
  seconds to return. Record this as an explicit action separate from
  switching the headset off.
- **Wireless Suspend** (pp. 23–24). With the headset docked and switched
  on, hold the Voice paddle to cycle the four battery LEDs through a
  selection menu. The guide says the pattern advances every 1.5 seconds.
  Release Voice on the desired pattern. The selection stays steadily lit
  for a couple of seconds, then the charge display returns. The guide
  says undocking re-enables all wireless features.

The following is a transcription of the page 23 illustration. Positions
run left to right across the four battery LEDs. A numbered LED is shown
highlighted. Other positions are drawn as dots.

| Hold time shown | Highlighted LEDs | Radio symbols in the guide |
| --- | --- | --- |
| 1.5 s | 1, 2, 3 | Wireless, Bluetooth and BLE shown enabled |
| 3 s | 1 | Wireless and Bluetooth crossed out |
| 4.5 s | 1, 2 | Wireless crossed out |
| 6 s | 1, 3 | Bluetooth crossed out |
| 9 s | 2, 3 | BLE crossed out |
| 10.5 s | 3 | Bluetooth and BLE crossed out |

The graphic gives no entry for 7.5 seconds. Use the pictured patterns as
manual observation cues and verify actual timing and radio behavior before
encoding a selector. The battery LEDs are context-dependent during this
mode. The Bluetooth connection flag observed over HID is a candidate for
testing connection changes. Pairing discovery, individual radio suspension
and the LED patterns still need their own physical and USB observations.
