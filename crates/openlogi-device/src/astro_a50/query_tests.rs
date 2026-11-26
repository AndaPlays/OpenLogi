//! Tests at the backend seam: the measured exchange, deadlines and discovery.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use hidpp::async_trait;
use hidpp::channel::HidppChannel;

use super::{open_volume_watch, query_battery};
use crate::backend::{
    BackendError, HidBackend, HotplugStream, NodeId, NodeInfo, RawChannel, RawWriter,
};

type Writes = Arc<Mutex<Vec<Vec<u8>>>>;

struct Channel {
    reports: VecDeque<Vec<u8>>,
    writes: Writes,
    fail_write: bool,
}

#[async_trait]
impl RawWriter for Channel {
    async fn write_output_report(&mut self, report: &[u8]) -> Result<(), BackendError> {
        self.writes.lock().unwrap().push(report.to_vec());
        if self.fail_write {
            return Err(BackendError::Disconnected);
        }
        Ok(())
    }
}

#[async_trait]
impl RawChannel for Channel {
    async fn read_input_report(&mut self, buffer: &mut [u8]) -> Result<usize, BackendError> {
        let Some(report) = self.reports.pop_front() else {
            return std::future::pending().await;
        };
        buffer[..report.len()].copy_from_slice(&report);
        Ok(report.len())
    }
}

struct Backend {
    nodes: Vec<NodeInfo>,
    channel: Mutex<Option<Channel>>,
    chat_mix_channel: Mutex<Option<Channel>>,
    volume_channel: Mutex<Option<Channel>>,
}

#[async_trait]
impl HidBackend for Backend {
    async fn enumerate(&self) -> Result<Vec<NodeInfo>, BackendError> {
        Ok(self.nodes.clone())
    }
    async fn enumerate_hidpp(&self) -> Result<Vec<NodeInfo>, BackendError> {
        Ok(Vec::new())
    }
    async fn open_hidpp(&self, _: &NodeInfo) -> Result<Option<Arc<HidppChannel>>, BackendError> {
        panic!("A50 must not be probed as HID++")
    }
    async fn open_raw_writer(&self, _: &NodeInfo) -> Result<Box<dyn RawWriter>, BackendError> {
        panic!("A50 needs a duplex connection")
    }
    async fn open_raw_channel(&self, _: &NodeInfo) -> Result<Box<dyn RawChannel>, BackendError> {
        let channel = self
            .channel
            .lock()
            .unwrap()
            .take()
            .or_else(|| self.chat_mix_channel.lock().unwrap().take())
            .or_else(|| self.volume_channel.lock().unwrap().take())
            .ok_or(BackendError::Disconnected)?;
        Ok(Box::new(channel))
    }
    fn watch(&self) -> Result<HotplugStream, BackendError> {
        Ok(Box::new(futures_lite::stream::empty()))
    }
}

fn node() -> NodeInfo {
    NodeInfo {
        id: NodeId::from("astro-test".to_owned()),
        vendor_id: 0x046d,
        product_id: 0x0b1c,
        usage_page: 0xff32,
        usage_id: 0x0074,
        name: "A50".into(),
        manufacturer: None,
        serial_number: None,
    }
}

fn capture_report(key: &str) -> Vec<u8> {
    let capture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/diagnostics/astro-a50-gen5-battery.json"
    ))
    .unwrap();
    let exchange = &capture["battery_query"];
    let report = if key == "output_report" {
        &exchange[key]
    } else {
        &exchange[key][0]
    };
    report["hex"]
        .as_str()
        .unwrap()
        .split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn backend(reports: Vec<Vec<u8>>) -> (Backend, Writes) {
    let writes = Arc::new(Mutex::new(Vec::new()));
    (
        Backend {
            nodes: vec![node()],
            chat_mix_channel: Mutex::new(None),
            volume_channel: Mutex::new(None),
            channel: Mutex::new(Some(Channel {
                reports: reports.into(),
                writes: writes.clone(),
                fail_write: false,
            })),
        },
        writes,
    )
}

#[tokio::test(start_paused = true)]
async fn wheel_stream_publishes_measured_reports_without_more_output_requests() {
    let capture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/diagnostics/astro-a50-gen5-volume-wheel-arch.json"
    ))
    .expect("valid passive Arch capture");
    let reports = capture["reports"].as_array().expect("captured reports");
    let wheel_reports: Vec<Vec<u8>> = reports
        .iter()
        .map(|captured| {
            let report: Vec<_> = captured["hex"]
                .as_str()
                .expect("volume report hex")
                .split_whitespace()
                .map(|byte| u8::from_str_radix(byte, 16).expect("hex byte"))
                .collect();
            assert_eq!(captured["length"], report.len());
            assert_eq!(report[5], 0x00);
            report
        })
        .collect();

    // An event already queued before the GET reply is not the baseline.
    let mut initial_reply = wheel_reports[0].clone();
    initial_reply[5] = 0x0c;
    initial_reply[8] = 17;
    let mut queued = vec![
        wheel_reports[0].clone(),
        initial_reply.clone(),
        initial_reply,
    ];
    queued.extend(wheel_reports);
    let (backend, writes) = backend(queued);
    let (initial, mut watch) = open_volume_watch(&backend, &node()).await.unwrap();
    assert_eq!(initial, 17);

    for expected in [12, 11, 12, 13, 12] {
        let step = tokio::time::timeout(std::time::Duration::from_secs(1), watch.next_step())
            .await
            .expect("wheel report available without a rescan")
            .unwrap();
        assert_eq!(step, expected);
    }

    let mut request = vec![0; 64];
    request[..6].copy_from_slice(&[0x02, 0x0c, 0x03, 0, 0x08, 0x0c]);
    assert_eq!(*writes.lock().unwrap(), vec![request]);
}

