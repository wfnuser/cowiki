//! End-to-end tests of the session registry with the real protocol adapters
//! talking to deterministic fake providers (`web/tests/fixtures/*.mjs`).

use super::contract::{
    AgentEvent, AgentItem, AgentMode, AgentSnapshot, AgentStatus, MessageRole, ToolStatus,
};
use super::provider::{EventSink, OpenError, Opened, ProviderConfig, ProviderLauncher};
use super::registry::{AgentRuntime, CreateSession, EventEmitter};
use super::store::SessionStore;
use crate::agent_execution_gate::{AgentExecutionGate, ExecutionScope};
use crate::local_engine::LocalEngine;
use crate::terminal::AgentKind;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Collector(Mutex<Vec<AgentEvent>>);

impl EventEmitter for Collector {
    fn emit(&self, event: AgentEvent) {
        self.0.lock().unwrap().push(event);
    }
}

struct FakeLauncher {
    node: PathBuf,
    fixtures: PathBuf,
    scenario: Mutex<String>,
    log: PathBuf,
}

impl ProviderLauncher for FakeLauncher {
    fn open(
        &self,
        mut config: ProviderConfig,
        resume: Option<String>,
        sink: EventSink,
    ) -> Result<Opened, OpenError> {
        let fixture = match config.agent {
            AgentKind::Codex => "fake-codex-app-server.mjs",
            _ => "fake-acp-agent.mjs",
        };
        config.launch.program = self.node.clone();
        config.launch.args = vec![
            self.fixtures.join(fixture).to_string_lossy().into_owned(),
            self.scenario.lock().unwrap().clone(),
        ];
        config.launch.env = std::env::vars().collect();
        config
            .launch
            .env
            .push(("FAKE_LOG".into(), self.log.to_string_lossy().into_owned()));
        super::open_with_protocol(config, resume, sink)
    }
}

fn node() -> PathBuf {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .chain([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ])
        .map(|directory| directory.join("node"))
        .find(|candidate| candidate.is_file())
        .expect("these tests need Node.js on PATH")
}

struct Harness {
    _temp: tempfile::TempDir,
    root: PathBuf,
    engine: LocalEngine,
    gate: AgentExecutionGate,
    runtime: AgentRuntime,
    launcher: Arc<FakeLauncher>,
    space: String,
}

impl Harness {
    fn new(scenario: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let engine = LocalEngine::open(&root.join("metadata")).unwrap();
        let notes = root.join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        std::fs::write(notes.join("README.md"), "# Notes\n").unwrap();
        let space = engine.add_space("Notes", "notes", &notes).unwrap();
        let launcher = Arc::new(FakeLauncher {
            node: node(),
            fixtures: Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures"),
            scenario: Mutex::new(scenario.to_string()),
            log: root.join("fake.log"),
        });
        let gate = AgentExecutionGate::default();
        let runtime = Self::runtime_for(&engine, &gate, &root, launcher.clone());
        Self {
            _temp: temp,
            root,
            engine,
            gate,
            runtime,
            launcher,
            space: space.slug,
        }
    }

    fn runtime_for(
        engine: &LocalEngine,
        gate: &AgentExecutionGate,
        root: &Path,
        launcher: Arc<FakeLauncher>,
    ) -> AgentRuntime {
        AgentRuntime::new(
            engine.clone(),
            gate.clone(),
            SessionStore::new(&root.join("metadata")),
            Arc::new(Collector(Mutex::new(Vec::new()))),
            launcher,
            None,
        )
    }

    fn restart(&mut self) {
        self.runtime.shutdown_all();
        self.runtime =
            Self::runtime_for(&self.engine, &self.gate, &self.root, self.launcher.clone());
    }

    fn scenario(&self, scenario: &str) {
        *self.launcher.scenario.lock().unwrap() = scenario.to_string();
    }

    fn create(&self, agent: AgentKind, mode: AgentMode, change_id: Option<&str>) -> String {
        self.runtime
            .create_session(CreateSession {
                space_slug: self.space.clone(),
                mode,
                change_id: change_id.map(ToOwned::to_owned),
                agent,
                title: Some("Test task".into()),
            })
            .unwrap()
            .session_id
    }

