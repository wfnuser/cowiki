use crate::local_engine::LocalEngine;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpLaunch {
    pub space_slug: String,
    pub metadata_dir: PathBuf,
}

pub fn parse_launch_args<I, S>(arguments: I) -> Result<Option<McpLaunch>, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let arguments = arguments.into_iter().map(Into::into).collect::<Vec<_>>();
    if !arguments.iter().any(|argument| argument == "--mcp") {
        return Ok(None);
    }
    let value_after = |flag: &str| {
        arguments
            .iter()
            .position(|argument| argument == flag)
            .and_then(|index| arguments.get(index + 1))
            .filter(|value| !value.starts_with("--"))
            .cloned()
    };
    let space_slug = value_after("--space")
        .ok_or_else(|| "CoWiki MCP mode requires --space <local-space-slug>".to_string())?;
    let metadata_dir = value_after("--metadata-dir")
        .map(PathBuf::from)
        .unwrap_or_else(default_metadata_dir);
    Ok(Some(McpLaunch {
        space_slug,
        metadata_dir,
    }))
}

pub fn run_from_process_args() -> Result<bool, String> {
    let Some(launch) = parse_launch_args(std::env::args())? else {
        return Ok(false);
    };
    run_stdio(&launch.metadata_dir, &launch.space_slug)?;
    Ok(true)
}

fn default_metadata_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cowiki/.cowiki")
}

pub fn handle_request(engine: &LocalEngine, space_slug: &str, request: Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "cowiki-local-knowledge", "version": env!("CARGO_PKG_VERSION")}
            }
        }),
        "ping" => success(id, json!({})),
        "tools/list" => success(id, json!({"tools": tool_definitions()})),
        "tools/call" => handle_tool_call(engine, space_slug, id, request.get("params")),
        method if method.starts_with("notifications/") => Value::Null,
        _ => rpc_error(id, -32601, format!("method not found: {method}")),
    }
}

fn handle_tool_call(
    engine: &LocalEngine,
    space_slug: &str,
    id: Value,
    params: Option<&Value>,
) -> Value {
    let name = params
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params
        .and_then(|value| value.get("arguments"))
        .cloned()
        .unwrap_or_else(|| json!({}));

    let result: Result<Value, String> = (|| match name {
        "get_space_context" => get_space_context(engine, space_slug),
        "search_pages" => {
            let query = required_string(&arguments, "query")?;
            let limit = arguments
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(12)
                .min(100) as usize;
            engine
                .search_pages(space_slug, query, limit)
                .map(|results| json!({"results": results}))
        }
        "get_page" => {
            let path = required_string(&arguments, "path")?;
            engine
                .get_page_by_path(space_slug, path)
                .and_then(|page| serde_json::to_value(page).map_err(|e| e.to_string()))
        }
        "list_backlinks" => {
            let path = required_string(&arguments, "path")?;
            engine
                .list_backlinks(space_slug, path)
                .map(|results| json!({"results": results}))
        }
        _ => Err(format!("unknown or unavailable read-only tool: {name}")),
    })();

    match result {
        Ok(value) => success(id, tool_result(value, false)),
        Err(error) => success(id, tool_result(json!({"error": error}), true)),
    }
}

fn get_space_context(engine: &LocalEngine, space_slug: &str) -> Result<Value, String> {
    let space = engine.find_space(space_slug)?;
    let page_count = engine.indexed_page_count(space_slug)?;
    let root = space.local_path.clone();
    Ok(json!({
        "space": space,
        "root": root,
        "indexedPageCount": page_count,
        "markdownIsSourceOfTruth": true,
        "guidance": [
            "Search before answering questions about this Space.",
            "Read the relevant pages and cite their relative file paths.",
            "The index is derived; Markdown files are the source of truth."
        ]
    }))
}

fn required_string<'a>(arguments: &'a Value, name: &str) -> Result<&'a str, String> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("'{name}' must be a non-empty string"))
}

fn tool_result(value: Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": value,
        "isError": is_error
    })
}

fn success(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: Value, code: i64, message: String) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "get_space_context",
            "description": "Describe the current CoWiki Space and its local knowledge boundary.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        }),
        json!({
            "name": "search_pages",
            "description": "Search Markdown pages by title and body text. Returns ranked file evidence and snippets.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "minLength": 1},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100, "default": 12}
                },
                "required": ["query"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "get_page",
            "description": "Read one Markdown page by its path relative to the Space root.",
            "inputSchema": {
                "type": "object",
                "properties": {"path": {"type": "string", "minLength": 1}},
                "required": ["path"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "list_backlinks",
            "description": "List pages whose wikilinks point to the requested Markdown page.",
            "inputSchema": {
                "type": "object",
                "properties": {"path": {"type": "string", "minLength": 1}},
                "required": ["path"],
                "additionalProperties": false
            }
        }),
    ]
}

