# cowiki CLI Setup Script (Windows PowerShell)
# Usage: .\setup.ps1 -ApiKey <key> [-Url <url>] [-Shell <shell>]

param(
    [Parameter(Mandatory=$true)]
    [string]$ApiKey,

    [string]$Url = "http://localhost:3000",

    [ValidateSet("powershell", "bash", "zsh", "fish")]
    [string]$Shell
)

$ErrorActionPreference = "Stop"

Write-Host "=== cowiki CLI Setup ===" -ForegroundColor Cyan
Write-Host ""

# Step 1: Build from source
Write-Host "[1/4] Building cowiki CLI..." -ForegroundColor Yellow
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$SkillDir = Split-Path -Parent $ScriptDir
$RepoRoot = Split-Path -Parent $SkillDir
$CliDir = Join-Path $RepoRoot "cli"

if (-not (Test-Path $CliDir)) {
    Write-Host "Error: cli/ directory not found at $CliDir" -ForegroundColor Red
    Write-Host "Make sure you're running this script from within the cowiki repository."
    exit 1
}

Push-Location $CliDir
try {
    cargo build --release
    Write-Host "✓ Build complete" -ForegroundColor Green
} finally {
    Pop-Location
}
Write-Host ""

# Step 2: Create config
Write-Host "[2/4] Creating config..." -ForegroundColor Yellow
$ConfigDir = Join-Path $env:APPDATA "cowiki"
New-Item -ItemType Directory -Force -Path $ConfigDir | Out-Null

$ConfigContent = @"
server_url = "$Url"
api_key = "$ApiKey"
"@

Set-Content -Path (Join-Path $ConfigDir "config.toml") -Value $ConfigContent
Write-Host "✓ Config saved to $ConfigDir\config.toml" -ForegroundColor Green
Write-Host ""

# Step 3: Verify
Write-Host "[3/4] Verifying installation..." -ForegroundColor Yellow
$Binary = Join-Path $CliDir "target\release\cowiki.exe"
if (Test-Path $Binary) {
    Write-Host "✓ Binary: $Binary" -ForegroundColor Green
} else {
    Write-Host "✗ Binary not found at $Binary" -ForegroundColor Red
    exit 1
}

# Step 4: Shell completions
if ($Shell) {
    Write-Host "[4/4] Setting up $Shell completions..." -ForegroundColor Yellow
    switch ($Shell) {
        "powershell" {
            & $Binary completions powershell | Out-String | Invoke-Expression
            Write-Host "✓ PowerShell completions loaded for this session" -ForegroundColor Green
            Write-Host "  To make permanent, add to your PowerShell profile:" -ForegroundColor Cyan
            Write-Host "  & $Binary completions powershell | Out-String | Invoke-Expression"
        }
        "bash" {
            Write-Host "✓ For bash, add this to ~/.bashrc:" -ForegroundColor Green
            Write-Host "  source <($Binary completions bash)"
        }
        default {
            Write-Host "✓ Run: cowiki completions $Shell for setup" -ForegroundColor Green
        }
    }
} else {
    Write-Host "[4/4] Shell completions skipped" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== Setup Complete ===" -ForegroundColor Green
Write-Host ""
Write-Host "Try it out:"
Write-Host "  cowiki list"
Write-Host '  cowiki search "your topic"'
