//! ASTRO A50 Gen 5 vendor reports, separate from HID++ and A50 X.
//!
//! The measured exchange and its provenance live in
//! `docs/diagnostics/astro-a50-gen5-battery.{md,json}`. The query uses the injected backend; this module never opens host devices directly.

use std::time::Duration;

use openlogi_core::device::{
    A50_VOLUME_MAX, BatteryInfo, BatteryLevel, BatteryStatus, CHAT_MIX_GAME_END,
};

use crate::backend::{BackendError, HidBackend, NodeInfo, RawChannel};

/// Query the A50 Gen 5 battery once, with a deadline covering open, write and read.
///
/// Only the registry's measured vendor collection may be addressed. Other input
/// reports are ignored within the same deadline, without retrying the request.
pub async fn query_battery(
    backend: &dyn HidBackend,
    node: &NodeInfo,
) -> Result<BatteryInfo, BackendError> {
    let battery = query(backend, node, 0x06, decode_battery_report).await?;
    Ok(BatteryInfo {
        percentage: Some(battery.percent()),
        level: BatteryLevel::Unknown,
        status: BatteryStatus::Unknown,
    })
}

/// Query the headset's hardware Game/Voice balance without changing it.
pub async fn query_chat_mix(backend: &dyn HidBackend, node: &NodeInfo) -> Result<u8, BackendError> {
    query(backend, node, 0x0a, decode_chat_mix_report).await
}

const VOLUME_COMMAND: u8 = 0x08;
const GET_HANDLE: u8 = 0x0c;
const WHEEL_HANDLE: u8 = 0x00;
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// An open A50 vendor channel that has completed its initial volume GET.
///
/// This owns the same channel as the GET, so events that follow the response
/// are available to the reader without a second open.
pub struct VolumeWatch(Box<dyn RawChannel>);

impl VolumeWatch {
    /// Wait for a spontaneous volume report. GET replies and unrelated input
    /// are ignored.
    pub async fn next_step(&mut self) -> Result<u8, BackendError> {
        read_matching(&mut *self.0, |report| {
            volume_with_handle(report, WHEEL_HANDLE)
        })
        .await
    }
}

/// Read the current hardware step and keep the channel open for wheel updates.
pub async fn open_volume_watch(
    backend: &dyn HidBackend,
    node: &NodeInfo,
) -> Result<(u8, VolumeWatch), BackendError> {
    let (step, channel) = query_with_channel(backend, node, VOLUME_COMMAND, |report| {
        volume_with_handle(report, GET_HANDLE)
    })
    .await?;
    Ok((step, VolumeWatch(channel)))
}

fn volume_with_handle(report: &[u8], handle: u8) -> Result<u8, VolumeReportError> {
    if report.get(5).copied() != Some(handle) {
        return Err(VolumeReportError::Frame);
    }
    decode_volume_report(report)
}

async fn query<T, E>(
    backend: &dyn HidBackend,
    node: &NodeInfo,
    command: u8,
    decode: impl Fn(&[u8]) -> Result<T, E>,
) -> Result<T, BackendError> {
    query_with_channel(backend, node, command, decode)
        .await
        .map(|(reading, _)| reading)
}

async fn query_with_channel<T, E>(
    backend: &dyn HidBackend,
    node: &NodeInfo,
    command: u8,
    decode: impl Fn(&[u8]) -> Result<T, E>,
) -> Result<(T, Box<dyn RawChannel>), BackendError> {
    if !openlogi_device_registry::headset::matches_astro_a50_gen5(
        node.vendor_id,
        node.product_id,
        node.usage_page,
        node.usage_id,
    ) {
        return Err(BackendError::Backend(
            "not the A50 Gen 5 vendor collection".into(),
        ));
    }
    tokio::time::timeout(QUERY_TIMEOUT, async {
        let mut channel = backend.open_raw_channel(node).await?;
        let mut request = [0; 64];
        request[..6].copy_from_slice(&[0x02, 0x0c, 0x03, 0, command, GET_HANDLE]);
        channel.write_output_report(&request).await?;
        let reading = read_matching(&mut *channel, decode).await?;
        Ok((reading, channel))
    })
    .await
    .map_err(|_| BackendError::Backend("A50 Gen 5 query timed out".into()))?
}

