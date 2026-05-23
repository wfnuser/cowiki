# cowiki MCP Setup Script (Windows PowerShell)
# Configures MCP client (VS Code or Claude Code) with cowiki server settings.
#
# Usage:
#   powershell -File setup.ps1 -VSCode -ApiKey cw_xxx [-Url http://localhost:8080/]
#   powershell -File setup.ps1 -ClaudeCode -ApiKey cw_xxx [-Url http://localhost:8080/]

param(
    [switch]$VSCode,
    [switch]$ClaudeCode,
    [string]$ApiKey = "",
    [string]$Url = "http://localhost:8080/"
)

# --- Validate ---
if (-not $VSCode -and -not $ClaudeCode) {
    Write-Error "Must specify -VSCode or -ClaudeCode"
    exit 1
}

if ($VSCode -and $ClaudeCode) {
    Write-Error "Cannot specify both -VSCode and -ClaudeCode"
    exit 1
}

if ([string]::IsNullOrEmpty($ApiKey)) {
    Write-Error "-ApiKey is required"
    exit 1
}

if (-not $ApiKey.StartsWith("cw_")) {
    Write-Warning "API key doesn't start with 'cw_'. Are you sure it's correct?"
}

# Ensure URL ends with /
if (-not $Url.EndsWith("/")) {
    $Url = $Url + "/"
}

# --- Helper: Build JSON config ---
function Get-CowikiMcpEntry {
    return @{
        "cowiki-mcp" = @{
            url     = $Url
            type    = "http"
            headers = @{
                Authorization = "Bearer $ApiKey"
            }
        }
    }
}

function Write-JsonFile {
    param([string]$Path, [hashtable]$Data)

    $json = $Data | ConvertTo-Json -Depth 4
    # ConvertTo-Json on older PS may produce overly escaped strings; use -Compress for clean output
    # but here we want readable output, so we accept the default formatting.
    Set-Content -Path $Path -Value $json -Encoding UTF8
    Write-Host "   ? Created/updated: $Path"
}

function Merge-McpConfig {
    param(
        [string]$ConfigPath,
        [string]$ServerKey,      # "servers" for VS Code, "mcpServers" for Claude
        [hashtable]$NewEntry
    )

    $existing = @{}
    if (Test-Path $ConfigPath) {
        try {
            $content = Get-Content -Path $ConfigPath -Raw -Encoding UTF8
            $existing = $content | ConvertFrom-Json -AsHashtable
        } catch {
            Write-Warning "Existing config is invalid JSON. Backing up and replacing."
            Copy-Item $ConfigPath "$ConfigPath.bak"
            $existing = @{}
        }
    }

    if (-not $existing.ContainsKey($ServerKey)) {
        $existing[$ServerKey] = @{}
    }

    # Merge: add or update cowiki-mcp
    foreach ($key in $NewEntry.Keys) {
        $existing[$ServerKey][$key] = $NewEntry[$key]
    }

    Write-JsonFile -Path $ConfigPath -Data $existing
}

# --- VS Code Mode ---
if ($VSCode) {
    Write-Host "? Configuring VS Code MCP: .vscode/mcp.json"

    $configDir = ".vscode"
    $configFile = Join-Path $configDir "mcp.json"

    if (-not (Test-Path $configDir)) {
        New-Item -ItemType Directory -Path $configDir | Out-Null
    }

    $entry = Get-CowikiMcpEntry
    Merge-McpConfig -ConfigPath $configFile -ServerKey "servers" -NewEntry $entry

    # Set restrictive permissions (Windows ACL)
    try {
        $acl = Get-Acl $configFile
        $acl.SetAccessRuleProtection($true, $false)
        $rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
            $env:USERNAME, "FullControl", "Allow"
        )
        $acl.SetAccessRule($rule)
        Set-Acl $configFile $acl
    } catch {
        Write-Warning "Could not set restrictive permissions on config file."
    }

    Write-Host ""
    Write-Host "? VS Code MCP configured!"
    Write-Host "  Restart VS Code or reload the window for changes to take effect."
}

# --- Claude Code Mode ---
if ($ClaudeCode) {
    Write-Host "? Configuring Claude Code MCP"

    $claudeJson = Join-Path $HOME ".claude.json"
    $claudeMcpJson = Join-Path $HOME ".claude" "mcp.json"

    if (Test-Path $claudeJson) {
        $configFile = $claudeJson
    } elseif (Test-Path $claudeMcpJson) {
        $configFile = $claudeMcpJson
    } else {
        $configFile = $claudeJson
    }

    Write-Host "   Config file: $configFile"

    $entry = Get-CowikiMcpEntry
    Merge-McpConfig -ConfigPath $configFile -ServerKey "mcpServers" -NewEntry $entry

    Write-Host ""
    Write-Host "? Claude Code MCP configured!"
    Write-Host "  Restart Claude Code for changes to take effect."
    Write-Host "  Verify with: claude mcp list"
}