#[tokio::test]
async fn measured_exchange_populates_discovery_without_inventing_charge_status() {
    let response = capture_report("input_reports");
    let mut unrelated = response.clone();
    unrelated[4] = 0x08;
    let mut ack = response.clone();
    ack[2] = 3;
    let (backend, writes) = backend(vec![unrelated, ack, response[..8].to_vec(), response]);
    let devices = crate::inventory::standalone::enumerate_standalone(&backend)
        .await
        .unwrap();
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].display_name, "ASTRO A50 Gen 5");
    assert_eq!(devices[0].kind, openlogi_core::device::DeviceKind::Headset);
    assert_eq!(devices[0].chat_mix, None);
    let battery = devices[0].battery.as_ref().unwrap();
    assert_eq!(battery.percentage, Some(93));
    assert_eq!(
        battery.status,
        openlogi_core::device::BatteryStatus::Unknown
    );
    assert_eq!(battery.level, openlogi_core::device::BatteryLevel::Unknown);
    assert_eq!(
        *writes.lock().unwrap(),
        vec![capture_report("output_report")]
    );
}

#[tokio::test(start_paused = true)]
async fn chat_mix_is_reported_when_battery_times_out() {
    let (backend, writes) = backend(Vec::new());
    let mut report = vec![0; 64];
    report[..7].copy_from_slice(&[0x02, 0x0c, 0x04, 0, 0x0a, 0x0c, 6]);
    let _ = backend.chat_mix_channel.lock().unwrap().replace(Channel {
        reports: [report].into(),
        writes: writes.clone(),
        fail_write: false,
    });
    let devices = crate::inventory::standalone::enumerate_standalone(&backend)
        .await
        .unwrap();
    assert!(devices[0].battery.is_none());
    assert_eq!(devices[0].chat_mix, Some(6));
    let mut request = vec![0; 64];
    request[..6].copy_from_slice(&[0x02, 0x0c, 0x03, 0, 0x0a, 0x0c]);
    assert_eq!(
        *writes.lock().unwrap(),
        vec![capture_report("output_report"), request]
    );
}

#[tokio::test(start_paused = true)]
async fn measured_volume_reaches_inventory_when_other_queries_time_out() {
    let (backend, writes) = backend(Vec::new());
    let _ = backend.chat_mix_channel.lock().unwrap().replace(Channel {
        reports: VecDeque::new(),
        writes: writes.clone(),
        fail_write: false,
    });
    let mut reply = vec![0; 64];
    reply[..9].copy_from_slice(&[0x02, 0x0c, 0x06, 0, 0x08, 0x0c, 0x19, 0, 0x11]);
    let _ = backend.volume_channel.lock().unwrap().replace(Channel {
        reports: [reply].into(),
        writes: writes.clone(),
        fail_write: false,
    });

    let devices = crate::inventory::standalone::enumerate_standalone(&backend)
        .await
        .unwrap();
    assert_eq!(devices[0].headset_volume, Some(17));
    assert_eq!(devices[0].chat_mix, None);
    assert!(devices[0].battery.is_none());
    let mut request = vec![0; 64];
    request[..6].copy_from_slice(&[0x02, 0x0c, 0x03, 0, 0x08, 0x0c]);
    let writes = writes.lock().unwrap();
    assert_eq!(writes.len(), 3);
    assert_eq!(writes[2], request);
}

#[tokio::test(start_paused = true)]
async fn no_reply_is_bounded_and_keeps_the_headset_visible_without_a_battery() {
    let (backend, writes) = backend(Vec::new());
    let start = tokio::time::Instant::now();
    let devices = crate::inventory::standalone::enumerate_standalone(&backend)
        .await
        .unwrap();
    assert_eq!(
        tokio::time::Instant::now() - start,
        std::time::Duration::from_secs(2)
    );
    assert_eq!(devices.len(), 1);
    assert!(devices[0].battery.is_none());
    assert_eq!(writes.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn write_failure_is_not_retried() {
    let (backend, writes) = backend(Vec::new());
    backend.channel.lock().unwrap().as_mut().unwrap().fail_write = true;
    assert!(matches!(
        query_battery(&backend, &node()).await,
        Err(BackendError::Disconnected)
    ));
    assert_eq!(writes.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn wrong_collection_and_a50x_are_never_opened() {
    let (backend, writes) = backend(Vec::new());
    for field in 0..4 {
        let mut other = node();
        match field {
            0 => other.vendor_id ^= 1,
            1 => other.product_id ^= 1,
            2 => other.usage_page ^= 1,
            _ => other.usage_id ^= 1,
        }
        assert!(matches!(
            query_battery(&backend, &other).await,
            Err(BackendError::Backend(_))
        ));
    }
    let mut other = node();
    other.product_id = 0x0b1a;
    assert!(matches!(
        query_battery(&backend, &other).await,
        Err(BackendError::Backend(_))
    ));
    assert!(writes.lock().unwrap().is_empty());
    assert!(backend.channel.lock().unwrap().is_some());
}

#[tokio::test]
async fn indistinguishable_devices_are_rejected_before_querying() {
    let (mut backend, writes) = backend(Vec::new());
    let mut duplicate = node();
    duplicate.id = NodeId::from("other-node".to_owned());
    backend.nodes.push(duplicate);
    assert!(matches!(
        crate::inventory::standalone::enumerate_standalone(&backend).await,
        Err(crate::InventoryError::AmbiguousRawDevice)
    ));
    assert!(writes.lock().unwrap().is_empty());
}
