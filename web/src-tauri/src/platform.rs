use std::path::PathBuf;

pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let candidates = [std::env::var_os("USERPROFILE"), std::env::var_os("HOME")];
    #[cfg(not(windows))]
    let candidates = [std::env::var_os("HOME")];
    candidates
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .find(|path| path.is_absolute())
}

// Desktop and its MCP subprocess must open the same index even when HOME is
// absent (normal on Windows). Never silently create an index in the working tree.
pub fn metadata_dir() -> Result<PathBuf, String> {
    resolve_metadata_dir(home_dir(), || {
        #[cfg(not(windows))]
        {
            // Match Tauri PathResolver::app_data_dir without creating a window,
            // so the MCP subprocess also finds older HOME-less installations.
            let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
            dirs::data_dir().map(|base| base.join(&context.config().identifier))
        }
        #[cfg(windows)]
        {
            None
        }
    })
}

fn resolve_metadata_dir(
    home: Option<PathBuf>,
    app_data: impl FnOnce() -> Option<PathBuf>,
) -> Result<PathBuf, String> {
    home.filter(|path| path.is_absolute())
        .or_else(app_data)
        .filter(|path| path.is_absolute())
        .map(|base| base.join("cowiki").join(".cowiki"))
        .ok_or_else(|| {
            "Cannot locate a home or platform application-data directory for CoWiki".to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_keeps_home_layout_and_legacy_app_data_fallback() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let app_data = root.path().join("app.cowiki.desktop");
        assert_eq!(
            resolve_metadata_dir(Some(home.clone()), || panic!("home must win")).unwrap(),
            home.join("cowiki/.cowiki")
        );
        assert_eq!(
            resolve_metadata_dir(None, || Some(app_data.clone())).unwrap(),
            app_data.join("cowiki/.cowiki")
        );
        assert!(resolve_metadata_dir(None, || None).is_err());
        assert!(
            resolve_metadata_dir(Some(PathBuf::from("relative")), || Some(PathBuf::from(
                "relative"
            )))
            .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn missing_home_keeps_desktop_and_mcp_on_the_same_registered_space() {
        const CHILD_ROOT: &str = "COWIKI_TEST_HOMELESS_ROOT";
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let root = PathBuf::from(root);
            assert!(std::env::var_os("HOME").is_none());
            let metadata = metadata_dir().unwrap();
            assert_eq!(metadata, root.join("app.cowiki.desktop/cowiki/.cowiki"));
            let engine = crate::local_engine::LocalEngine::open(&metadata).unwrap();
            let folder = root.join("Space");
            std::fs::create_dir(&folder).unwrap();
            engine.add_space("Fallback", "fallback", &folder).unwrap();
            drop(engine);
            let launch = crate::mcp::parse_launch_args(["cowiki", "--mcp", "--space", "fallback"])
                .unwrap()
                .unwrap();
            assert_eq!(launch.metadata_dir, metadata);
            assert!(crate::local_engine::LocalEngine::open(&launch.metadata_dir)
                .unwrap()
                .find_space("fallback")
                .is_ok());
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "platform::tests::missing_home_keeps_desktop_and_mcp_on_the_same_registered_space",
                "--nocapture",
            ])
            .env_remove("HOME")
            .env("XDG_DATA_HOME", root.path())
            .env(CHILD_ROOT, root.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