    fn wait(&self, id: &str, what: &str, done: impl Fn(&AgentSnapshot) -> bool) -> AgentSnapshot {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let snapshot = self.runtime.snapshot(id).unwrap();
            if done(&snapshot) {
                return snapshot;
            }
            if Instant::now() > deadline {
                panic!("timed out waiting for {what}: {snapshot:#?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn settled(&self, id: &str) -> AgentSnapshot {
        self.wait(id, "the session to settle", |snapshot| {
            !snapshot.status.is_busy()
        })
    }

    fn log(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.launcher.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    fn sent(&self, method: &str) -> Vec<Value> {
        self.log()
            .into_iter()
            .filter(|message| message.get("method").and_then(Value::as_str) == Some(method))
            .collect()
    }

    fn responses(&self) -> Vec<Value> {
        self.log()
            .into_iter()
            .filter(|message| message.get("method").is_none())
            .collect()
    }
}

fn assistant_text(snapshot: &AgentSnapshot) -> String {
    snapshot
        .items
        .iter()
        .filter_map(|item| match item {
            AgentItem::Message {
                role: MessageRole::Assistant,
                text,
                ..
            } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn tool(snapshot: &AgentSnapshot, name: &str) -> Option<(ToolStatus, String)> {
    snapshot.items.iter().find_map(|item| match item {
        AgentItem::Tool {
            name: tool_name,
            status,
            output,
            ..
        } if tool_name == name => Some((*status, output.clone())),
        _ => None,
    })
}

#[test]
fn codex_streams_two_turns_on_one_thread() {
    let harness = Harness::new("normal");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "first").unwrap();
    let first = harness.wait(&id, "first answer", |snapshot| {
        snapshot.status == AgentStatus::Idle && assistant_text(snapshot).contains("(first)")
    });
    assert_eq!(first.thread_id.as_deref(), Some("thread-1"));
    assert_eq!(first.model.as_deref(), Some("gpt-fake"));
    assert_eq!(
        tool(&first, "ls"),
        Some((ToolStatus::Completed, "README.md\n".into()))
    );
    assert!(
        !assistant_text(&first).contains("FOREIGN"),
        "another thread's delta leaked in"
    );

    harness.runtime.send(&id, "second").unwrap();
    let second = harness.wait(&id, "second answer", |snapshot| {
        snapshot.status == AgentStatus::Idle && assistant_text(snapshot).contains("(second)")
    });
    assert_eq!(
        harness.sent("thread/start").len(),
        1,
        "a second turn must reuse the thread"
    );
    assert_eq!(harness.sent("turn/start").len(), 2);
    let start = &harness.sent("thread/start")[0]["params"];
    assert_eq!(start["approvalPolicy"], "on-request");
    assert_eq!(start["sandbox"], "workspace-write");
    let turn = &harness.sent("turn/start")[0]["params"];
    assert_eq!(turn["sandboxPolicy"]["networkAccess"], false);
    assert_eq!(
        turn["sandboxPolicy"]["writableRoots"][0],
        harness.root.join("notes").to_string_lossy().as_ref()
    );
    assert!(
        second
            .items
            .iter()
            .filter(|item| matches!(
                item,
                AgentItem::Message {
                    role: MessageRole::User,
                    ..
                }
            ))
            .count()
            == 2
    );
}

#[test]
fn codex_signed_out_is_reported_without_a_terminal() {
    let harness = Harness::new("signed-out");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "hello").unwrap();
    let failed = harness.settled(&id);
    assert_eq!(failed.status, AgentStatus::Failed);
    {
        let error = failed.error.clone().unwrap_or_default();
        assert!(error.contains("not signed in"), "{error}");
    }
    assert!(harness.sent("thread/start").is_empty());
    assert_eq!(
        harness.gate.active_starts(),
        0,
        "a failed start must release the gate"
    );
}

#[test]
fn codex_local_config_cannot_widen_permissions() {
    for scenario in ["full-access", "never-ask"] {
        let harness = Harness::new(scenario);
        let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
        harness.runtime.send(&id, "hello").unwrap();
        let failed = harness.settled(&id);
        assert_eq!(failed.status, AgentStatus::Failed, "{scenario}");
        assert!(
            harness.sent("turn/start").is_empty(),
            "{scenario} must not run a turn"
        );
    }
}

#[test]
fn codex_approvals_are_answered_once_and_never_for_the_session() {
    let harness = Harness::new("approval");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "clean").unwrap();
    let waiting = harness.wait(&id, "an approval", |snapshot| {
        snapshot.status == AgentStatus::Waiting
    });
    let approval = waiting.approvals[0].clone();
    assert_eq!(approval.command.as_deref(), Some("rm -rf build"));
    assert!(approval.description.contains("clean the build folder"));
    assert!(harness
        .runtime
        .answer_approval(&id, "approval-forged", true)
        .is_err());
    harness
        .runtime
        .answer_approval(&id, &approval.id, true)
        .unwrap();
    assert!(
        harness
            .runtime
            .answer_approval(&id, &approval.id, true)
            .is_err(),
        "duplicate answers are refused"
    );
    let done = harness.wait(&id, "the approved turn", |snapshot| {
        snapshot.status == AgentStatus::Idle
    });
    assert_eq!(
        tool(&done, "rm -rf build").unwrap().0,
        ToolStatus::Completed
    );

    harness.runtime.send(&id, "clean again").unwrap();
    let waiting = harness.wait(&id, "a second approval", |snapshot| {
        snapshot.status == AgentStatus::Waiting
    });
    harness
        .runtime
        .answer_approval(&id, &waiting.approvals[0].id, false)
        .unwrap();
    harness.wait(&id, "the declined turn", |snapshot| {
        snapshot.status == AgentStatus::Idle
    });
    let decisions = harness
        .responses()
        .into_iter()
        .filter_map(|response| {
            response
                .pointer("/result/decision")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    assert_eq!(decisions, vec!["accept", "decline"]);
}

#[test]
fn codex_unsupported_requests_fail_closed() {
    let harness = Harness::new("unknown-request");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "do it").unwrap();
    let done = harness.wait(&id, "the turn", |snapshot| {
        snapshot.status == AgentStatus::Idle
    });
    assert!(done.approvals.is_empty());
    let responses = harness.responses();
    let errors = responses
        .iter()
        .filter(|response| response.get("error").is_some())
        .count();
    assert_eq!(
        errors, 2,
        "permissions and user input are refused: {responses:#?}"
    );
    assert!(
        responses.iter().any(|response| response
            .pointer("/result/decision")
            .and_then(Value::as_str)
            == Some("decline")),
        "a whole-folder grant is declined"
    );
    assert!(!responses
        .iter()
        .any(|response| response.to_string().contains("accept")));
}

#[test]
fn codex_stop_before_the_turn_exists_interrupts_it_when_it_starts() {
    let harness = Harness::new("start-delay");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "long").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.sent("turn/start").is_empty() {
        assert!(Instant::now() < deadline, "turn/start was never sent");
        std::thread::sleep(Duration::from_millis(5));
    }
    harness.runtime.stop(&id).unwrap();
    let stopped = harness.settled(&id);
    assert_eq!(stopped.status, AgentStatus::Stopped);
    assert_eq!(harness.sent("turn/interrupt").len(), 1);
    assert!(
        !stopped.items.iter().any(|item| matches!(
            item,
            AgentItem::Tool {
                status: ToolStatus::Running,
                ..
            }
        )),
        "a stopped turn leaves no step running"
    );
    // The same session (and process) can run and stop again after Stop.
    harness.runtime.send(&id, "again").unwrap();
    harness.wait(&id, "the next turn", |snapshot| {
        assistant_text(snapshot).matches('.').count() > 3
    });
    harness.runtime.stop(&id).unwrap();
    assert_eq!(harness.settled(&id).status, AgentStatus::Stopped);
    assert_eq!(harness.sent("turn/interrupt").len(), 2);
    assert_eq!(harness.sent("thread/start").len(), 1);
}

#[test]
fn codex_stop_while_starting_never_sends_the_task() {
    let harness = Harness::new("normal");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "never").unwrap();
    harness.runtime.stop(&id).unwrap();
    let stopped = harness.settled(&id);
    assert_eq!(stopped.status, AgentStatus::Stopped);
    assert!(harness.sent("turn/start").is_empty());
}

#[test]
fn codex_early_completion_is_not_revived_by_the_late_response() {
    let harness = Harness::new("early-complete");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "quick").unwrap();
    harness.wait(&id, "completion", |snapshot| {
        snapshot.status == AgentStatus::Idle && assistant_text(snapshot).contains("done early")
    });
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        harness.runtime.snapshot(&id).unwrap().status,
        AgentStatus::Idle
    );
}

