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
        "hybrid_search" => {
            let query = required_string(&arguments, "query")?;
            let options = crate::local_engine::HybridSearchOptions {
                token_budget: arguments
                    .get("token_budget")
                    .or_else(|| arguments.get("tokenBudget"))
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
                max_hops: arguments
                    .get("max_hops")
                    .or_else(|| arguments.get("maxHops"))
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
                seed_top_k: arguments
                    .get("seed_top_k")
                    .or_else(|| arguments.get("seedTopK"))
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
                include_backlinks: arguments
                    .get("include_backlinks")
                    .or_else(|| arguments.get("includeBacklinks"))
                    .and_then(Value::as_bool),
                exclude_retired: arguments
                    .get("exclude_retired")
                    .or_else(|| arguments.get("excludeRetired"))
                    .and_then(Value::as_bool),
                follow_redirects: arguments
                    .get("follow_redirects")
                    .or_else(|| arguments.get("followRedirects"))
                    .and_then(Value::as_bool),
                use_okf_governance: arguments
                    .get("use_okf_governance")
                    .or_else(|| arguments.get("useOkfGovernance"))
                    .and_then(Value::as_bool),
                use_lexical: arguments
                    .get("use_lexical")
                    .or_else(|| arguments.get("useLexical"))
                    .and_then(Value::as_bool),
                use_vector: arguments
                    .get("use_vector")
                    .or_else(|| arguments.get("useVector"))
                    .and_then(Value::as_bool),
                use_graph_expansion: arguments
                    .get("use_graph_expansion")
                    .or_else(|| arguments.get("useGraphExpansion"))
                    .and_then(Value::as_bool),
                scope_filter: arguments
                    .get("scope_filter")
                    .or_else(|| arguments.get("scopeFilter"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                as_of_date: arguments
                    .get("as_of_date")
                    .or_else(|| arguments.get("asOfDate"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            };
            engine
                .hybrid_search(space_slug, query, &options)
                .and_then(|bundle| serde_json::to_value(bundle).map_err(|e| e.to_string()))
        }
        "get_page" => {
            let path = required_string(&arguments, "path")?;
            engine.get_page_by_path(space_slug, path).and_then(|page| {
                let okf_metadata = crate::okf::parse_metadata(&page.body);
                let mut value = serde_json::to_value(page).map_err(|e| e.to_string())?;
                if let Some(map) = value.as_object_mut() {
                    map.insert(
                        "okfMetadata".to_string(),
                        serde_json::to_value(okf_metadata).map_err(|e| e.to_string())?,
                    );
                }
                Ok(value)
            })
        }
        "list_backlinks" => {
            let path = required_string(&arguments, "path")?;
            engine
                .list_backlinks(space_slug, path)
                .map(|results| json!({"results": results}))
        }
        "list_broken_links" => engine
            .list_broken_links(space_slug)
            .map(|results| json!({"results": results})),
        "validate_okf" => {
            let as_of = arguments
                .get("as_of")
                .or_else(|| arguments.get("asOf"))
                .and_then(Value::as_str);
            let check_links = arguments
                .get("check_links")
                .or_else(|| arguments.get("checkLinks"))
                .and_then(Value::as_bool)
                .unwrap_or(true);
            engine
                .validate_okf(space_slug, as_of, check_links)
                .and_then(|report| serde_json::to_value(report).map_err(|e| e.to_string()))
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
    let stats = engine.index_stats(space_slug)?;
    let declared_okf = crate::okf::declared_version(&space.local_path)?
        .unwrap_or_else(|| crate::okf::OKF_VERSION.to_string());
    let root = space.local_path.clone();
    Ok(json!({
        "space": space,
        "root": root,
        "okfVersion": declared_okf,
        "supportedOkfVersions": crate::okf::SUPPORTED_OKF_VERSIONS,
        "rootIndex": "index.md",
        "indexedPageCount": stats.indexed_pages,
        "indexStats": stats,
        "markdownIsSourceOfTruth": true,
        "guidance": [
            "Search before answering questions about this Space; use hybrid_search for multi-hop budgeted evidence with line citations.",
            "Start from index.md and follow progressive-disclosure links when exploring.",
            "Read the relevant pages and cite their relative file paths.",
            "Respect OKF v0.3 governance fields (status, stale_after, trust_tier, verified, redirect_to) and demote verified: true to false (trust_tier: agent_draft, generated: true) when an agent edits a human-verified page.",
            "Use standard Markdown links as the portable relationship graph; wikilinks are an extension.",
            "Use validate_okf for read-only conformance, staleness, redirect, and broken-link diagnostics.",
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
    let mut tools = vec![
        json!({
            "name": "get_space_context",
            "description": "Describe the current CoWiki Space and its local knowledge boundary.",
            "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
        }),
        json!({
            "name": "search_pages",
            "description": "Search Markdown pages by title and body text with OKF v0.3 governance filtering and redirect resolution. Returns ranked file evidence and snippets.",
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
            "name": "hybrid_search",
            "description": "Run budgeted section-level BM25 + dense semantic vector RRF retrieval with hub-dampened 1-hop/2-hop link-graph expansion (including #anchor section links), OKF v0.3 governance filtering, redirect resolution, and line-range citations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "minLength": 1},
                    "token_budget": {"type": "integer", "minimum": 64, "maximum": 16000, "default": 1200},
                    "max_hops": {"type": "integer", "minimum": 0, "maximum": 2, "default": 2},
                    "seed_top_k": {"type": "integer", "minimum": 1, "maximum": 20, "default": 6},
                    "include_backlinks": {"type": "boolean", "default": true},
                    "exclude_retired": {"type": "boolean", "default": true},
                    "follow_redirects": {"type": "boolean", "default": true},
                    "use_okf_governance": {"type": "boolean", "default": true},
                    "use_lexical": {"type": "boolean", "default": true},
                    "use_vector": {"type": "boolean", "default": true},
                    "use_graph_expansion": {"type": "boolean", "default": true},
                    "scope_filter": {"type": "string"},
                    "as_of_date": {"type": "string"}
                },
                "required": ["query"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "get_page",
            "description": "Read one Markdown page and its parsed OKF v0.3 metadata by its path relative to the Space root.",
            "inputSchema": {
                "type": "object",
                "properties": {"path": {"type": "string", "minLength": 1}},
                "required": ["path"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "list_backlinks",
            "description": "List pages whose standard Markdown links or wikilinks point to the requested OKF document path.",
            "inputSchema": {
                "type": "object",
                "properties": {"path": {"type": "string", "minLength": 1}},
                "required": ["path"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "list_broken_links",
            "description": "List unresolved internal Markdown links in the current Space. Results are read-only diagnostics derived from refreshed Markdown files.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "validate_okf",
            "description": "Validate the current Space against OKF v0.3 conformance, staleness, trust-reset, redirect, and link diagnostics (read-only).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "as_of": {"type": "string"},
                    "check_links": {"type": "boolean", "default": true}
                },
                "additionalProperties": false
            }
        }),
    ];
    // Every tool only reads the Space, so clients may run them without asking.
    for tool in &mut tools {
        tool["annotations"] = json!({"readOnlyHint": true, "destructiveHint": false});
    }
    tools
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
            "# Local-first\n\nThe network is optional. See [[sync]] and [missing](missing.md).",
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
                "hybrid_search",
                "get_page",
                "list_backlinks",
                "list_broken_links",
                "validate_okf",
            ]
        );
        assert!(!names
            .iter()
            .any(|name| name.contains("put") || name.contains("write")));
    }

    #[test]
    fn mcp_hybrid_search_and_validate_okf_work_end_to_end() {
        let (temp, engine, slug) = setup();
        let root = temp.path().join("space");
        std::fs::write(
            root.join("sync.md"),
            "---\ntype: Note\ntitle: Sync\nstatus: active\nverified: true\n---\n\n# Sync\n\nReplication notes referencing [Deep Policy](policy.md).\n",
        )
        .unwrap();
        std::fs::write(
            root.join("policy.md"),
            "---\ntype: Policy\ntitle: Deep Policy\nstatus: active\n---\n\n# Deep Policy\n\nConflict resolution uses deterministic timestamps.\n",
        )
        .unwrap();
        std::fs::write(
            root.join("stale.md"),
            "---\ntype: Note\ntitle: Stale Draft\ntrust_tier: agent_draft\nverified: true\ngenerated: true\nstale_after: \"2024-01-01\"\n---\n\n# Stale Draft\n\nNeeds re-verification.\n",
        )
        .unwrap();

        let hybrid = super::handle_request(
            &engine,
            &slug,
            json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": {
                    "name": "hybrid_search",
                    "arguments": {
                        "query": "network optional deterministic timestamps",
                        "token_budget": 500,
                        "max_hops": 2
                    }
                }
            }),
        );
        assert_eq!(hybrid["result"]["isError"], false, "{hybrid}");
        let evidence = hybrid["result"]["structuredContent"]["evidence"]
            .as_array()
            .unwrap();
        assert!(!evidence.is_empty());
        assert!(
            hybrid["result"]["structuredContent"]["tokensUsed"]
                .as_u64()
                .unwrap()
                <= 500
        );
        let paths: Vec<&str> = evidence
            .iter()
            .filter_map(|item| item["pagePath"].as_str())
            .collect();
        assert!(paths.contains(&"local-first.md"));
        assert!(paths.contains(&"sync.md"));

        let audit = super::handle_request(
            &engine,
            &slug,
            json!({
                "jsonrpc": "2.0",
                "id": 11,
                "method": "tools/call",
                "params": {
                    "name": "validate_okf",
                    "arguments": {"as_of": "2026-11-22", "check_links": true}
                }
            }),
        );
        assert_eq!(audit["result"]["isError"], false, "{audit}");
        let structured = &audit["result"]["structuredContent"];
        assert_eq!(structured["valid"], false);
        assert_eq!(structured["summaryByCode"]["trust_reset_violation"], 1);
        assert_eq!(structured["summaryByCode"]["expired_stale_after"], 1);
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
        let body = page["result"]["structuredContent"]["body"]
            .as_str()
            .unwrap();
        assert!(body.contains("type: Note"));
        assert!(body.contains("# Sync\n\nReplication notes."));

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
    fn mcp_reports_broken_links_from_refreshed_markdown() {
        let (temp, engine, slug) = setup();
        let response = super::handle_request(
            &engine,
            &slug,
            json!({
                "jsonrpc":"2.0","id":6,"method":"tools/call",
                "params":{"name":"list_broken_links","arguments":{}}
            }),
        );
        assert_eq!(response["result"]["isError"], false);
        let results = response["result"]["structuredContent"]["results"]
            .as_array()
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["source_path"], "local-first.md");
        assert_eq!(results[0]["target"], "missing.md");

        std::fs::write(temp.path().join("space/missing.md"), "# Found\n").unwrap();
        let refreshed = super::handle_request(
            &engine,
            &slug,
            json!({
                "jsonrpc":"2.0","id":7,"method":"tools/call",
                "params":{"name":"list_broken_links","arguments":{}}
            }),
        );
        assert!(refreshed["result"]["structuredContent"]["results"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn mcp_uses_okf_repo_paths_and_can_read_reserved_docs_and_sources() {
        let (temp, engine, slug) = setup();
        let root = temp.path().join("space");
        std::fs::create_dir_all(root.join("research")).unwrap();
        std::fs::create_dir_all(root.join(".cowiki/sources")).unwrap();
        std::fs::write(
            root.join("research/note.md"),
            "# Nested Evidence\n\nCapybara.",
        )
        .unwrap();
        std::fs::write(
            root.join(".cowiki/sources/interview.md"),
            "---\ntype: Source\ntitle: Interview\n---\n\nSource evidence about wombats.",
        )
        .unwrap();
        engine.rebuild_search_index(&slug).unwrap();

        let search = super::handle_request(
            &engine,
            &slug,
            json!({
                "jsonrpc":"2.0","id":8,"method":"tools/call",
                "params":{"name":"search_pages","arguments":{"query":"wombats"}}
            }),
        );
        assert_eq!(
            search["result"]["structuredContent"]["results"][0]["path"],
            ".cowiki/sources/interview.md"
        );

        for path in ["index.md", ".cowiki/sources/interview.md"] {
            let page = super::handle_request(
                &engine,
                &slug,
                json!({
                    "jsonrpc":"2.0","id":9,"method":"tools/call",
                    "params":{"name":"get_page","arguments":{"path":path}}
                }),
            );
            assert_eq!(page["result"]["isError"], false, "{page}");
            assert_eq!(page["result"]["structuredContent"]["path"], path);
        }
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
