# Legacy Cloud MCP prototype

> This crate is a legacy Cloud prototype retained temporarily as implementation
> reference. It is not supported, not the current desktop MCP, and not for a
> local Space.

This process proxies MCP tools to the retired `cowiki-server` REST contract.
Its authenticated HTTP workflow, remote write tools, and review model predate
CoWiki's local-first desktop architecture.

For current Agent integration, use the read-only MCP embedded in the desktop
app and edit Markdown files directly. See [the local MCP
contract](../docs/mcp.md) and the [`cowiki-space`
skill](../skills/cowiki-space/SKILL.md).

Future Cloud MCP support will be designed around shared Spaces and Cloud
Changes. Do not build new local workflows on this crate's API.
