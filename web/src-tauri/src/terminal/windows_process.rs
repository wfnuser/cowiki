//! Windows launchers use literal argv. Standard npm shims are resolved to Node
//! instead of passing prompts/MCP JSON through cmd.exe's expansion and 8 KiB limit.
use std::ffi::OsString;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub(super) struct Invocation {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
}

pub(super) fn find_executable(
    name: &str,
    directories: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    directories
        .into_iter()
        .filter(|path| path.is_absolute())
        .flat_map(|directory| {
            ["exe", "com", "cmd"].map(|ext| directory.join(format!("{name}.{ext}")))
        })
        .find(|path| path.is_file())
}

pub(super) fn resolve(
    executable: &Path,
    directories: impl IntoIterator<Item = PathBuf>,
) -> Result<Invocation, String> {
    if !executable.is_absolute() || !executable.is_file() {
        return Err("Agent launcher must be an existing absolute path".to_string());
    }
    let extension = executable
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "exe" | "com" => Ok(Invocation { program: executable.to_path_buf(), arguments: Vec::new() }),
        "cmd" => {
            let entry = npm_entrypoint(executable)?;
            let parent = executable.parent().ok_or("Agent launcher has no parent")?;
            let node = std::iter::once(parent.to_path_buf()).chain(directories)
                .filter(|path| path.is_absolute()).map(|path| path.join("node.exe")).find(|path| path.is_file())
                .ok_or("This npm Agent needs node.exe on PATH. Install Node.js and restart CoWiki.")?;
            Ok(Invocation { program: node, arguments: vec![entry.into_os_string()] })
        }
        _ => Err("Unsupported Windows Agent launcher. Install a native .exe or a standard npm global package; WSL and PowerShell wrappers cannot run in this terminal.".to_string()),
    }
}