#[test]
fn codex_crash_fails_the_turn_and_releases_the_change() {
    let harness = Harness::new("crash");
    let change = harness
        .engine
        .create_agent_change(&harness.space, "Codex")
        .unwrap();
    let id = harness.create(AgentKind::Codex, AgentMode::Background, Some(&change.id));
    harness.runtime.send(&id, "explode").unwrap();
    let failed = harness.settled(&id);
    assert_eq!(failed.status, AgentStatus::Failed);
    {
        let error = failed.error.clone().unwrap_or_default();
        assert!(error.contains("boom"), "{error}");
    }
    assert_eq!(harness.gate.active_starts(), 0);
    let preparation = harness
        .runtime
        .prepare_change_close(&harness.space, &change.id)
        .unwrap();
    assert!(harness.gate.begin_close(&harness.space, &change.id).is_ok());
    drop(preparation);
}

#[test]
fn codex_turn_start_timeout_stops_without_resending() {
    let harness = Harness::new("timeout");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "slow").unwrap();
    let failed = harness.settled(&id);
    assert_eq!(failed.status, AgentStatus::Failed);
    {
        let error = failed.error.clone().unwrap_or_default();
        assert!(
            error.contains("instead of sending the task again"),
            "{error}"
        );
    }
    assert_eq!(harness.sent("turn/start").len(), 1);
    assert_eq!(harness.gate.active_starts(), 0);
}

