use super::*;

fn bounds(x: i32, y: i32, width: i32, height: i32) -> Bounds {
    Bounds {
        x,
        y,
        width,
        height,
    }
}

fn monitors() -> [Monitor; 2] {
    [
        Monitor {
            id: "left".into(),
            work: bounds(-1600, -100, 1600, 1000),
            primary: false,
            dpi: 144,
        },
        Monitor {
            id: "main".into(),
            work: bounds(0, 40, 1920, 1040),
            primary: true,
            dpi: 96,
        },
    ]
}

#[test]
fn new_placement_is_bottom_center_of_primary_work_area() {
    assert_eq!(
        SavedOverlay::default().restored(&monitors(), 160, 36),
        Some(bounds(880, 1028, 160, 36)),
    );
}

#[test]
fn moving_between_monitors_keeps_both_positions_and_uses_destination_dpi() {
    let mut saved = SavedOverlay::default();
    let mut screens = monitors();
    saved.record(&screens, bounds(50, 90, 160, 36));
    let dragged = bounds(-1550, -80, 240, 54);
    saved.record(&screens, dragged);
    assert_eq!(saved.positions.len(), 2);
    assert_eq!(saved.positions["main"], Offset { x: 50, y: 50 });
    assert_eq!(saved.restored(&screens, 160, 36), Some(dragged));

    screens[0].work.x = 1920;
    assert_eq!(
        saved.restored(&screens, 160, 36),
        Some(bounds(1970, -80, 240, 54))
    );
}

#[test]
fn unplugging_the_last_display_falls_back_until_it_returns() {
    let mut saved = SavedOverlay::default();
    let screens = monitors();
    let dragged = bounds(-400, 700, 240, 54);
    saved.record(&screens, dragged);
    assert_eq!(
        saved.restored(&screens[1..], 160, 36),
        Some(bounds(880, 1028, 160, 36))
    );
    assert_eq!(saved.restored(&screens, 160, 36), Some(dragged));
}

#[test]
fn unplug_and_reconnect_restore_each_display_own_position() {
    let mut saved = SavedOverlay::default();
    let mut screens = monitors();
    let main_position = bounds(830, 480, 160, 36);
    let left_position = bounds(-450, 590, 240, 54);
    saved.record(&screens, main_position);
    saved.record(&screens, left_position);

    assert_eq!(saved.restored(&screens[1..], 160, 36), Some(main_position));
    assert_eq!(saved.restored(&screens, 160, 36), Some(left_position));

    screens[1].work.width = 800;
    assert_eq!(
        saved.restored(&screens[1..], 160, 36),
        Some(bounds(320, 1028, 160, 36))
    );
    assert_eq!(saved.restored(&screens, 160, 36), Some(left_position));
}

#[test]
fn drop_outside_every_display_uses_default_even_with_a_saved_primary_position() {
    let mut saved = SavedOverlay::default();
    let screens = monitors();
    saved.record(&screens, bounds(50, 100, 160, 36));
    saved.record(&screens, bounds(3000, 2000, 160, 36));
    assert_eq!(
        saved.restored(&screens, 160, 36),
        Some(bounds(880, 1028, 160, 36))
    );
}

#[test]
fn shrinking_work_area_corrects_partial_intersections_and_rejects_lost_positions() {
    let mut saved = SavedOverlay::default();
    let mut screens = monitors();
    saved.record(&screens, bounds(1700, 890, 160, 36));
    screens[1].work.width = 1750;
    screens[1].work.height = 870;
    assert_eq!(
        saved.restored(&screens, 160, 36),
        Some(bounds(1590, 874, 160, 36))
    );
    screens[1].work.width = 800;
    assert_eq!(
        saved.restored(&screens, 160, 36),
        Some(bounds(320, 858, 160, 36))
    );
}

#[test]
fn dropping_between_displays_uses_largest_visible_area_and_recovers_from_a_gap() {
    let mut saved = SavedOverlay::default();
    let screens = monitors();
    saved.record(&screens, bounds(-120, 80, 160, 36));
    assert_eq!(saved.last_monitor.as_deref(), Some("left"));
    let restored = saved.restored(&screens, 160, 36).unwrap();
    assert_eq!(restored, bounds(-240, 80, 240, 54));
    saved.record(&screens, bounds(i32::MAX, i32::MIN, 160, 36));
    assert_eq!(
        saved.restored(&screens, 160, 36),
        Some(bounds(880, 1028, 160, 36))
    );
    assert_eq!(saved.restored(&[], 160, 36), None);
}

#[test]
fn placement_preserves_legacy_positions_and_leaves_preferences_to_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Ordner mit ß").join("overlay.json");
    assert!(!path.exists());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, br#"{"preferences":{"showReady":true},"lastMonitor":"main","positions":{"main":{"x":12,"y":17}}}"#).unwrap();
    let mut store = OverlayStore::load(&path).unwrap();
    assert_eq!(
        store.saved.restored(&monitors(), 160, 36),
        Some(bounds(12, 57, 160, 36))
    );
    let screens = monitors();
    let dragged = bounds(-1200, 600, 240, 54);
    store.remember_on(&screens, dragged).unwrap();
    let reopened = OverlayStore::load(&path).unwrap();
    let data: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(data.get("preferences").is_none());
    assert_eq!(reopened.saved.restored(&screens, 160, 36), Some(dragged));
}

#[test]
fn failed_position_write_leaves_in_memory_location_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blocked").join("overlay.json");
    let mut store = OverlayStore::load(&path).unwrap();
    std::fs::write(dir.path().join("blocked"), []).unwrap();
    assert!(
        store
            .remember_on(&monitors(), bounds(100, 100, 160, 36))
            .is_err()
    );
    assert!(store.saved.last_monitor.is_none());
}
