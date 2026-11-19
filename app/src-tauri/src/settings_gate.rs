#[derive(Default)]
pub struct SettingsGate {
    ready: bool,
    pending: bool,
}

impl SettingsGate {
    pub fn request(&mut self) -> bool {
        if self.ready {
            true
        } else {
            self.pending = true;
            false
        }
    }

    pub fn finish_initialization(&mut self) -> bool {
        self.ready = true;
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_during_initialization_are_deferred_and_coalesced() {
        let mut gate = SettingsGate::default();
        assert!(!gate.request());
        assert!(!gate.request());
        assert!(gate.finish_initialization());
        assert!(gate.request());
    }

    #[test]
    fn startup_without_requests_keeps_settings_hidden() {
        let mut gate = SettingsGate::default();
        assert!(!gate.finish_initialization());
        assert!(gate.request());
    }
}