#[test]
fn codex_oversize_lines_are_dropped_and_the_turn_finishes() {
    let harness = Harness::new("oversize");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "big").unwrap();
    let done = harness.wait(&id, "the turn", |snapshot| {
        snapshot.status == AgentStatus::Idle && assistant_text(snapshot).contains("(big)")
    });
    assert!(done.notice.unwrap_or_default().contains("larger than") || done.items.len() >= 3);
}

#[test]
fn running_sessions_block_change_close_and_idle_ones_are_ended_first() {
    let harness = Harness::new("long");
    let change = harness
        .engine
        .create_agent_change(&harness.space, "Codex")
        .unwrap();
    let id = harness.create(AgentKind::Codex, AgentMode::Background, Some(&change.id));
    harness.runtime.send(&id, "work").unwrap();
    harness.wait(&id, "the turn to run", |snapshot| {
        assistant_text(snapshot).contains('.')
    });
    let refused = harness
        .runtime
        .prepare_change_close(&harness.space, &change.id)
        .err()
        .unwrap();
    assert!(refused.contains("Stop the running Agent"));
    assert!(
        harness
            .gate
            .begin_close(&harness.space, &change.id)
            .is_err(),
        "the provider still holds the gate"
    );

    harness.runtime.stop(&id).unwrap();
    assert_eq!(harness.settled(&id).status, AgentStatus::Stopped);
    // Idle, provider still alive: closing ends it and confirms it is gone.
    assert_eq!(harness.gate.active_starts(), 1);
    let preparation = harness
        .runtime
        .prepare_change_close(&harness.space, &change.id)
        .unwrap();
    assert_eq!(harness.gate.active_starts(), 0);
    let lease = harness
        .gate
        .begin_close(&harness.space, &change.id)
        .unwrap();
    assert!(
        harness.runtime.send(&id, "sneak in").is_err(),
        "sends are refused while the Change closes"
    );
    harness
        .engine
        .discard_agent_change(&harness.space, &change.id)
        .unwrap();
    preparation.commit("discarded");
    drop(lease);
    let closed = harness.runtime.snapshot(&id).unwrap();
    assert!(closed.closed);
    assert!(harness.runtime.send(&id, "after").is_err());
    assert!(harness.runtime.resume(&id).is_err());
}