fn npm_entrypoint(shim: &Path) -> Result<PathBuf, String> {
    let unsupported = || {
        format!("{} is not a supported npm launcher. Reinstall the Agent using its native Windows or npm installer.", shim.display())
    };
    let mut text = String::new();
    std::fs::File::open(shim)
        .map_err(|error| error.to_string())?
        .take(65537)
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    if text.len() > 65536 {
        return Err(unsupported());
    }
    let mut entries = Vec::new();
    // npm/cmd-shim emits a quoted %dp0%\node_modules\... entry followed by %*.
    // Recognize only that form; never evaluate arbitrary batch instructions.
    for (index, token) in text.split('"').enumerate() {
        if index % 2 == 0 {
            continue;
        }
        let normalized = token.replace('\\', "/");
        let Some(relative) = normalized.strip_prefix("%dp0%/").or_else(|| {
            normalized
                .strip_prefix("%~dp0")
                .map(|path| path.trim_start_matches('/'))
        }) else {
            continue;
        };
        let path = Path::new(relative);
        if !relative.starts_with("node_modules/") {
            continue;
        }
        if !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
            || relative.contains(':')
            || relative.contains('%')
            || !matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("js" | "cjs" | "mjs") | None
            )
        {
            return Err(unsupported());
        }
        if !entries.contains(&path.to_path_buf()) {
            entries.push(path.to_path_buf());
        }
    }
    if entries.len() != 1 {
        return Err(unsupported());
    }
    let parent = shim.parent().ok_or_else(unsupported)?;
    let modules = parent
        .join("node_modules")
        .canonicalize()
        .map_err(|_| unsupported())?;
    let entry = parent
        .join(&entries[0])
        .canonicalize()
        .map_err(|_| unsupported())?;
    if !entry.starts_with(modules) || !entry.is_file() {
        return Err(unsupported());
    }
    if entry.extension().is_none() {
        let mut prefix = [0; 128];
        let count = std::fs::File::open(&entry)
            .map_err(|_| unsupported())?
            .read(&mut prefix)
            .map_err(|_| unsupported())?;
        let line = String::from_utf8_lossy(&prefix[..count]);
        let shebang = line.lines().next().unwrap_or_default();
        if shebang != "#!/usr/bin/env node" && shebang != "#!/usr/bin/node" {
            return Err(unsupported());
        }
    }
    // Node's module resolver mishandles verbatim paths on Windows. Keep the
    // canonical containment check above, then use a normal path when equivalent.
    Ok(dunce::simplified(&entry).to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npm_fixture(root: &Path) -> PathBuf {
        let entry = root.join("node_modules/@example/agent/bin/cli.js");
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(
            &entry,
            "process.stdout.write(JSON.stringify(process.argv.slice(2)))",
        )
        .unwrap();
        let shim = root.join("codex.cmd");
        std::fs::write(
            &shim,
            "@ECHO off\r\n\"%_prog%\" \"%dp0%\\node_modules\\@example\\agent\\bin\\cli.js\" %*\r\n",
        )
        .unwrap();
        shim
    }

    #[test]
    fn resolves_native_and_npm_launchers_without_a_shell() {
        let temp = tempfile::tempdir().unwrap();
        let shim = npm_fixture(temp.path());
        std::fs::write(temp.path().join("node.exe"), "fixture").unwrap();
        assert_eq!(
            find_executable("codex", [PathBuf::from("."), temp.path().to_path_buf()]),
            Some(shim.clone())
        );
        let invocation = resolve(&shim, []).unwrap();
        assert_eq!(invocation.program, temp.path().join("node.exe"));
        assert_eq!(invocation.arguments.len(), 1);
        let native = temp.path().join("codex.exe");
        std::fs::write(&native, "fixture").unwrap();
        assert_eq!(
            find_executable("codex", [temp.path().to_path_buf()]),
            Some(native.clone())
        );
        assert!(resolve(&native, []).unwrap().arguments.is_empty());
    }

    #[test]
    fn extensionless_node_entrypoints_require_a_node_shebang() {
        let temp = tempfile::tempdir().unwrap();
        let shim = npm_fixture(temp.path());
        let entry = temp.path().join("node_modules/@example/agent/bin/cli");
        std::fs::write(&shim, "\"%~dp0node_modules\\@example\\agent\\bin\\cli\" %*").unwrap();
        std::fs::write(&entry, "#!/usr/bin/env node\nconsole.log('agent');\n").unwrap();
        assert_eq!(
            npm_entrypoint(&shim).unwrap(),
            dunce::canonicalize(&entry).unwrap()
        );
        std::fs::write(&entry, "#!/bin/sh\necho agent\n").unwrap();
        assert!(npm_entrypoint(&shim).is_err());
    }

    #[test]
    fn rejects_missing_node_unrecognized_and_escaping_shims() {
        let temp = tempfile::tempdir().unwrap();
        let shim = npm_fixture(temp.path());
        assert!(resolve(&shim, []).unwrap_err().contains("node.exe"));
        for text in [
            "powershell evil.ps1",
            "\"%dp0%\\node_modules\\..\\outside.js\" %*",
            "\"%dp0%\\node_modules\\missing.js\" %*",
        ] {
            std::fs::write(&shim, text).unwrap();
            assert!(npm_entrypoint(&shim).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn native_node_receives_literal_unicode_and_mcp_arguments() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("知识库 & spaces (test)");
        let shim = npm_fixture(&root);
        let invocation = resolve(&shim, super::super::agent_search_directories()).unwrap();
        let task = "检索 \"quotes\" %PATH% & echo unsafe | $(whoami)\nnext line";
        let launch = super::super::build_agent_command(
            super::super::AgentKind::Claude,
            super::super::TerminalMode::Live,
            super::super::TerminalIntent::Run,
            "notes",
            &shim,
            &root.join("CoWiki.exe"),
            Some(task),
        );
        let output = std::process::Command::new(invocation.program)
            .args(invocation.arguments)
            .args(&launch.arguments)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(actual, launch.arguments);
        assert!(actual.last().unwrap().ends_with(task));
        let config: serde_json::Value = serde_json::from_str(&actual[1]).unwrap();
        assert_eq!(
            config["mcpServers"]["cowiki"]["command"],
            root.join("CoWiki.exe").to_string_lossy().as_ref()
        );
    }
    #[cfg(windows)]
    #[test]
    #[ignore = "requires native Windows ConPTY; run explicitly in Windows CI"]
    fn native_conpty_supports_input_output_resize_and_exit() {
        use portable_pty::{native_pty_system, CommandBuilder, PtySize};
        use std::io::{Read, Write};
        use std::time::{Duration, Instant};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("终端 & spaces");
        std::fs::create_dir_all(&root).unwrap();
        let node = find_executable("node", super::super::agent_search_directories()).unwrap();
        let pair = native_pty_system().openpty(PtySize::default()).unwrap();
        pair.master
            .resize(PtySize {
                rows: 30,
                cols: 100,
                ..PtySize::default()
            })
            .unwrap();
        let mut command = CommandBuilder::new(node);
        command.cwd(&root);
        let argument = "知识库 \"quotes\" %PATH% & $(whoami)\nnext line";
        command.args(["-e", "console.log('COWIKI_ARG:' + JSON.stringify(process.argv[1])); process.stdin.once('data', d => { console.log('COWIKI_PTY_OK:' + d.toString().trim()); process.exit(0); })", argument]);
        let mut child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = [0; 4096];
            while let Ok(count) = reader.read(&mut bytes) {
                if count == 0 || send.send(bytes[..count].to_vec()).is_err() {
                    break;
                }
            }
        });
        writer.write_all(b"hello\r").unwrap();
        writer.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut output = Vec::new();
        while Instant::now() < deadline {
            match receive.recv_timeout(Duration::from_millis(100)) {
                Ok(bytes) => output.extend(bytes),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break,
            }
            if String::from_utf8_lossy(&output).contains("COWIKI_PTY_OK:hello") {
                break;
            }
        }
        let observed = String::from_utf8_lossy(&output).contains("COWIKI_PTY_OK:hello");
        let _ = child.kill();
        child.wait().unwrap();
        assert!(String::from_utf8_lossy(&output).contains(&format!(
            "COWIKI_ARG:{}",
            serde_json::to_string(argument).unwrap()
        )));
        assert!(
            observed,
            "ConPTY output: {}",
            String::from_utf8_lossy(&output)
        );
    }
}