async fn read_matching<T, E>(
    channel: &mut dyn RawChannel,
    decode: impl Fn(&[u8]) -> Result<T, E>,
) -> Result<T, BackendError> {
    loop {
        let mut report = [0; 65];
        let length = channel.read_input_report(&mut report).await?;
        if length == 0 {
            return Err(BackendError::Disconnected);
        }
        let Some(bytes) = report.get(..length) else {
            return Err(BackendError::Backend("invalid raw HID report size".into()));
        };
        if let Ok(reading) = decode(bytes) {
            return Ok(reading);
        }
        // Give timeouts and suspension a chance even on a busy channel.
        tokio::task::yield_now().await;
    }
}

/// A validated battery reply from the A50 Gen 5 vendor interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryReport {
    percent: u8,
    /// Transaction token as received; replies need not echo the request token.
    pub handle: u8,
    /// Raw byte 7. It matched G HUB’s percentage in the measured powered-on states.
    pub secondary_level_raw: u8,
    /// Raw byte 8. Observed values include 0x00, 0x01 and 0x03.
    pub status_raw: u8,
}

impl BatteryReport {
    /// Battery percentage from byte 6 in the inclusive range 0–100.
    #[must_use]
    pub const fn percent(self) -> u8 {
        self.percent
    }
}

/// Why a report cannot be accepted as an A50 Gen 5 battery reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BatteryReportError {
    /// The HID report does not contain exactly 64 bytes, including report ID.
    #[error("A50 Gen 5 battery report must contain 64 bytes, got {0}")]
    ReportSize(usize),
    /// Report ID, marker, reserved header byte or command does not match.
    #[error("not an A50 Gen 5 battery reply")]
    Header,
    /// The declared payload is not the measured three-byte battery payload.
    #[error("unsupported A50 Gen 5 battery reply length {0}")]
    PayloadLength(u8),
    /// The reported percentage is outside the supported range.
    #[error("invalid A50 Gen 5 battery percentage {0}")]
    Percentage(u8),
}

/// Decode one complete 64-byte battery input report, including report ID.
///
/// Accepts any transaction handle and preserves the secondary level and status
/// bytes verbatim. Transport padding is ignored.
/// Unrelated commands, bare acknowledgements and unknown payload layouts are
/// rejected rather than interpreted as battery values.
pub fn decode_battery_report(report: &[u8]) -> Result<BatteryReport, BatteryReportError> {
    if report.len() != 64 {
        return Err(BatteryReportError::ReportSize(report.len()));
    }
    if report[0] != 0x02 || report[1] != 0x0c || report[3] != 0 || report[4] != 0x06 {
        return Err(BatteryReportError::Header);
    }
    // LEN counts reserved byte, command, handle and the three payload bytes.
    if report[2] != 6 {
        return Err(BatteryReportError::PayloadLength(report[2]));
    }
    if report[6] > 100 {
        return Err(BatteryReportError::Percentage(report[6]));
    }
    Ok(BatteryReport {
        percent: report[6],
        handle: report[5],
        secondary_level_raw: report[7],
        status_raw: report[8],
    })
}

/// Why a report cannot be accepted as an A50 Gen 5 ChatMix reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ChatMixReportError {
    /// Invalid report size, header or declared payload length.
    #[error("not a complete A50 Gen 5 ChatMix reply")]
    Frame,
    /// The reported step exceeds the Game endpoint.
    #[error("invalid A50 Gen 5 ChatMix step {0}")]
    Step(u8),
}

