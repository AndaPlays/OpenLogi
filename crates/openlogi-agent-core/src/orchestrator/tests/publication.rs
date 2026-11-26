//! Inventory readiness and what every mutator republishes.

use super::*;

/// An *empty* snapshot still flips the health to `Ready`: the watcher only
/// forwards completed enumerations, so "checked and found nothing" must not
/// be reported as "still scanning" — that's the whole distinction the
/// health exists to carry.
#[test]
fn empty_refresh_marks_inventory_ready() {
    let mut orch = orchestrator(Config::default());
    assert_eq!(orch.inventory_health(), InventoryHealth::Scanning);
    orch.refresh_inventory(&[], &[], false);
    assert_eq!(orch.inventory_health(), InventoryHealth::Ready);
}

/// `Unavailable` is a startup-only downgrade: it reports "enumeration has
/// never worked", recovers when a snapshot finally lands, and never
/// clobbers a live device set on a mid-session failure (mirroring the
/// watcher's keep-last-snapshot policy).
#[test]
fn unavailable_only_downgrades_a_pending_inventory() {
    let mut orch = orchestrator(Config::default());
    orch.mark_inventory_unavailable();
    assert_eq!(orch.inventory_health(), InventoryHealth::Unavailable);
    orch.refresh_inventory(&[], &[], false);
    assert_eq!(orch.inventory_health(), InventoryHealth::Ready);
    orch.mark_inventory_unavailable();
    assert_eq!(orch.inventory_health(), InventoryHealth::Ready);
}

#[test]
fn every_inventory_mutator_republishes_what_the_ipc_server_answers() {
    let observable = Arc::new(ObservableState::new("test".to_string()));
    let mut orch = Orchestrator::new(Config::default(), Arc::clone(&observable));
    assert_eq!(
        observable.snapshot().status.inventory,
        InventoryHealth::Scanning,
        "a fresh agent has not enumerated yet"
    );

    orch.mark_inventory_unavailable();
    assert_eq!(
        observable.snapshot().status.inventory,
        InventoryHealth::Unavailable
    );

    orch.refresh_inventory(
        &[direct_inventory(Some("serial-1"), [1, 2, 3, 4])],
        &[],
        false,
    );
    let published = observable.snapshot();
    assert_eq!(published.status.inventory, InventoryHealth::Ready);
    assert_eq!(published.inventory, orch.inventory());
    assert_eq!(published.standalone, orch.standalone());
    assert_eq!(published.inventory.len(), 1);

    // A camera sample and a config reload are the other two facts the cell
    // carries; both must reach it from inside the mutator.
    orch.set_camera_active(true);
    assert!(observable.snapshot().camera_active);

    let mut config = Config::default();
    config.app_settings.launch_at_login = true;
    orch.reload_config(config);
    assert!(observable.snapshot().status.launch_at_login);
}

#[test]
fn settings_changes_only_publish_for_online_inventory_routes() {
    let observable = Arc::new(ObservableState::new("test".to_string()));
    let mut orch = Orchestrator::new(Config::default(), observable.clone());
    let mut inventory = direct_inventory(Some("synthetic-keyboard"), [1, 2, 3, 4]);
    let route = DeviceRoute::for_slot(&inventory, 0xff).unwrap();
    let mut observation = observable.subscribe();

    orch.announce_settings_changed(&route);
    assert!(
        !observation.has_changed().unwrap(),
        "scanning does not invent a route"
    );
    orch.mark_inventory_unavailable();
    observation.borrow_and_update();
    orch.announce_settings_changed(&route);
    assert!(
        !observation.has_changed().unwrap(),
        "unavailable is not ready"
    );

    inventory.paired[0].online = false;
    orch.refresh_inventory(&[inventory.clone()], &[], false);
    observation.borrow_and_update();
    orch.announce_settings_changed(&route);
    assert!(
        !observation.has_changed().unwrap(),
        "offline routes stay offline"
    );

    inventory.paired[0].online = true;
    inventory.paired[0].settings_revision = 5;
    orch.refresh_inventory(&[inventory.clone()], &[], false);
    observation.borrow_and_update();
    orch.announce_settings_changed(&route);
    assert!(observation.has_changed().unwrap());
    assert_eq!(
        observable.snapshot().inventory[0].paired[0].settings_revision,
        6
    );
    assert_eq!(
        orch.inventory(),
        &[inventory.clone()],
        "publication never mutates device-reported revisions"
    );

    inventory.paired[0].settings_revision = 6;
    orch.refresh_inventory(&[inventory], &[], false);
    assert_eq!(
        observable.snapshot().inventory[0].paired[0].settings_revision,
        7
    );
}

#[test]
fn headset_wheel_updates_reach_observers_without_reconciling_inventory() {
    let observable = Arc::new(ObservableState::new("test".into()));
    let mut orch = Orchestrator::new(Config::default(), Arc::clone(&observable));
    let mut headset = StandaloneDevice {
        address: RawDeviceAddress {
            vendor_id: 0x046d,
            product_id: 0x0b1c,
            usage_page: 0xff32,
            usage_id: 0x0074,
            identity: "id:astro-volume-test".into(),
        },
        display_name: "ASTRO A50 Gen 5".into(),
        manufacturer: None,
        serial_number: None,
        unit_id: [0; 4],
        kind: DeviceKind::Headset,
        online: true,
        capabilities: None,
        light_capabilities: None,
        driver_id: "logitech-gaming-headset".into(),
        registry_model_id: None,
        battery: Some(openlogi_core::device::BatteryInfo {
            percentage: Some(67),
            level: openlogi_core::device::BatteryLevel::Unknown,
            status: openlogi_core::device::BatteryStatus::Unknown,
        }),
        chat_mix: Some(6),
        headset_volume: Some(17),
    };
    let address = headset.address.clone();
    orch.refresh_inventory(&[], std::slice::from_ref(&headset), false);
    let mut subscription = observable.subscribe();

    for step in [12, 11, 12, 13, 12] {
        orch.update_headset_volume(&address, step);
        assert!(subscription.has_changed().unwrap());
        headset.headset_volume = Some(step);
        assert_eq!(
            subscription.borrow_and_update().snapshot.standalone,
            [headset.clone()]
        );
    }

    orch.update_headset_volume(&address, 12);
    assert!(!subscription.has_changed().unwrap());
    headset.online = false;
    orch.refresh_inventory(&[], std::slice::from_ref(&headset), false);
    let _ = subscription.borrow_and_update();
    orch.update_headset_volume(&address, 9);
    assert!(!subscription.has_changed().unwrap());
    assert_eq!(observable.snapshot().standalone, [headset]);
}

#[test]
fn equal_runtime_projection_does_not_wake_managers() {
    let mut orch = orchestrator(Config::default());
    orch.devices = vec![dev("a", 1, true)];
    orch.rebuild();
    let mut capture_plans = orch.shared.capture_plans.clone();
    let mut keyboard_specs = orch.shared.keyboard_specs.clone();
    let mut host_switch_links = orch.shared.host_switch_links.clone();
    let _ = capture_plans.borrow_and_update();
    let _ = keyboard_specs.borrow_and_update();
    let _ = host_switch_links.borrow_and_update();

    orch.publish_device_runtime();

    assert!(
        !capture_plans
            .has_changed()
            .expect("publication remains open")
    );
    assert!(
        !keyboard_specs
            .has_changed()
            .expect("publication remains open")
    );
    assert!(
        !host_switch_links
            .has_changed()
            .expect("publication remains open")
    );
}
