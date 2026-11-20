use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Labels {
    pub(super) general_title: String,
    pub(super) general_overlay: String,
    pub(super) general_dictation: String,
    pub(super) general_reinsert: String,
    pub(super) general_mode: String,
    pub(super) general_hold: String,
    pub(super) general_toggle: String,
    pub(super) general_microphone: String,
    pub(super) general_default_device: String,
    pub(super) general_missing: String,
    pub(super) general_microphone_test: String,
    pub(super) general_stop_test: String,
    pub(super) general_level: String,
    pub(super) general_test_active: String,
    pub(super) general_test_ok: String,
    pub(super) general_test_silent: String,
    pub(super) general_test_unavailable: String,
    pub(super) general_model: String,
    pub(super) general_auto_model: String,
    pub(super) general_installed: String,
    pub(super) general_not_installed: String,
    pub(super) general_download: String,
    pub(super) general_remove: String,
    pub(super) general_confirm_remove: String,
    pub(super) general_removed: String,
    pub(super) general_tones: String,
    pub(super) general_autostart: String,
    pub(super) general_volume: String,
    pub(super) general_record: String,
    pub(super) general_conflict: String,
    pub(super) general_unsupported: String,
    pub(super) general_keep: String,
    pub(super) general_failure: String,
}

pub(super) static LABELS: LazyLock<Labels> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../ui/src/locales/de.json"))
        .expect("The embedded general settings labels must be valid")
});
