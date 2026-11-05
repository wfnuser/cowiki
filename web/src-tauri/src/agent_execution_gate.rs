//! One start/close gate for every local Agent runtime.
//!
//! Terminal PTYs and native Agent sessions both reserve an
//! [`ExecutionScope`] for as long as their process may write files. Merging or
//! discarding an Agent Change takes a [`CloseLease`] on the same gate, so an
//! Agent write and a Change merge/discard can never both succeed: whichever
//! side reserves first makes the other fail. The gate's mutex is a leaf lock:
//! callers may hold other locks while calling it, but the gate never calls out.
//! Lock order for Draft mutations stays gate → LocalEngine mutation lock.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex};

/// A Space plus an optional Agent Change. A Live scope (no Change) writes the
/// Draft directly and therefore blocks closing every Change in its Space.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExecutionScope {
    pub space_slug: String,
    pub change_id: Option<String>,
}

impl ExecutionScope {
    pub fn live(space_slug: &str) -> Self {
        Self {
            space_slug: space_slug.to_string(),
            change_id: None,
        }
    }

    pub fn background(space_slug: &str, change_id: &str) -> Self {
        Self {
            space_slug: space_slug.to_string(),
            change_id: Some(change_id.to_string()),
        }
    }

    fn blocks_close(&self, space_slug: &str, change_id: &str) -> bool {
        self.space_slug == space_slug
            && self.change_id.as_deref().is_none_or(|own| own == change_id)
    }
}

#[derive(Default)]
struct GateState {
    next_lease: u64,
    starts: HashMap<u64, ExecutionScope>,
    closing: HashSet<(String, String)>,
}

#[derive(Clone, Default)]
pub struct AgentExecutionGate {
    state: Arc<Mutex<GateState>>,
}

/// Held while an Agent process for the scope may still run. Dropping it
/// releases the reservation.
pub struct StartLease {
    gate: AgentExecutionGate,
    id: u64,
    scope: ExecutionScope,
}

/// Held for the whole merge/discard of one Agent Change.
pub struct CloseLease {
    gate: AgentExecutionGate,
    key: (String, String),
}

impl AgentExecutionGate {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, GateState>, String> {
        self.state
            .lock()
            .map_err(|_| "Agent execution state is unavailable".to_string())
    }

    pub fn reserve_start(&self, scope: ExecutionScope) -> Result<StartLease, String> {
        let mut state = self.lock()?;
        let blocked = match scope.change_id.as_deref() {
            Some(change_id) => state
                .closing
                .contains(&(scope.space_slug.clone(), change_id.to_string())),
            None => state
                .closing
                .iter()
                .any(|(space_slug, _)| *space_slug == scope.space_slug),
        };
        if blocked {
            return Err(if scope.change_id.is_some() {
                "Agent Change is being closed; its Agent cannot start".to_string()
            } else {
                "An Agent Change in this Space is being closed; a Live Agent cannot start"
                    .to_string()
            });
        }
        state.next_lease += 1;
        let id = state.next_lease;
        state.starts.insert(id, scope.clone());
        Ok(StartLease {
            gate: self.clone(),
            id,
            scope,
        })
    }

    pub fn begin_close(&self, space_slug: &str, change_id: &str) -> Result<CloseLease, String> {
        let mut state = self.lock()?;
        if let Some(scope) = state
            .starts
            .values()
            .find(|scope| scope.blocks_close(space_slug, change_id))
        {
            return Err(if scope.change_id.is_none() {
                "Stop the active Live Agent before merging or discarding an Agent Change in this Space."
                    .to_string()
            } else {
                "Stop the active background Agent before merging or discarding this Agent Change."
                    .to_string()
            });
        }
        let key = (space_slug.to_string(), change_id.to_string());
        if !state.closing.insert(key.clone()) {
            return Err("Agent Change is already being closed".to_string());
        }
        Ok(CloseLease {
            gate: self.clone(),
            key,
        })
    }

    #[cfg(test)]
    pub fn active_starts(&self) -> usize {
        self.lock().map(|state| state.starts.len()).unwrap_or(0)
    }
}

impl Drop for StartLease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.gate.state.lock() {
            state.starts.remove(&self.id);
        }
    }
}

impl Drop for CloseLease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.gate.state.lock() {
            state.closing.remove(&self.key);
        }
    }
}

impl fmt::Debug for StartLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StartLease")
            .field("scope", &self.scope)
            .finish()
    }
}

impl fmt::Debug for CloseLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CloseLease")
            .field("change", &self.key)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_session_blocks_change_close() {
        let gate = AgentExecutionGate::default();
        let scope = ExecutionScope::background("space-a", "change-a");
        let lease = gate.reserve_start(scope).unwrap();
        assert!(gate.begin_close("space-a", "change-a").is_err());
        drop(lease);
        assert!(gate.begin_close("space-a", "change-a").is_ok());
    }

    #[test]
    fn live_scope_blocks_every_change_in_its_space_only() {
        let gate = AgentExecutionGate::default();
        let live = gate.reserve_start(ExecutionScope::live("space-a")).unwrap();
        let error = gate.begin_close("space-a", "change-a").unwrap_err();
        assert!(error.contains("Live"));
        assert!(gate.begin_close("space-b", "change-a").is_ok());
        drop(live);
        assert!(gate.begin_close("space-a", "change-a").is_ok());
    }

    #[test]
    fn closing_change_refuses_new_starts_until_released() {
        let gate = AgentExecutionGate::default();
        let closing = gate.begin_close("space-a", "change-a").unwrap();
        assert!(gate
            .reserve_start(ExecutionScope::background("space-a", "change-a"))
            .unwrap_err()
            .contains("being closed"));
        assert!(gate
            .reserve_start(ExecutionScope::live("space-a"))
            .unwrap_err()
            .contains("being closed"));
        assert!(gate
            .reserve_start(ExecutionScope::background("space-a", "change-b"))
            .is_ok());
        assert!(gate.begin_close("space-a", "change-a").is_err());
        drop(closing);
        assert!(gate
            .reserve_start(ExecutionScope::background("space-a", "change-a"))
            .is_ok());
    }

    #[test]
    fn concurrent_start_and_close_never_both_succeed() {
        for _ in 0..200 {
            let gate = AgentExecutionGate::default();
            let starter = {
                let gate = gate.clone();
                std::thread::spawn(move || {
                    gate.reserve_start(ExecutionScope::background("s", "c"))
                        .ok()
                })
            };
            let closer = {
                let gate = gate.clone();
                std::thread::spawn(move || gate.begin_close("s", "c").ok())
            };
            let start = starter.join().unwrap();
            let close = closer.join().unwrap();
            assert!(
                !(start.is_some() && close.is_some()),
                "start and close both succeeded"
            );
        }
    }
}
