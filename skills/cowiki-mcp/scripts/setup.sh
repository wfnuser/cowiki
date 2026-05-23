#!/usr/bin/env bash
# cowiki MCP Setup Script (Linux/macOS)
# Configures MCP client (VS Code or Claude Code) with cowiki server settings.
#
# Usage:
#   bash setup.sh --vscode --api-key cw_xxx [--url http://localhost:8080/]
#   bash setup.sh --claude-code --api-key cw_xxx [--url http://localhost:8080/]

set -euo pipefail

# --- Defaults ---
SERVER_URL="http://localhost:8080/"
API_KEY=""
MODE=""

# --- Parse arguments ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --vscode)
            MODE="vscode"
            shift
            ;;
        --claude-code)
            MODE="claude-code"
            shift
            ;;
        --api-key)
            API_KEY="$2"
            shift 2
            ;;
        --url)
            SERVER_URL="$2"
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 --vscode|--claude-code --api-key <key> [--url <url>]"
            echo ""
            echo "Options:"
            echo "  --vscode        Configure VS Code (.vscode/mcp.json)"
            echo "  --claude-code   Configure Claude Code (~/.claude.json)"
            echo "  --api-key KEY   cowiki API key (required, format: cw_*)"
            echo "  --url URL       MCP server URL (default: http://localhost:8080/)"
            echo "  --help, -h      Show this help"
            exit 0
            ;;
        *)
            echo "ERROR: Unknown argument: $1"
            echo "Run with --help for usage."
            exit 1
            ;;
    esac
done

# --- Validate ---
if [[ -z "$MODE" ]]; then
    echo "ERROR: Must specify --vscode or --claude-code"
    exit 1
fi

if [[ -z "$API_KEY" ]]; then
    echo "ERROR: --api-key is required"
    exit 1
fi

if [[ ! "$API_KEY" =~ ^cw_ ]]; then
    echo "WARNING: API key doesn't start with 'cw_'. Are you sure it's correct?"
fi

# --- Ensure URL ends with / ---
[[ "$SERVER_URL" != */ ]] && SERVER_URL="${SERVER_URL}/"

# --- VS Code Mode ---
configure_vscode() {
    local config_file=".vscode/mcp.json"
    local config_dir=".vscode"

    echo "→ Configuring VS Code MCP: $config_file"

    # Create .vscode directory if needed
    mkdir -p "$config_dir"

    local new_entry
    new_entry=$(cat <<EOF
{
  "servers": {
    "cowiki-mcp": {
      "url": "${SERVER_URL}",
      "type": "http",
      "headers": {
        "Authorization": "Bearer ${API_KEY}"
      }
    }
  }
}
EOF
)

    if [[ -f "$config_file" ]]; then
        echo "   Existing $config_file found. Merging..."

        # Check if cowiki-mcp already exists in config
        if command -v jq &>/dev/null; then
            if jq -e '.servers["cowiki-mcp"]' "$config_file" &>/dev/null; then
                # Update existing entry
                jq --arg url "$SERVER_URL" --arg key "$API_KEY" \
                    '.servers["cowiki-mcp"].url = $url |
                     .servers["cowiki-mcp"].headers.Authorization = ("Bearer " + $key)' \
                    "$config_file" > "${config_file}.tmp"
                mv "${config_file}.tmp" "$config_file"
                echo "   ✓ Updated existing cowiki-mcp entry"
            else
                # Merge: add cowiki-mcp to existing servers
                jq --arg url "$SERVER_URL" --arg key "$API_KEY" \
                    '.servers["cowiki-mcp"] = {
                        "url": $url,
                        "type": "http",
                        "headers": {"Authorization": ("Bearer " + $key)}
                    }' \
                    "$config_file" > "${config_file}.tmp"
                mv "${config_file}.tmp" "$config_file"
                echo "   ✓ Added cowiki-mcp to existing config"
            fi
        else
            # No jq available — create backup and write new
            echo "   jq not found. Creating backup: ${config_file}.bak"
            cp "$config_file" "${config_file}.bak"
            echo "$new_entry" > "$config_file"
            echo "   ⚠ Replaced entire config (backup saved). Install jq for smart merging."
        fi
    else
        echo "$new_entry" > "$config_file"
        echo "   ✓ Created new $config_file"
    fi

    # Set restrictive permissions
    chmod 600 "$config_file" 2>/dev/null || true

    echo ""
    echo "✓ VS Code MCP configured!"
    echo "  Restart VS Code or reload the window for changes to take effect."
}

# --- Claude Code Mode ---
configure_claude_code() {
    local config_file=""

    # Claude Code uses ~/.claude.json or ~/.claude/mcp.json
    if [[ -f "$HOME/.claude.json" ]]; then
        config_file="$HOME/.claude.json"
    elif [[ -f "$HOME/.claude/mcp.json" ]]; then
        config_file="$HOME/.claude/mcp.json"
    else
        config_file="$HOME/.claude.json"
        echo "→ No existing Claude config found. Creating: $config_file"
    fi

    echo "→ Configuring Claude Code MCP: $config_file"

    local new_entry
    new_entry=$(cat <<EOF
{
  "mcpServers": {
    "cowiki-mcp": {
      "type": "http",
      "url": "${SERVER_URL}",
      "headers": {
        "Authorization": "Bearer ${API_KEY}"
      }
    }
  }
}
EOF
)

    if [[ -f "$config_file" ]]; then
        if command -v jq &>/dev/null; then
            if jq -e '.mcpServers["cowiki-mcp"]' "$config_file" &>/dev/null; then
                jq --arg url "$SERVER_URL" --arg key "$API_KEY" \
                    '.mcpServers["cowiki-mcp"].url = $url |
                     .mcpServers["cowiki-mcp"].headers.Authorization = ("Bearer " + $key)' \
                    "$config_file" > "${config_file}.tmp"
                mv "${config_file}.tmp" "$config_file"
                echo "   ✓ Updated existing cowiki-mcp entry"
            else
                jq --arg url "$SERVER_URL" --arg key "$API_KEY" \
                    '.mcpServers["cowiki-mcp"] = {
                        "type": "http",
                        "url": $url,
                        "headers": {"Authorization": ("Bearer " + $key)}
                    }' \
                    "$config_file" > "${config_file}.tmp"
                mv "${config_file}.tmp" "$config_file"
                echo "   ✓ Added cowiki-mcp to existing config"
            fi
        else
            cp "$config_file" "${config_file}.bak"
            echo "$new_entry" > "$config_file"
            echo "   ⚠ Replaced entire config (backup saved). Install jq for smart merging."
        fi
    else
        mkdir -p "$(dirname "$config_file")"
        echo "$new_entry" > "$config_file"
        echo "   ✓ Created new $config_file"
    fi

    chmod 600 "$config_file" 2>/dev/null || true

    echo ""
    echo "✓ Claude Code MCP configured!"
    echo "  Restart Claude Code for changes to take effect."
    echo "  Verify with: claude mcp list"
}

# --- Execute ---
case "$MODE" in
    vscode)
        configure_vscode
        ;;
    claude-code)
        configure_claude_code
        ;;
    *)
        echo "ERROR: Unknown mode: $MODE"
        exit 1
        ;;
esac
