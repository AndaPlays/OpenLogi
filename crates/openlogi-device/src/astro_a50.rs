//! Battery queries and decoding of ASTRO A50 Gen 5 vendor reports, not HID++ or A50 X.
//!
//! The measured exchange and its provenance live in
//! `docs/diagnostics/astro-a50-gen5-battery.{md,json}`. The query uses the injected backend; this module never opens host devices directly.

use std::time::Duration;

use openlogi_core::device::{BatteryInfo, BatteryLevel, BatteryStatus};

use crate::backend::{BackendError, HidBackend, NodeInfo};

/// Query the A50 Gen 5 battery once, with a deadline covering open, write and read.
///
/// Only the registry's measured vendor collection may be addressed. Other input
/// reports are ignored within the same deadline, without retrying the request.
pub async fn query_battery(
    backend: &dyn HidBackend,
    node: &NodeInfo,
) -> Result<BatteryInfo, BackendError> {
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
    tokio::time::timeout(Duration::from_secs(2), async {
        let mut channel = backend.open_raw_channel(node).await?;
        let mut request = [0; 64];
        request[..6].copy_from_slice(&[0x02, 0x0c, 0x03, 0, 0x06, 0x0c]);
        channel.write_output_report(&request).await?;
        loop {
            let mut report = [0; 65];
            let length = channel.read_input_report(&mut report).await?;
            let Some(bytes) = report.get(..length) else {
                return Err(BackendError::Backend("invalid raw HID report size".into()));
            };
            if let Ok(battery) = decode_battery_report(bytes) {
                return Ok(BatteryInfo {
                    percentage: Some(battery.percent()),
                    level: BatteryLevel::Unknown,
                    status: BatteryStatus::Unknown,
                });
            }
            // Also bounds a backend that continuously returns unrelated reports.
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| BackendError::Backend("A50 Gen 5 battery query timed out".into()))?
}

/// A validated battery reply from the A50 Gen 5 vendor interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryReport {
    percent: u8,
    /// Transaction token as received; replies need not echo the request token.
    pub handle: u8,
    /// Byte adjacent to the percentage, whose meaning is not yet established.
    pub unknown_level: u8,
    /// Raw dock/charging field. Its semantics need further hardware validation.
    pub dock_charging_raw: u8,
}

impl BatteryReport {
    /// Battery percentage in the inclusive range 0–100.
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
/// Accepts any transaction handle and preserves the two fields whose semantics
/// have not been established on this unit. Transport padding is ignored.
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
        unknown_level: report[7],
        dock_charging_raw: report[8],
    })
}

#[cfg(test)]
mod tests {
    use super::{BatteryReportError, decode_battery_report};

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
    fn measured_reply_is_93_percent_with_uninterpreted_fields() {
        let battery = decode_battery_report(&measured_report()).expect("measured battery reply");
        assert_eq!(battery.percent(), 93);
        assert_eq!(battery.handle, 0x0c);
        assert_eq!(battery.unknown_level, 0x5f);
        assert_eq!(battery.dock_charging_raw, 0);
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
            assert_eq!(battery.unknown_level, 255);
            assert_eq!(battery.dock_charging_raw, 0x80);
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
