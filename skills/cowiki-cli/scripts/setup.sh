#!/usr/bin/env bash
# cowiki CLI Setup Script (Linux/macOS)
# Usage: bash setup.sh --api-key <key> [--url <url>] [--shell <shell>]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_DIR="$(dirname "$SCRIPT_DIR")"
REPO_ROOT="$(cd "$SKILL_DIR/../.." && pwd)"

# Defaults
API_KEY=""
SERVER_URL="http://localhost:3000"
SHELL_TYPE=""

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

usage() {
    cat <<EOF
Usage: $0 --api-key <key> [OPTIONS]

Options:
  --api-key <key>   API key for cowiki (required)
  --url <url>       Server URL (default: http://localhost:3000)
  --shell <shell>   Setup shell completions (bash, zsh, fish)
  -h, --help        Show this help

Example:
  $0 --api-key cw_abc123 --shell bash
EOF
    exit 1
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --api-key)
            API_KEY="$2"
            shift 2
            ;;
        --url)
            SERVER_URL="$2"
            shift 2
            ;;
        --shell)
            SHELL_TYPE="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            usage
            ;;
    esac
done

if [[ -z "$API_KEY" ]]; then
    echo -e "${RED}Error: --api-key is required${NC}"
    usage
fi

echo -e "${CYAN}=== cowiki CLI Setup ===${NC}"
echo ""

# Step 1: Build from source
echo -e "${YELLOW}[1/4] Building cowiki CLI...${NC}"
CLI_DIR="$REPO_ROOT/cli"
if [[ ! -d "$CLI_DIR" ]]; then
    echo -e "${RED}Error: cli/ directory not found at $CLI_DIR${NC}"
    echo "Make sure you're running this script from within the cowiki repository."
    exit 1
fi

cd "$CLI_DIR"
cargo build --release 2>&1 | tail -5
echo -e "${GREEN}✓ Build complete${NC}"
echo ""

# Step 2: Create config directory
echo -e "${YELLOW}[2/4] Creating config...${NC}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/cowiki"
mkdir -p "$CONFIG_DIR"

cat > "$CONFIG_DIR/config.toml" <<TOML
server_url = "$SERVER_URL"
api_key = "$API_KEY"
TOML

chmod 600 "$CONFIG_DIR/config.toml"
echo -e "${GREEN}✓ Config saved to $CONFIG_DIR/config.toml${NC}"
echo ""

# Step 3: Verify
echo -e "${YELLOW}[3/4] Verifying installation...${NC}"
BINARY="$CLI_DIR/target/release/cowiki"
if [[ -x "$BINARY" ]]; then
    echo -e "${GREEN}✓ Binary: $BINARY${NC}"
    "$BINARY" --version 2>/dev/null || echo -e "${YELLOW}  (version check skipped)${NC}"
else
    echo -e "${RED}✗ Binary not found at $BINARY${NC}"
    exit 1
fi

# Step 4: Shell completions
if [[ -n "$SHELL_TYPE" ]]; then
    echo -e "${YELLOW}[4/4] Setting up $SHELL_TYPE completions...${NC}"
    case "$SHELL_TYPE" in
        bash)
            echo "# cowiki completions" >> ~/.bashrc
            echo "source <(\"$BINARY\" completions bash)" >> ~/.bashrc
            echo -e "${GREEN}✓ Added to ~/.bashrc${NC}"
            echo -e "${CYAN}  Run: source ~/.bashrc${NC}"
            ;;
        zsh)
            echo "# cowiki completions" >> ~/.zshrc
            echo "source <(\"$BINARY\" completions zsh)" >> ~/.zshrc
            echo -e "${GREEN}✓ Added to ~/.zshrc${NC}"
            echo -e "${CYAN}  Run: source ~/.zshrc${NC}"
            ;;
        fish)
            mkdir -p ~/.config/fish/completions
            "$BINARY" completions fish > ~/.config/fish/completions/cowiki.fish
            echo -e "${GREEN}✓ Fish completions installed${NC}"
            ;;
        *)
            echo -e "${RED}Unknown shell: $SHELL_TYPE${NC}"
            ;;
    esac
else
    echo -e "${YELLOW}[4/4] Shell completions skipped${NC}"
    echo -e "${CYAN}  To add later: source <(cowiki completions bash)${NC}"
fi

echo ""
echo -e "${GREEN}=== Setup Complete ===${NC}"
echo ""
echo "Try it out:"
echo "  cowiki list"
echo "  cowiki search \"your topic\""
