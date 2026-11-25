# ASTRO A50 (Gen 5) battery measurement on Arch Linux

Measured on 2026-11-25 using the USB base station (`046d:0b1c`), interface 8,
vendor collection `ff32:0074`. The HID report descriptor matched the
[USB baseline]. Both reads used the verified Linux hidraw node with ordinary
user privileges. The [measurement data] preserves the exact HID report bytes
and lengths in a diagnostic JSON file.

## Observations

- **Passive window:** The 10-second read-only window yielded zero reports.
  PipeWire's Game playback node was running before and after. The idle HID
  observation does not establish which events can produce unsolicited reports.
- **Active window:** A single write submitted the 64-byte battery request
  `02 0c 03 00 06 0c` with 58 zero bytes. One 64-byte input report arrived
  about 2 ms after the start of the two-second measurement window. No other
  reports arrived and there were no retries.
- **Audio during the active query:** Both A50 playback nodes were suspended
  before and after the write. The query therefore did not test playback
  continuity under load. No audio route or device setting was changed and
  no dock or charging transition was induced.

## Response

The input report began `02 0c 06 00 06 0c 5d 5f 00`, followed by 55 zero
bytes. The header, command and six-byte length field agree with the
[reference driver]. The length counts bytes after the first three, so nine
bytes of the actual 64-byte input report are meaningful for this frame.

| Zero-based offset | Value | Observation |
| --- | --- | --- |
| 4 | `0x06` | Battery command |
| 5 | `0x0c` | Matched the request handle in this sample |
| 6 | `0x5d` | 93% when interpreted according to the reference driver |
| 7 | `0x5f` | Meaning unverified |
| 8 | `0x00` | No dock/charging indication under the reference driver's interpretation |

The physical dock position was not independently observed. A measurement
in this one state does not establish charging transitions or require the
response handle to equal the request handle. An earlier query delivered an
input that failed a stricter match; its bytes and length were not retained,
so it cannot be compared to the captured response.

## Reuse

The capture contains one battery exchange and no registered OpenLogi replay
scenario. Its actual bytes and lengths are suitable input for an offline
decoder test. Verify charging semantics and active-audio continuity on the
physical headset before treating either as covered by the capture.

[USB baseline]: astro-a50-linux.md
[measurement data]: astro-a50-gen5-battery.json
[reference driver]: https://github.com/Sapd/HeadsetControl/blob/6b3f7788c9561a467cad7a90576ccec15e10f6bf/lib/devices/logitech_astro_a50.hpp#L137-L147
