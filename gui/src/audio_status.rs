//! Telemetry lives in a persistent file. Nonzero data is not proof that an
//! audio processor is still running; require observed heartbeat movement.

use std::time::{Duration, Instant};

#[derive(Default)]
pub struct AudioStatus {
    sequence: Option<u32>,
    last_advance: Option<Instant>,
}

impl AudioStatus {
    pub fn observe(&mut self, sequence: u32, now: Instant) -> bool {
        if sequence == 0 {
            self.last_advance = None;
        } else if self.sequence.is_some_and(|previous| previous != sequence) {
            self.last_advance = Some(now);
        }
        self.sequence = Some(sequence);
        self.last_advance.is_some_and(|t| now.duration_since(t) < Duration::from_secs(2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_heartbeat_is_not_live() {
        let mut status = AudioStatus::default();
        let now = Instant::now();
        assert!(!status.observe(454_868_506, now));
        assert!(!status.observe(454_868_506, now + Duration::from_secs(5)));
        assert!(status.observe(454_868_507, now + Duration::from_secs(6)));
        assert!(!status.observe(454_868_507, now + Duration::from_secs(8)));
        assert!(status.observe(454_868_508, now + Duration::from_secs(9)));
    }

    #[test]
    fn zero_reset_and_counter_wrap_recover() {
        let mut status = AudioStatus::default();
        let now = Instant::now();
        assert!(!status.observe(u32::MAX, now));
        assert!(!status.observe(0, now));
        assert!(status.observe(1, now));
        assert!(!status.observe(0, now));
    }
}
