//! Confirmed sound state, bounded writes, and an explicit recovery path.
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tronteq_shared::{Band, Dynamics, Snapshot, StateError, StateHandle, Telemetry, NUM_BANDS};

#[derive(Clone, Debug)]
pub struct Problem {
    pub version: Option<u64>,
    pub message: String,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Backup {
    schema: u32,
    saved_unix_seconds: u64,
    snapshot: Snapshot,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RecoveryAction {
    Reload,
    Restore,
    ResetBypassed,
}

pub struct StateWriter {
    handle: StateHandle,
    confirmed: Option<Snapshot>,
    backup: Option<Backup>,
    backup_path: PathBuf,
    pub problem: Option<Problem>,
    pub backup_error: Option<String>,
    dirty: bool,
    retry_at: Instant,
}
impl StateWriter {
    pub fn open() -> anyhow::Result<Self> {
        std::fs::create_dir_all(tronteq_shared::STATE_DIR)?;
        Self::open_at(Path::new(tronteq_shared::STATE_FILE))
    }
    pub fn open_at(path: &Path) -> anyhow::Result<Self> {
        let handle = StateHandle::open_at(path)?;
        let backup_path = path.with_extension("last-good.json");
        let backup = std::fs::read(&backup_path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Backup>(&b).ok())
            .filter(|b| b.schema == 1 && b.snapshot.validate_recovery().is_ok());
        let mut me = Self {
            handle,
            confirmed: None,
            backup,
            backup_path,
            problem: None,
            backup_error: None,
            dirty: false,
            retry_at: Instant::now(),
        };
        let _ = me.reload();
        Ok(me)
    }
    /// This is a display snapshot when recovery is pending, never an implicit write.
    pub fn snapshot(&self) -> Snapshot {
        self.confirmed
            .or(self.backup.map(|b| b.snapshot))
            .unwrap_or_else(|| Snapshot::flat(true))
    }
    pub fn version(&self) -> u64 {
        self.confirmed.map_or(0, |s| s.version)
    }
    pub fn telemetry(&self) -> Telemetry {
        self.handle.telemetry()
    }
    pub fn recovery_sound(&self) -> Option<Snapshot> {
        self.confirmed.or(self.backup.map(|b| b.snapshot))
    }
    pub fn recovery_label(&self) -> String {
        if self.confirmed.is_some() {
            return "Last confirmed sound from this session".into();
        }
        match self.backup {
            Some(b) => {
                let age = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .saturating_sub(b.saved_unix_seconds);
                if age < 60 {
                    "Saved less than a minute ago".into()
                } else if age < 3600 {
                    format!("Saved {} minutes ago", age / 60)
                } else {
                    format!("Saved {} hours ago", age / 3600)
                }
            }
            None => "No valid recovery backup is available.".into(),
        }
    }
    fn fail(&mut self, error: StateError, version: Option<u64>) -> String {
        let version = match &error {
            StateError::Busy { version } => Some(*version),
            StateError::Changed { .. } => None,
            _ => version,
        };
        let message = error.to_string();
        self.problem = Some(Problem {
            version,
            message: message.clone(),
        });
        message
    }
    pub fn reload(&mut self) -> Result<Snapshot, String> {
        let s = self.handle.snapshot().map_err(|e| self.fail(e, None))?;
        // Legacy clients omitted dynamics; match the existing GUI migration rule.
        let s = if s.dynamics.comp_ratio == 0.0 {
            Snapshot {
                dynamics: Dynamics::default_passive(),
                ..s
            }
        } else {
            s
        };
        s.validate_recovery()
            .map_err(|e| self.fail(StateError::Invalid(e), Some(s.version)))?;
        self.confirmed = Some(s);
        self.dirty = true;
        self.problem = None;
        Ok(s)
    }
    pub fn write_state(
        &mut self,
        bands: &[Band; NUM_BANDS],
        bypass: bool,
        preamp_db: f32,
        dynamics: &Dynamics,
        delay_ms: f32,
    ) -> Result<(), String> {
        if let Some(p) = &self.problem {
            return Err(p.message.clone());
        }
        let mut s = Snapshot {
            version: self.version(),
            bands: *bands,
            bypass: u8::from(bypass),
            preamp_db,
            dynamics: *dynamics,
            delay_ms,
        };
        s.validate_recovery()
            .map_err(|e| self.fail(StateError::Invalid(e), Some(s.version)))?;
        s.version = self
            .handle
            .write(|w| {
                w.set_bands(bands);
                w.set_bypass(bypass);
                w.set_preamp(preamp_db);
                w.set_dynamics(dynamics);
                w.set_delay(delay_ms);
            })
            .map_err(|e| self.fail(e, None))?;
        self.confirmed = Some(s);
        self.dirty = true;
        Ok(())
    }
    pub fn recover(&mut self, action: RecoveryAction) -> Result<Snapshot, String> {
        if action == RecoveryAction::Reload {
            return self.reload();
        }
        let expected = self
            .problem
            .as_ref()
            .and_then(|p| p.version)
            .ok_or_else(|| {
                "Reload the current state before choosing a recovery action.".to_string()
            })?;
        let mut chosen = match action {
            RecoveryAction::Restore => self
                .recovery_sound()
                .ok_or_else(|| "No valid recovery backup is available.".to_string())?,
            RecoveryAction::ResetBypassed => Snapshot::flat(true),
            RecoveryAction::Reload => unreachable!(),
        };
        chosen.version = self
            .handle
            .recover(expected, &chosen)
            .map_err(|e| self.fail(e, None))?;
        self.confirmed = Some(chosen);
        self.problem = None;
        self.dirty = true;
        Ok(chosen)
    }
    /// Called after a gesture, never per drag frame. A failed backup does not
    /// misreport an applied audio edit or replace the previous valid backup.
    pub fn checkpoint(&mut self) {
        if !self.dirty || self.problem.is_some() || Instant::now() < self.retry_at {
            return;
        }
        let Some(snapshot) = self.confirmed else {
            return;
        };
        let backup = Backup {
            schema: 1,
            saved_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            snapshot,
        };
        let result = self.save_backup(&backup);
        match result {
            Ok(()) => {
                self.backup = Some(backup);
                self.dirty = false;
                self.backup_error = None;
            }
            Err(error) => {
                self.backup_error = Some(format!("Could not save sound recovery backup: {error}"));
                self.retry_at = Instant::now() + Duration::from_secs(2);
            }
        }
    }
    fn save_backup(&self, backup: &Backup) -> anyhow::Result<()> {
        use std::io::Write;
        let bytes = serde_json::to_vec_pretty(backup)?;
        let tmp = self
            .backup_path
            .with_extension(format!("json.{}.tmp", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        let result = (|| -> anyhow::Result<()> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&tmp, &self.backup_path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(tmp);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};
    fn path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tronteq-recovery-{}-{name}.bin",
            std::process::id()
        ))
    }
    fn mark_odd(path: &Path, value: u64) {
        let mut f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        f.write_all(&value.to_le_bytes()).unwrap();
    }
    fn clean(path: &Path) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path.with_extension("last-good.json"));
    }
    #[test]
    fn interrupted_startup_never_publishes_fallback_and_explicit_restore_keeps_full_sound() {
        let p = path("restore");
        clean(&p);
        let mut state = StateWriter::open_at(&p).unwrap();
        let mut saved = state.snapshot();
        saved.bands[3].gain = -6.0;
        saved.preamp_db = -4.5;
        saved.bypass = 1;
        saved.delay_ms = 317.5;
        state
            .write_state(
                &saved.bands,
                true,
                saved.preamp_db,
                &saved.dynamics,
                saved.delay_ms,
            )
            .unwrap();
        saved = state.snapshot();
        state.checkpoint();
        assert!(state.backup_error.is_none());
        drop(state);
        mark_odd(&p, saved.version + 1);
        let original = std::fs::read(&p).unwrap();
        let mut reopened = StateWriter::open_at(&p).unwrap();
        assert!(reopened.problem.is_some());
        assert_eq!(reopened.snapshot(), saved);
        assert!(reopened
            .write_state(&saved.bands, false, 9.0, &saved.dynamics, 0.0)
            .is_err());
        reopened.checkpoint();
        assert_eq!(std::fs::read(&p).unwrap(), original);
        let fixed = reopened.recover(RecoveryAction::Restore).unwrap();
        assert_eq!(fixed.bands, saved.bands);
        assert_eq!(fixed.preamp_db, -4.5);
        assert_eq!(fixed.bypass, 1);
        assert_eq!(fixed.delay_ms, 317.5);
        assert!(reopened.problem.is_none());
        assert_eq!(fixed.version & 1, 0);
        drop(reopened);
        clean(&p);
    }
    #[test]
    fn no_backup_requires_explicit_reset_and_corrupt_backup_is_not_offered() {
        let p = path("no-backup");
        clean(&p);
        drop(StateWriter::open_at(&p).unwrap());
        std::fs::write(p.with_extension("last-good.json"), "{corrupt").unwrap();
        mark_odd(&p, 3);
        let mut state = StateWriter::open_at(&p).unwrap();
        assert!(state.recovery_sound().is_none());
        assert!(state.recover(RecoveryAction::Restore).is_err());
        assert!(state.reload().is_err());
        let s = state.recover(RecoveryAction::ResetBypassed).unwrap();
        assert!(s.is_bypassed());
        assert_eq!(s.preamp_db, 0.0);
        assert_eq!(s.delay_ms, 0.0);
        assert!(s.bands.iter().all(|b| b.gain == 0.0));
        drop(state);
        clean(&p);
    }
    #[test]
    fn failed_checkpoint_preserves_existing_backup_and_can_retry() {
        let p = path("backup-failure");
        clean(&p);
        let mut state = StateWriter::open_at(&p).unwrap();
        state.checkpoint();
        let original = std::fs::read(p.with_extension("last-good.json")).unwrap();
        let tmp = state
            .backup_path
            .with_extension(format!("json.{}.tmp", std::process::id()));
        std::fs::create_dir(&tmp).unwrap();
        let s = state.snapshot();
        state
            .write_state(&s.bands, true, -6.0, &s.dynamics, 123.0)
            .unwrap();
        state.checkpoint();
        assert!(state.backup_error.is_some());
        assert_eq!(std::fs::read(&state.backup_path).unwrap(), original);
        std::fs::remove_dir(tmp).unwrap();
        state.retry_at = Instant::now();
        state.checkpoint();
        assert!(state.backup_error.is_none());
        assert_eq!(state.backup.unwrap().snapshot.preamp_db, -6.0);
        drop(state);
        clean(&p);
    }
}