/// Decode the ChatMix step from a complete A50 Gen 5 input report.
///
/// The measured landmarks are 0 (Voice), 6 (center) and 12 (Game).
/// Intermediate steps are returned as raw positions, without inferring gain.
pub fn decode_chat_mix_report(report: &[u8]) -> Result<u8, ChatMixReportError> {
    if report.len() != 64 {
        return Err(ChatMixReportError::Frame);
    }
    match report {
        [
            0x02,
            0x0c,
            0x04,
            0x00,
            0x0a,
            _,
            step @ 0..=CHAT_MIX_GAME_END,
            ..,
        ] => Ok(*step),
        [0x02, 0x0c, 0x04, 0x00, 0x0a, _, step, ..] => Err(ChatMixReportError::Step(*step)),
        _ => Err(ChatMixReportError::Frame),
    }
}

/// Why an input report cannot be accepted as an A50 Gen 5 volume reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VolumeReportError {
    /// Invalid report size, header or declared payload length.
    #[error("not a complete A50 Gen 5 volume reply")]
    Frame,
    /// The reported step is above the hardware maximum.
    #[error("invalid A50 Gen 5 volume step {0}")]
    Step(u8),
}

/// Decode the measured hardware volume step from byte 8 of a volume reply.
pub fn decode_volume_report(report: &[u8]) -> Result<u8, VolumeReportError> {
    if report.len() != 64 {
        return Err(VolumeReportError::Frame);
    }
    let [0x02, 0x0c, 0x06, 0x00, 0x08, _, _, _, step, ..] = report else {
        return Err(VolumeReportError::Frame);
    };
    if *step > A50_VOLUME_MAX {
        return Err(VolumeReportError::Step(*step));
    }
    Ok(*step)
}

#[cfg(test)]
mod tests {
    use super::{
        BatteryReportError, ChatMixReportError, VolumeReportError, decode_battery_report,
        decode_chat_mix_report, decode_volume_report,
    };

    #[test]
    fn measured_arch_volume_reports_decode_the_device_step() {
        // Captured header and raw/step pairs, followed by 55 zero bytes.
        for (raw, wire_step, expected) in [
            (0x19, 0x11, 17),
            (0x1c, 0x13, 19),
            (0x1f, 0x15, 21),
            (0x00, 0x00, 0),
            (0x11, 0x0c, 12),
        ] {
            let mut report = [0; 64];
            report[..9].copy_from_slice(&[0x02, 0x0c, 0x06, 0, 0x08, 0x0c, raw, 0, wire_step]);
            assert_eq!(decode_volume_report(&report), Ok(expected));
        }
    }

    #[test]
    fn rejects_out_of_range_and_other_volume_frames() {
        let mut report = [0; 64];
        report[..9].copy_from_slice(&[0x02, 0x0c, 0x06, 0, 0x08, 0x0c, 0x1f, 0, 22]);
        assert_eq!(
            decode_volume_report(&report),
            Err(VolumeReportError::Step(22))
        );
        assert_eq!(
            decode_volume_report(&report[..63]),
            Err(VolumeReportError::Frame)
        );
        assert_eq!(
            decode_volume_report(&measured_report()),
            Err(VolumeReportError::Frame)
        );
    }

    #[test]
    fn decodes_measured_arch_chat_mix_positions() {
        for (payload, step) in [(0x00, 0), (0x06, 6), (0x0c, 12)] {
            let mut report = [0; 64];
            report[..7].copy_from_slice(&[0x02, 0x0c, 0x04, 0x00, 0x0a, 0x0c, payload]);
            assert_eq!(decode_chat_mix_report(&report), Ok(step));
        }
    }