#[test]
fn a_live_session_blocks_closing_changes_in_its_space() {
    let harness = Harness::new("long");
    let change = harness
        .engine
        .create_agent_change(&harness.space, "Codex")
        .unwrap();
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "edit draft").unwrap();
    harness.wait(&id, "the turn to run", |snapshot| {
        assistant_text(snapshot).contains('.')
    });
    assert!(harness
        .runtime
        .prepare_change_close(&harness.space, &change.id)
        .err()
        .unwrap()
        .contains("Live"));
    harness.runtime.stop(&id).unwrap();
    harness.settled(&id);
}

#[test]
fn restart_interrupts_running_work_and_resume_reuses_the_thread() {
    let mut harness = Harness::new("long");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "work").unwrap();
    harness.wait(&id, "the turn to run", |snapshot| {
        assistant_text(snapshot).contains('.')
    });
    harness.restart();
    let restored = harness.runtime.snapshot(&id).unwrap();
    assert_eq!(restored.status, AgentStatus::Interrupted);
    assert!(restored.notice.unwrap().contains("Nothing was re-run"));
    assert_eq!(harness.gate.active_starts(), 0, "exit ended the provider");
    let turns_before = harness.sent("turn/start").len();

    harness.scenario("normal");
    harness.runtime.resume(&id).unwrap();
    let resumed = harness.settled(&id);
    assert_eq!(resumed.status, AgentStatus::Idle);
    assert_eq!(harness.sent("thread/resume").len(), 1);
    assert_eq!(
        harness.sent("thread/resume")[0]["params"]["threadId"],
        "thread-1"
    );
    assert_eq!(
        harness.sent("turn/start").len(),
        turns_before,
        "resuming never re-sends the task"
    );
    harness.runtime.send(&id, "continue").unwrap();
    harness.wait(&id, "the continued turn", |snapshot| {
        snapshot.status == AgentStatus::Idle && assistant_text(snapshot).contains("(continue)")
    });
}

#[test]
fn resume_reports_a_missing_thread_instead_of_starting_a_new_one() {
    let mut harness = Harness::new("normal");
    let id = harness.create(AgentKind::Codex, AgentMode::Live, None);
    harness.runtime.send(&id, "first").unwrap();
    harness.wait(&id, "the turn", |snapshot| {
        snapshot.status == AgentStatus::Idle && snapshot.thread_id.is_some()
    });
    harness.runtime.shutdown_all();
    let store = SessionStore::new(&harness.root.join("metadata"));
    let mut record = store
        .load_all()
        .into_iter()
        .find(|record| record.snapshot.session_id == id)
        .unwrap();
    record.snapshot.thread_id = Some("thread-gone".into());
    store.save(&record).unwrap();
    harness.restart();
    let starts = harness.sent("thread/start").len();
    harness.runtime.resume(&id).unwrap();
    let failed = harness.settled(&id);
    assert_eq!(failed.status, AgentStatus::Failed);
    {
        let error = failed.error.clone().unwrap_or_default();
        assert!(error.contains("did not start a new one"), "{error}");
    }
    assert_eq!(harness.sent("thread/start").len(), starts);
}

#[test]
fn sessions_are_bound_to_their_space_and_change() {
    let harness = Harness::new("normal");
    let other = harness.root.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let other_space = harness.engine.add_space("Other", "other", &other).unwrap();
    let foreign = harness
        .engine
        .create_agent_change(&other_space.slug, "Codex")
        .unwrap();
    let create = |mode, change_id: Option<&str>| {
        harness.runtime.create_session(CreateSession {
            space_slug: harness.space.clone(),
            mode,
            change_id: change_id.map(ToOwned::to_owned),
            agent: AgentKind::Codex,
            title: None,
        })
    };
    assert!(
        create(AgentMode::Background, Some(&foreign.id)).is_err(),
        "another Space's Change"
    );
    assert!(
        create(AgentMode::Live, Some(&foreign.id)).is_err(),
        "Live with a Change"
    );
    assert!(
        create(AgentMode::Background, None).is_err(),
        "background without a Change"
    );
    assert!(
        harness
            .runtime
            .create_session(CreateSession {
                space_slug: harness.space.clone(),
                mode: AgentMode::Live,
                change_id: None,
                agent: AgentKind::Antigravity,
                title: None,
            })
            .is_err(),
        "no native protocol"
    );
    assert!(harness
        .runtime
        .list_sessions(&other_space.slug)
        .unwrap()
        .is_empty());
}