pub fn run_stdio(metadata_dir: &Path, space_slug: &str) -> Result<(), String> {
    let engine = LocalEngine::open(metadata_dir)?;
    engine.find_space(space_slug)?;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle_request(&engine, space_slug, request),
            Err(error) => rpc_error(Value::Null, -32700, format!("parse error: {error}")),
        };
        if response.is_null() {
            continue;
        }
        serde_json::to_writer(&mut stdout, &response).map_err(|e| e.to_string())?;
        stdout.write_all(b"\n").map_err(|e| e.to_string())?;
        stdout.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::local_engine::LocalEngine;
    use serde_json::json;

    fn setup() -> (tempfile::TempDir, LocalEngine, String) {
        let temp = tempfile::tempdir().unwrap();
        let engine = LocalEngine::open(&temp.path().join("metadata")).unwrap();
        let root = temp.path().join("space");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("local-first.md"),
            "# Local-first\n\nThe network is optional. See [[sync]].",
        )
        .unwrap();
        std::fs::write(root.join("sync.md"), "# Sync\n\nReplication notes.").unwrap();
        let space = engine.add_space("Space", "space", &root).unwrap();
        (temp, engine, space.slug)
    }

    #[test]
    fn mcp_lists_only_read_only_knowledge_tools() {
        let (_temp, engine, slug) = setup();
        let response = super::handle_request(
            &engine,
            &slug,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}),
        );
        let names = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "get_space_context",
                "search_pages",
                "get_page",
                "list_backlinks"
            ]
        );
        assert!(!names
            .iter()
            .any(|name| name.contains("put") || name.contains("write")));
    }

    #[test]
    fn mcp_search_returns_file_evidence_for_the_llm() {
        let (_temp, engine, slug) = setup();
        let response = super::handle_request(
            &engine,
            &slug,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"tools/call",
                "params":{"name":"search_pages","arguments":{"query":"network optional","limit":5}}
            }),
        );
        assert_eq!(response["id"], 2);
        assert_eq!(response["result"]["isError"], false);
        let structured = &response["result"]["structuredContent"];
        assert_eq!(structured["results"][0]["path"], "local-first.md");
        assert_eq!(structured["results"][0]["title"], "Local-first");
    }

    #[test]
    fn mcp_reads_pages_and_backlinks_but_rejects_write_tools() {
        let (_temp, engine, slug) = setup();
        let page = super::handle_request(
            &engine,
            &slug,
            json!({
                "jsonrpc":"2.0","id":3,"method":"tools/call",
                "params":{"name":"get_page","arguments":{"path":"sync.md"}}
            }),
        );
        assert_eq!(
            page["result"]["structuredContent"]["body"],
            "# Sync\n\nReplication notes."
        );

        let backlinks = super::handle_request(
            &engine,
            &slug,
            json!({
                "jsonrpc":"2.0","id":4,"method":"tools/call",
                "params":{"name":"list_backlinks","arguments":{"path":"sync.md"}}
            }),
        );
        assert_eq!(
            backlinks["result"]["structuredContent"]["results"][0]["path"],
            "local-first.md"
        );

        let write = super::handle_request(
            &engine,
            &slug,
            json!({
                "jsonrpc":"2.0","id":5,"method":"tools/call",
                "params":{"name":"put_page","arguments":{"path":"x.md","body":"bad"}}
            }),
        );
        assert_eq!(write["result"]["isError"], true);
    }

    #[test]
    fn mcp_mode_requires_a_space_and_accepts_an_explicit_metadata_directory() {
        let parsed = super::parse_launch_args([
            "cowiki",
            "--mcp",
            "--space",
            "research",
            "--metadata-dir",
            "/tmp/cowiki-index",
        ])
        .unwrap()
        .unwrap();
        assert_eq!(parsed.space_slug, "research");
        assert_eq!(
            parsed.metadata_dir,
            std::path::PathBuf::from("/tmp/cowiki-index")
        );

        assert!(super::parse_launch_args(["cowiki"]).unwrap().is_none());
        assert!(super::parse_launch_args(["cowiki", "--mcp"]).is_err());
    }
}
