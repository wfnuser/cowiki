mod local_engine;

use local_engine::{LocalEngine, PageFull, PageMeta, SourceContent, SourceItem, Space, SubmitResult};
use std::path::PathBuf;
use tauri::{Manager, State};

#[tauri::command]
fn choose_local_space_directory() -> Option<String> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
fn local_list_spaces(engine: State<'_, LocalEngine>) -> Result<Vec<Space>, String> {
    engine.list_spaces()
}

#[tauri::command]
fn local_add_space(
    engine: State<'_, LocalEngine>,
    name: String,
    slug: String,
    local_path: String,
) -> Result<Space, String> {
    engine.add_space(&name, &slug, &PathBuf::from(local_path))
}

#[tauri::command]
fn local_list_pages(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    dir: String,
) -> Result<Vec<PageMeta>, String> {
    engine.list_pages(&space_slug, &dir)
}

#[tauri::command]
fn local_get_page(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    dir: String,
    page_slug: String,
) -> Result<PageFull, String> {
    engine.get_page(&space_slug, &dir, &page_slug)
}

#[tauri::command]
fn local_write_page(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    dir: String,
    page_slug: String,
    content: String,
) -> Result<(), String> {
    engine.write_page(&space_slug, &dir, &page_slug, &content)
}

#[tauri::command]
fn local_list_sources(
    engine: State<'_, LocalEngine>,
    space_slug: String,
) -> Result<Vec<SourceItem>, String> {
    engine.list_sources(&space_slug)
}

#[tauri::command]
fn local_get_source(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    filename: String,
) -> Result<SourceContent, String> {
    engine.get_source(&space_slug, &filename)
}

#[tauri::command]
fn local_submit(
    engine: State<'_, LocalEngine>,
    space_slug: String,
    _paths: Vec<String>,
) -> Result<SubmitResult, String> {
    engine.submit(&space_slug)
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            choose_local_space_directory,
            local_list_spaces,
            local_add_space,
            local_list_pages,
            local_get_page,
            local_write_page,
            local_list_sources,
            local_get_source,
            local_submit,
        ])
        .setup(|app| {
            // Keep the small, rebuildable index beside the previous local
            // metadata so repositories opened by older CoWiki builds can be
            // recovered automatically on upgrade.
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| app.path().app_data_dir().unwrap_or_default());
            let cowiki_home = home.join("cowiki");
            let engine = LocalEngine::open(&cowiki_home.join(".cowiki"))
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            engine
                .import_legacy_spaces(&cowiki_home)
                .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            app.manage(engine);

            let window = tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                .title("CoWiki")
                .inner_size(1280.0, 860.0)
                .min_inner_size(980.0, 680.0)
                .title_bar_style(tauri::TitleBarStyle::Overlay)
                .hidden_title(true)
                .build()?;
            window.show()?;
            window.set_focus()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running CoWiki desktop client");
}
