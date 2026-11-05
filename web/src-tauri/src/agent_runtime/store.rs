//! Private, local session records: `<metadata dir>/agent-sessions/<id>.json`.
//!
//! The records hold what the sidebar shows and the identities needed to
//! resume (provider thread id, Space id, canonical directories). They never
//! enter a Space, Git or Cloud, and never contain credentials or the agent's
//! environment. Writes replace the file atomically; unreadable files are set
//! aside, not deleted.

use super::contract::{AgentSnapshot, AgentStatus};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub schema_version: u32,
    pub snapshot: AgentSnapshot,
    pub space_id: String,
    pub space_path: PathBuf,
    pub cwd: PathBuf,
}

/// Work that was running when CoWiki stopped is never replayed.
pub fn restore_status(status: AgentStatus) -> AgentStatus {
    match status {
        AgentStatus::Starting
        | AgentStatus::Running
        | AgentStatus::Waiting
        | AgentStatus::Stopping => AgentStatus::Interrupted,
        other => other,
    }
}

pub fn valid_session_id(id: &str) -> bool {
    id.strip_prefix("agent-")
        .is_some_and(|rest| rest.len() == 32 && rest.chars().all(|c| c.is_ascii_hexdigit()))
}

#[derive(Clone)]
pub struct SessionStore {
    dir: PathBuf,
}

impl SessionStore {
    pub fn new(metadata_dir: &Path) -> Self {
        Self {
            dir: metadata_dir.join("agent-sessions"),
        }
    }

    fn ensure_dir(&self) -> Result<(), String> {
        match fs::symlink_metadata(&self.dir) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(format!("{} must be a plain directory", self.dir.display()));
            }
            Ok(_) => {}
            Err(_) => fs::create_dir_all(&self.dir).map_err(|error| error.to_string())?,
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700));
        }
        Ok(())
    }

    fn path_for(&self, id: &str) -> Result<PathBuf, String> {
        if !valid_session_id(id) {
            return Err("invalid Agent session id".into());
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    pub fn save(&self, record: &SessionRecord) -> Result<(), String> {
        self.ensure_dir()?;
        let path = self.path_for(&record.snapshot.session_id)?;
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err("refusing to replace a symbolic link with an Agent session".into());
        }
        let bytes = serde_json::to_vec(record).map_err(|error| error.to_string())?;
        let temporary = self.dir.join(format!(
            ".{}.{}.tmp",
            record.snapshot.session_id,
            uuid::Uuid::new_v4().simple()
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|error| format!("could not save the Agent session: {error}"))
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let path = self.path_for(id)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }

    /// Loads every record. Running work comes back as Interrupted, pending
    /// approvals are dropped, and unreadable files are renamed aside.
    pub fn load_all(&self) -> Vec<SessionRecord> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut records = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(id) = name.strip_suffix(".json") else {
                continue;
            };
            if !valid_session_id(id) {
                continue;
            }
            if fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_file()) {
                continue;
            }
            let parsed = fs::read(&path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<SessionRecord>(&bytes).ok())
                .filter(|record| {
                    record.schema_version == SCHEMA_VERSION && record.snapshot.session_id == id
                });
            match parsed {
                Some(mut record) => {
                    let restored = restore_status(record.snapshot.status);
                    if restored != record.snapshot.status {
                        record.snapshot.status = restored;
                        record.snapshot.notice = Some(
                            "CoWiki closed while this Agent was working. Nothing was re-run; continue when you are ready."
                                .into(),
                        );
                    }
                    record.snapshot.approvals.clear();
                    records.push(record);
                }
                None => {
                    let aside = path.with_file_name(format!(
                        "{name}.corrupt-{}",
                        super::contract::now_millis()
                    ));
                    let _ = fs::rename(&path, aside);
                }
            }
        }
        records.sort_by_key(|record| record.snapshot.created_at);
        records
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::contract::{AgentApproval, AgentMode, ApprovalKind};
    use crate::terminal::AgentKind;

    fn record(id: &str, status: AgentStatus) -> SessionRecord {
        SessionRecord {
            schema_version: SCHEMA_VERSION,
            snapshot: AgentSnapshot {
                session_id: id.into(),
                provider: AgentKind::Codex,
                title: "Task".into(),
                space_slug: "demo".into(),
                mode: AgentMode::Live,
                change_id: None,
                thread_id: Some("thread-1".into()),
                model: None,
                seq: 4,
                status,
                items: vec![],
                approvals: vec![AgentApproval {
                    id: "a".into(),
                    turn_id: None,
                    kind: ApprovalKind::Command,
                    title: "Run".into(),
                    description: String::new(),
                    command: Some("ls".into()),
                    cwd: None,
                    paths: vec![],
                }],
                error: None,
                notice: None,
                closed: false,
                created_at: 1,
                updated_at: 1,
            },
            space_id: "space".into(),
            space_path: PathBuf::from("/space"),
            cwd: PathBuf::from("/space"),
        }
    }

    #[test]
    fn startup_never_replays_running_work() {
        assert_eq!(
            restore_status(AgentStatus::Running),
            AgentStatus::Interrupted
        );
        assert_eq!(
            restore_status(AgentStatus::Waiting),
            AgentStatus::Interrupted
        );
        assert_eq!(
            restore_status(AgentStatus::Starting),
            AgentStatus::Interrupted
        );
        assert_eq!(
            restore_status(AgentStatus::Stopping),
            AgentStatus::Interrupted
        );
        assert_eq!(restore_status(AgentStatus::Stopped), AgentStatus::Stopped);
        assert_eq!(restore_status(AgentStatus::Idle), AgentStatus::Idle);
    }

    #[test]
    fn round_trip_interrupts_running_records_and_drops_approvals() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path());
        let id = format!("agent-{}", uuid::Uuid::new_v4().simple());
        store.save(&record(&id, AgentStatus::Waiting)).unwrap();
        let loaded = store.load_all();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].snapshot.status, AgentStatus::Interrupted);
        assert!(loaded[0].snapshot.approvals.is_empty());
        assert_eq!(loaded[0].snapshot.thread_id.as_deref(), Some("thread-1"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(
                temp.path()
                    .join("agent-sessions")
                    .join(format!("{id}.json")),
            )
            .unwrap()
            .permissions()
            .mode();
            assert_eq!(mode & 0o077, 0, "session file is readable by others");
        }
    }

    #[test]
    fn corrupt_records_are_set_aside_not_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path());
        let dir = temp.path().join("agent-sessions");
        fs::create_dir_all(&dir).unwrap();
        let id = format!("agent-{}", uuid::Uuid::new_v4().simple());
        fs::write(dir.join(format!("{id}.json")), b"{not json").unwrap();
        assert!(store.load_all().is_empty());
        let names = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            names.iter().any(|name| name.contains(".corrupt-")),
            "{names:?}"
        );
    }

    #[test]
    fn rejects_ids_that_could_escape_the_directory() {
        let temp = tempfile::tempdir().unwrap();
        let store = SessionStore::new(temp.path());
        assert!(store
            .save(&record("../agent-x", AgentStatus::Idle))
            .is_err());
        assert!(!valid_session_id("agent-../../x"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlinked_store_directory() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, temp.path().join("agent-sessions")).unwrap();
        let store = SessionStore::new(temp.path());
        let id = format!("agent-{}", uuid::Uuid::new_v4().simple());
        assert!(store.save(&record(&id, AgentStatus::Idle)).is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
    }
}