    #[test]
    fn rejects_chat_mix_outside_range_and_unrelated_frames() {
        let mut report = [0; 64];
        report[..7].copy_from_slice(&[0x02, 0x0c, 0x04, 0x00, 0x0a, 0x0c, 0x0d]);
        assert_eq!(
            decode_chat_mix_report(&report),
            Err(ChatMixReportError::Step(13))
        );
        assert_eq!(
            decode_chat_mix_report(&report[..63]),
            Err(ChatMixReportError::Frame)
        );
        report[2] = 3;
        assert_eq!(
            decode_chat_mix_report(&report),
            Err(ChatMixReportError::Frame)
        );
        assert_eq!(
            decode_chat_mix_report(&measured_report()),
            Err(ChatMixReportError::Frame)
        );
    }

    fn measured_report() -> Vec<u8> {
        let capture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../docs/diagnostics/astro-a50-gen5-battery.json"
        ))
        .expect("valid measured capture");
        let report = &capture["battery_query"]["input_reports"][0];
        let bytes: Vec<u8> = report["hex"]
            .as_str()
            .expect("report hex")
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).expect("hex byte"))
            .collect();
        assert_eq!(report["length_bytes"], bytes.len());
        bytes
    }

    #[test]
    fn windows_replies_preserve_both_levels_and_each_status() {
        // The Windows capture used the same levels in all three states.
        for status in [0x00, 0x01, 0x03] {
            let mut report = [0; 64];
            report[..9].copy_from_slice(&[0x02, 0x0c, 0x06, 0x00, 0x06, 0x0c, 0x5c, 0x5f, status]);
            let battery = decode_battery_report(&report).expect("measured Windows reply");
            assert_eq!(battery.percent(), 92);
            assert_eq!(battery.secondary_level_raw, 95);
            assert_eq!(battery.status_raw, status);
        }
    }

    #[test]
    fn measured_reply_is_93_percent_with_uninterpreted_fields() {
        let battery = decode_battery_report(&measured_report()).expect("measured battery reply");
        assert_eq!(battery.percent(), 93);
        assert_eq!(battery.handle, 0x0c);
        assert_eq!(battery.secondary_level_raw, 0x5f);
        assert_eq!(battery.status_raw, 0);
    }

    #[test]
    fn rejects_every_truncation_and_oversized_reports() {
        let mut report = measured_report();
        for length in 0..64 {
            assert_eq!(
                decode_battery_report(&report[..length]),
                Err(BatteryReportError::ReportSize(length))
            );
        }
        report.push(0);
        assert_eq!(
            decode_battery_report(&report),
            Err(BatteryReportError::ReportSize(65))
        );
    }

    #[test]
    fn rejects_foreign_headers_and_non_battery_commands() {
        for index in [0, 1, 3, 4] {
            let mut report = measured_report();
            report[index] ^= 1;
            assert_eq!(
                decode_battery_report(&report),
                Err(BatteryReportError::Header)
            );
        }
    }

    #[test]
    fn rejects_acknowledgements_and_unknown_payload_layouts() {
        for length in [0, 3, 4, 5, 7, 63, 255] {
            let mut report = measured_report();
            report[2] = length;
            assert_eq!(
                decode_battery_report(&report),
                Err(BatteryReportError::PayloadLength(length))
            );
        }
    }

    #[test]
    fn bounds_percentage_without_interpreting_unknown_fields() {
        let mut report = measured_report();
        report[5] = 0;
        report[7] = 255;
        report[8] = 0x80;
        report[63] = 0xab;
        for percent in [0, 100] {
            report[6] = percent;
            let battery = decode_battery_report(&report).expect("valid boundary percentage");
            assert_eq!(battery.percent(), percent);
            assert_eq!(battery.handle, 0);
            assert_eq!(battery.secondary_level_raw, 255);
            assert_eq!(battery.status_raw, 0x80);
        }
        for percent in [101, 255] {
            report[6] = percent;
            assert_eq!(
                decode_battery_report(&report),
                Err(BatteryReportError::Percentage(percent))
            );
        }
    }
}

#[cfg(test)]
mod query_tests;
