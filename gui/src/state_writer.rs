//! Thin wrapper around tronteq_shared::StateHandle that tracks version for UI.

use anyhow::Result;
use tronteq_shared::{Band, Dynamics, Snapshot, StateHandle, NUM_BANDS};

pub struct StateWriter {
    handle: StateHandle,
    last_committed: u64,
}

impl StateWriter {
    pub fn open() -> Result<Self> {
        let handle = StateHandle::open_or_init()?;
        let snap = handle.snapshot();
        Ok(StateWriter { handle, last_committed: snap.version })
    }

    pub fn snapshot(&self) -> Snapshot {
        self.handle.snapshot()
    }

    pub fn write_state(
        &mut self,
        bands: &[Band; NUM_BANDS],
        bypass: bool,
        preamp_db: f32,
        dynamics: &Dynamics,
    ) {
        self.handle.write(|w| {
            w.set_bands(bands);
            w.set_bypass(bypass);
            w.set_preamp(preamp_db);
            w.set_dynamics(dynamics);
        });
        let snap = self.handle.snapshot();
        self.last_committed = snap.version;
    }

    pub fn version(&self) -> u64 {
        self.last_committed
    }
}