#[test]
fn acp_streams_messages_tools_and_plan() {
    let harness = Harness::new("normal");
    let id = harness.create(AgentKind::Grok, AgentMode::Live, None);
    harness.runtime.send(&id, "hi").unwrap();
    let done = harness.wait(&id, "the ACP turn", |snapshot| {
        snapshot.status == AgentStatus::Idle && assistant_text(snapshot).contains("from ACP")
    });
    assert_eq!(done.thread_id.as_deref(), Some("acp-1"));
    assert_eq!(done.model.as_deref(), Some("fake-model"));
    assert_eq!(
        tool(&done, "Read README.md"),
        Some((ToolStatus::Completed, "# Readme".into()))
    );
    assert!(tool(&done, "Plan").unwrap().1.contains("Read the note"));
    assert_eq!(tool(&done, "Thinking").unwrap().0, ToolStatus::Completed);
    let initialize = &harness.sent("initialize")[0]["params"];
    assert_eq!(initialize["protocolVersion"], 1);
    assert_eq!(initialize["clientCapabilities"]["terminal"], false);
    assert_eq!(harness.log()[0]["jsonrpc"], "2.0");
}

#[test]
fn acp_permissions_choose_one_time_options_only() {
    let harness = Harness::new("permission");
    let id = harness.create(AgentKind::Claude, AgentMode::Live, None);
    harness.runtime.send(&id, "touch").unwrap();
    let waiting = harness.wait(&id, "an approval", |snapshot| {
        snapshot.status == AgentStatus::Waiting
    });
    assert_eq!(
        waiting.approvals[0].command.as_deref(),
        Some("touch notes.md")
    );
    assert_eq!(waiting.approvals[0].paths, vec!["notes.md".to_string()]);
    harness
        .runtime
        .answer_approval(&id, &waiting.approvals[0].id, true)
        .unwrap();
    let done = harness.wait(&id, "the turn", |snapshot| {
        snapshot.status == AgentStatus::Idle
    });
    assert_eq!(
        tool(&done, "touch notes.md").unwrap().0,
        ToolStatus::Completed
    );
    harness.runtime.send(&id, "touch again").unwrap();
    let waiting = harness.wait(&id, "a second approval", |snapshot| {
        snapshot.status == AgentStatus::Waiting
    });
    harness
        .runtime
        .answer_approval(&id, &waiting.approvals[0].id, false)
        .unwrap();
    harness.wait(&id, "the declined turn", |snapshot| {
        snapshot.status == AgentStatus::Idle
    });
    let chosen = harness
        .responses()
        .into_iter()
        .filter_map(|response| {
            response
                .pointer("/result/outcome/optionId")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    assert_eq!(chosen, vec!["once", "reject"], "never allow_always");
}

#[test]
fn acp_stop_cancels_the_prompt() {
    let harness = Harness::new("cancel");
    let id = harness.create(AgentKind::Grok, AgentMode::Live, None);
    harness.runtime.send(&id, "long").unwrap();
    harness.wait(&id, "streaming", |snapshot| {
        assistant_text(snapshot).contains('.')
    });
    harness.runtime.stop(&id).unwrap();
    assert_eq!(harness.settled(&id).status, AgentStatus::Stopped);
    assert_eq!(harness.sent("session/cancel").len(), 1);
}

#[test]
fn acp_signed_out_and_old_protocols_are_reported() {
    let harness = Harness::new("auth");
    let id = harness.create(AgentKind::Grok, AgentMode::Live, None);
    harness.runtime.send(&id, "hi").unwrap();
    let failed = harness.settled(&id);
    {
        let error = failed.error.clone().unwrap_or_default();
        assert!(error.contains("Grok is not signed in"), "{error}");
    }

    harness.scenario("old-protocol");
    harness.runtime.send(&id, "hi").unwrap();
    let failed = harness.settled(&id);
    {
        let error = failed.error.clone().unwrap_or_default();
        assert!(error.contains("ACP version"), "{error}");
    }
    assert!(
        harness.sent("session/new").len() == 1,
        "no session on an unsupported protocol"
    );
}

#[test]
fn acp_unattended_modes_are_switched_back_to_asking() {
    let harness = Harness::new("bypass");
    let id = harness.create(AgentKind::Claude, AgentMode::Live, None);
    harness.runtime.send(&id, "hi").unwrap();
    harness.wait(&id, "the turn", |snapshot| {
        snapshot.status == AgentStatus::Idle
    });
    let set_mode = harness.sent("session/set_mode");
    assert_eq!(set_mode.len(), 1);
    assert_eq!(set_mode[0]["params"]["modeId"], "default");
    assert!(super::acp::is_unattended_mode("bypassPermissions"));
    assert!(super::acp::is_unattended_mode("acceptEdits"));
    assert!(!super::acp::is_unattended_mode("default"));
}

#[test]
fn acp_file_system_and_terminal_requests_are_refused() {
    let harness = Harness::new("fs-request");
    let id = harness.create(AgentKind::Grok, AgentMode::Live, None);
    harness.runtime.send(&id, "read").unwrap();
    harness.wait(&id, "the turn", |snapshot| {
        snapshot.status == AgentStatus::Idle
    });
    assert!(harness
        .responses()
        .iter()
        .any(|response| response.pointer("/error/code").and_then(Value::as_i64) == Some(-32601)));
}

#[test]
fn gate_scope_matches_session_mode() {
    let harness = Harness::new("normal");
    let lease = harness
        .gate
        .reserve_start(ExecutionScope::live(&harness.space))
        .unwrap();
    let change = harness
        .engine
        .create_agent_change(&harness.space, "Codex")
        .unwrap();
    assert!(harness
        .gate
        .begin_close(&harness.space, &change.id)
        .is_err());
    drop(lease);
}

/// `cargo test --lib real_cli_probes -- --ignored --nocapture`: probes the
/// locally installed CLIs (handshake and account status only, no model call).
#[test]
#[ignore]
fn real_cli_probes() {
    for agent in [
        AgentKind::Codex,
        AgentKind::Grok,
        AgentKind::Claude,
        AgentKind::OpenCode,
        AgentKind::Hermes,
        AgentKind::Antigravity,
    ] {
        let readiness = super::auth::probe(agent);
        println!("{agent:?}: {}", serde_json::to_string(&readiness).unwrap());
    }
}

fn wait_long(
    runtime: &AgentRuntime,
    id: &str,
    what: &str,
    done: impl Fn(&AgentSnapshot) -> bool,
) -> AgentSnapshot {
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        let snapshot = runtime.snapshot(id).unwrap();
        if done(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}: {snapshot:#?}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn print_items(label: &str, snapshot: &AgentSnapshot) {
    println!(
        "--- {label}: status={:?} error={:?} notice={:?}",
        snapshot.status, snapshot.error, snapshot.notice
    );
    for item in &snapshot.items {
        match item {
            AgentItem::Message { role, text, .. } => println!(
                "  [{role:?}] {}",
                text.chars().take(300).collect::<String>()
            ),
            AgentItem::Tool {
                name,
                status,
                output,
                ..
            } => println!(
                "  [tool {status:?}] {name} :: {}",
                output
                    .chars()
                    .take(160)
                    .collect::<String>()
                    .replace('\n', " ⏎ ")
            ),
        }
    }
}

/// Real Codex, real model (uses the signed-in account's quota):
/// `cargo test --lib real_codex_round_trip -- --ignored --nocapture`.
/// A temporary Space and Agent Change only; nothing else is touched.
#[test]
#[ignore]
fn real_codex_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let engine = LocalEngine::open(&root.join("metadata")).unwrap();
    let notes = root.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    std::fs::write(
        notes.join("README.md"),
        "# Test Space\n\nA disposable CoWiki test Space.\n",
    )
    .unwrap();
    let space = engine.add_space("Notes", "notes", &notes).unwrap();
    let gate = AgentExecutionGate::default();
    let runtime = AgentRuntime::new(
        engine.clone(),
        gate.clone(),
        SessionStore::new(&root.join("metadata")),
        Arc::new(Collector(Mutex::new(Vec::new()))),
        Arc::new(super::LocalLauncher),
        None,
    );
    let change = engine.create_agent_change(&space.slug, "Codex").unwrap();
    let id = runtime
        .create_session(CreateSession {
            space_slug: space.slug.clone(),
            mode: AgentMode::Background,
            change_id: Some(change.id.clone()),
            agent: AgentKind::Codex,
            title: Some("Real Codex check".into()),
        })
        .unwrap()
        .session_id;

    // 1. Write a file inside the Agent Change only.
    runtime.send(&id, "Create a file named hello.md in the current directory whose only content is the line: hello from codex. Do nothing else and reply with one short sentence.").unwrap();
    let first = wait_long(&runtime, &id, "the write turn", |snapshot| {
        !snapshot.status.is_busy() || snapshot.status == AgentStatus::Waiting
    });
    print_items("write", &first);
    if first.status == AgentStatus::Waiting {
        let approval = &first.approvals[0];
        println!(
            "approval requested: {:?} {:?}",
            approval.title, approval.command
        );
        runtime.answer_approval(&id, &approval.id, true).unwrap();
    }
    let first = wait_long(&runtime, &id, "the write turn to finish", |snapshot| {
        !snapshot.status.is_busy()
    });
    print_items("write done", &first);
    assert_eq!(first.status, AgentStatus::Idle);
    let worktree = engine
        .agent_change_worktree(&space.slug, &change.id)
        .unwrap();
    assert!(
        worktree.join("hello.md").exists(),
        "Codex did not write in the Change worktree"
    );
    assert!(
        !notes.join("hello.md").exists(),
        "the Draft must not change before review"
    );

    // 2. Network is outside the sandbox: Codex must ask, and CoWiki denies.
    runtime.send(&id, "Run exactly this shell command and report its output: curl -sS -m 5 -I https://example.com").unwrap();
    let second = wait_long(&runtime, &id, "the network turn", |snapshot| {
        !snapshot.status.is_busy() || snapshot.status == AgentStatus::Waiting
    });
    print_items("network", &second);
    if second.status == AgentStatus::Waiting {
        let approval = &second.approvals[0];
        println!(
            "network approval: {:?} {:?} {:?}",
            approval.title, approval.command, approval.description
        );
        runtime.answer_approval(&id, &approval.id, false).unwrap();
    }
    let second = wait_long(&runtime, &id, "the network turn to finish", |snapshot| {
        !snapshot.status.is_busy()
    });
    print_items("network done", &second);

    // 3. Stop a long command.
    runtime.send(&id, "Run this shell command and wait for it to finish: for i in $(seq 1 120); do echo $i; sleep 1; done").unwrap();
    let running = wait_long(&runtime, &id, "the long command", |snapshot| {
        snapshot.status == AgentStatus::Waiting
            || snapshot.items.iter().any(|item| {
                matches!(
                    item,
                    AgentItem::Tool {
                        status: ToolStatus::Running,
                        tool_kind: super::contract::ToolKind::Command,
                        ..
                    }
                )
            })
            || !snapshot.status.is_busy()
    });
    print_items("long", &running);
    if running.status == AgentStatus::Waiting {
        runtime
            .answer_approval(&id, &running.approvals[0].id, true)
            .unwrap();
        std::thread::sleep(Duration::from_secs(3));
    }
    let stop_started = Instant::now();
    runtime.stop(&id).unwrap();
    let stopped = wait_long(&runtime, &id, "Stop", |snapshot| !snapshot.status.is_busy());
    println!("stopped in {:?}", stop_started.elapsed());
    print_items("stopped", &stopped);
    assert_eq!(stopped.status, AgentStatus::Stopped);

    // 4. Review: closing the Change ends the idle provider, then merge.
    assert!(runtime
        .prepare_change_close(&space.slug, &change.id)
        .is_ok());
    let preparation = runtime
        .prepare_change_close(&space.slug, &change.id)
        .unwrap();
    let lease = gate.begin_close(&space.slug, &change.id).unwrap();
    let merged = engine.merge_agent_change(&space.slug, &change.id).unwrap();
    preparation.commit("merged into the Draft");
    drop(lease);
    println!("merge status: {}", merged.status);
    assert!(
        notes.join("hello.md").exists(),
        "the reviewed change reached the Draft"
    );
    assert!(runtime.snapshot(&id).unwrap().closed);
    runtime.shutdown_all();
}
