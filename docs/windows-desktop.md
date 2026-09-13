# Windows desktop alpha

The Windows target is **Windows 10 1809+ / Windows 11, x64**, with Microsoft
Edge WebView2. CoWiki's local Spaces, Markdown editing, search, source import,
Git reviews/checkpoints, and managed Agent changes run in the desktop process.
Local work does not require a CoWiki account or server. Agent providers have
separate installation, authentication, and network requirements. Cloud Git sync
also requires [Git for Windows](https://gitforwindows.org/) on PATH; offline
local Spaces use the embedded Git library.

## Install

Download `CoWiki_*_x64-setup.exe` from a desktop release that includes a Windows
asset. Windows CI/release automation still needs to be enabled upstream; until
then maintainers can build an unsigned installer using the commands below.
An older release containing only a DMG does not include Windows support.

The NSIS installer installs for the current user and supports English and
Simplified Chinese. Builds are unsigned; check that the download came from the
project before approving a Windows publisher/SmartScreen prompt. Code signing
is not configured yet.

WebView2 is usually present on Windows 10/11. If missing, setup downloads its
Microsoft bootstrapper, which requires a network connection. For a machine that
must remain offline, provision WebView2 first using Microsoft's offline runtime
installer. Once installed, CoWiki's local engine works offline.

Windows ARM64 and Windows 7/8 are outside this target. WSL is a separate Linux
environment; this installer and terminal do not launch Agents installed only
inside WSL.

## Agents and local data

Install a CLI whose vendor supports native Windows, then restart CoWiki so it
inherits the updated PATH. CoWiki discovers `.exe`/`.com` launchers and standard
npm global `.cmd` shims, including `%APPDATA%\npm`. npm Agents also need
`node.exe` on PATH (or next to the shim). Native installers do not require Node
unless the Agent itself does.

For npm launchers, CoWiki resolves the JavaScript entry point under that npm
prefix's `node_modules` and starts Node directly. Prompts, Chinese paths, spaces,
quotes, and MCP JSON are passed as literal arguments. Arbitrary batch files,
PowerShell wrappers, aliases, and WSL commands are not supported launchers.
Their readiness error explains how to install a supported CLI. Merely listing
an Agent in the UI does not imply that its vendor supports Windows.

The embedded terminal uses Windows ConPTY. Codex login runs in the same native
terminal; authentication remains owned by the CLI. CoWiki does not read Agent
credentials. Live sessions start in the Draft folder, and Background sessions
start in a managed Git worktree. Background snapshots and migration rollback
preserve the original LF/CRLF file bytes without changing your Git settings.

Desktop and MCP both store derived metadata in `%USERPROFILE%\cowiki\.cowiki`.
`HOME` is a fallback only when `USERPROFILE` is missing or not absolute.
On macOS/Linux, an unavailable `HOME` retains the previous Tauri application-data
fallback (`<platform data directory>/app.cowiki.desktop/cowiki/.cowiki`). Desktop
and MCP resolve it identically, so previously registered Spaces remain visible.

Spaces remain ordinary folders at the locations you select. Do not copy a live SQLite
index between Windows and WSL; open the Markdown Space on the intended platform.

## Build from source

Install [Tauri's Windows prerequisites](https://v2.tauri.app/start/prerequisites/):
Visual Studio Build Tools with **Desktop development with C++**, a Windows SDK,
WebView2, [Git for Windows](https://gitforwindows.org/), Node.js 24+, and Rust
stable using the `x86_64-pc-windows-msvc` toolchain.
Run these commands in PowerShell from a normal user account:

```powershell
git clone https://github.com/wfnuser/cowiki.git
cd cowiki
git checkout dev
cd web
npm ci
npm run desktop:dev
```

Build an installer with:

```powershell
npm run desktop:build -- --target x86_64-pc-windows-msvc --bundles nsis
```

Output: `web/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/`.
The platform-specific `tauri.windows.conf.json` is merged automatically.
See [Tauri's installer guide](https://v2.tauri.app/distribute/windows-installer/)
for signing and enterprise deployment options.

## Validation and release checks

Windows CI integration is pending. A Windows runner should run frontend
tests/lint, the Rust engine suite (including literal Node argument round trips),
Clippy, the explicit ConPTY smoke test below, and the NSIS build. Retain the
installer artifact and attach it to desktop releases after the native checks
pass. Cross-platform checks alone are not sufficient to promote this alpha.

Run the terminal smoke explicitly on native Windows (Node.js required):

```powershell
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml native_conpty_supports_input_output_resize_and_exit -- --ignored --nocapture
```

Before promoting an alpha build to an end-user release, record the Windows
version and installer SHA, then check:

- Install, launch from Start, restart, upgrade, and uninstall. Confirm Spaces
  remain intact and the same registered Spaces reappear after restart.
- Create/import a Space under a path containing Chinese characters and spaces.
  Edit, auto-save, rename/delete, search, import supported sources, and open the
  working diff and checkpoints while offline.
- Run a native or npm Agent, complete its login, resize the terminal, and close
  it. Verify MCP can find the same Space. Review, merge, and discard a Background
  Change, including a conflict with an edit made in another editor.
- Check file/folder dialogs and WebView2 rendering, including high-DPI displays.

Cross-compilation and Wine engine tests are useful additional checks; they do
not establish native WebView2, ConPTY, installer, or real-provider behavior.
