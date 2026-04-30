use rusty_link::{AblLink, SessionState};

pub struct LinkSession {
    link: AblLink,
    enabled: bool,
    _quantum: f64,
}

impl LinkSession {
    pub fn new() -> Self {
        let link = AblLink::new(120.0); // initial tempo
        Self {
            link,
            enabled: false,
            _quantum: 4.0, // 4 beats per bar
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.link.enable(enabled);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_tempo(&self, bpm: f64) {
        if !self.enabled || bpm <= 0.0 {
            return;
        }
        let mut state = SessionState::new();
        self.link.capture_app_session_state(&mut state);
        let time = self.link.clock_micros();
        state.set_tempo(bpm, time);
        self.link.commit_app_session_state(&state);
    }

    pub fn num_peers(&self) -> u64 {
        self.link.num_peers()
    }

    pub fn tempo(&self) -> f64 {
        let mut state = SessionState::new();
        self.link.capture_app_session_state(&mut state);
        state.tempo()
    }
}
