//! The environment an Agent CLI gets when CoWiki starts it directly.
//!
//! A desktop app launched from Finder has a minimal PATH, while the terminal
//! integration ran agents through the user's login shell. Native sessions
//! keep that behaviour without a shell between CoWiki and the CLI: the login
//! shell's environment is captured once and then passed to a fixed argv.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

const MARKER: &str = "__COWIKI_AGENT_ENV__";

static LOGIN_ENVIRONMENT: OnceLock<Vec<(String, String)>> = OnceLock::new();

pub fn agent_environment() -> Vec<(String, String)> {
    LOGIN_ENVIRONMENT
        .get_or_init(|| {
            let mut environment = capture_login_environment()
                .unwrap_or_else(|| std::env::vars().collect::<BTreeMap<_, _>>());
            augment_path(&mut environment);
            // Plain output for everything the agent runs on CoWiki's behalf.
            environment.insert("NO_COLOR".into(), "1".into());
            environment
                .entry("TERM".into())
                .or_insert_with(|| "dumb".into());
            // CoWiki's own variables never leak into the agent.
            environment.retain(|key, _| !key.starts_with("COWIKI_AGENT_"));
            environment.into_iter().collect()
        })
        .clone()
}

fn capture_login_environment() -> Option<BTreeMap<String, String>> {
    #[cfg(windows)]
    {
        None
    }
    #[cfg(not(windows))]
    {
        let shell = crate::terminal::resolve_agent_lookup_shell();
        let script = format!("printf '\\0{MARKER}\\0'; /usr/bin/env -0");
        let output =
            crate::terminal::run_shell_command(&shell, &script, Duration::from_secs(6)).ok()?;
        if !output.status.success() {
            return None;
        }
        parse_environment(&output.stdout)
    }
}

fn parse_environment(stdout: &[u8]) -> Option<BTreeMap<String, String>> {
    let marker = format!("\0{MARKER}\0");
    let text = String::from_utf8_lossy(stdout);
    let start = text.find(&marker)? + marker.len();
    let mut environment = BTreeMap::new();
    for entry in text[start..].split('\0') {
        let Some((key, value)) = entry.split_once('=') else {
            continue;
        };
        if key.is_empty() || key.contains(char::is_whitespace) {
            continue;
        }
        environment.insert(key.to_string(), value.to_string());
    }
    environment.contains_key("PATH").then_some(environment)
}

fn augment_path(environment: &mut BTreeMap<String, String>) {
    let mut directories = environment
        .get("PATH")
        .map(|value| std::env::split_paths(value).collect::<Vec<PathBuf>>())
        .unwrap_or_default();
    for directory in crate::terminal::agent_search_directories() {
        if !directories.contains(&directory) {
            directories.push(directory);
        }
    }
    if let Ok(joined) = std::env::join_paths(directories) {
        environment.insert("PATH".into(), joined.to_string_lossy().into_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_the_environment_after_the_marker() {
        let mut stdout = b"rc file noise PATH=/wrong\n".to_vec();
        stdout.extend_from_slice(format!("\0{MARKER}\0").as_bytes());
        stdout.extend_from_slice(b"PATH=/usr/bin:/bin\0MULTI=line one\nline two\0bad entry\0");
        let environment = parse_environment(&stdout).unwrap();
        assert_eq!(environment["PATH"], "/usr/bin:/bin");
        assert_eq!(environment["MULTI"], "line one\nline two");
        assert!(!environment.contains_key("bad entry"));
    }

    #[test]
    fn missing_marker_falls_back() {
        assert!(parse_environment(b"PATH=/usr/bin\0").is_none());
    }
}
