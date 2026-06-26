#!/usr/bin/env bash
# Gem Finder — full VPS bootstrap + deploy
# Usage: sudo bash deploy/install.sh yourdomain.com
#
# Run from the repo root. Requires SECRETS.env in the repo root.
# Safe to re-run (idempotent). Only prerequisite: DNS A record pointing to this machine.

set -euo pipefail

DOMAIN="${1:-}"
INSTALL_DIR=/opt/gem-finder

# ── Preflight ────────────────────────────────────────────────────────────────

if [[ $EUID -ne 0 ]]; then
    echo "Error: run as root — sudo bash deploy/install.sh yourdomain.com"
    exit 1
fi

if [[ -z "$DOMAIN" ]]; then
    echo "Usage: sudo bash deploy/install.sh yourdomain.com"
    exit 1
fi

if [[ ! -f "Cargo.toml" ]]; then
    echo "Error: run from the gem-finder repo root (Cargo.toml not found)"
    exit 1
fi

if [[ ! -f "SECRETS.env" ]]; then
    echo "Error: SECRETS.env not found."
    echo "  cp SECRETS.env.example SECRETS.env && nano SECRETS.env"
    exit 1
fi

echo ""
echo "╔══════════════════════════════════════════╗"
echo "║    Gem Finder — VPS bootstrap + deploy   ║"
echo "║    Domain: $DOMAIN"
echo "╚══════════════════════════════════════════╝"
echo ""

# ── Step 1: System dependencies ──────────────────────────────────────────────

echo "▶ [1/7] System dependencies"
apt-get update -qq
apt-get install -y -qq \
    build-essential \
    pkg-config \
    libssl-dev \
    git \
    curl

# ── Step 2: Caddy ─────────────────────────────────────────────────────────────

echo "▶ [2/7] Caddy"
if ! command -v caddy &>/dev/null; then
    apt-get install -y -qq debian-keyring debian-archive-keyring apt-transport-https
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
        | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
    curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
        | tee /etc/apt/sources.list.d/caddy-stable.list > /dev/null
    apt-get update -qq
    apt-get install -y -qq caddy
    echo "   Caddy installed"
else
    echo "   Caddy already installed — skipping"
fi

# ── Step 3: Rust + Trunk ──────────────────────────────────────────────────────

echo "▶ [3/7] Rust + Trunk"
CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
if ! "$CARGO_HOME/bin/cargo" version &>/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal
    echo "   Rust installed"
else
    echo "   Rust already installed — skipping"
fi

# shellcheck source=/dev/null
source "$CARGO_HOME/env"

rustup target add wasm32-unknown-unknown 2>/dev/null || true

if ! "$CARGO_HOME/bin/trunk" version &>/dev/null 2>&1; then
    "$CARGO_HOME/bin/cargo" install trunk
    echo "   Trunk installed"
else
    echo "   Trunk already installed — skipping"
fi

# ── Step 4: Build backend ─────────────────────────────────────────────────────

echo "▶ [4/7] Build backend (cargo build --release)"
"$CARGO_HOME/bin/cargo" build --release --package gem-finder-api

# ── Step 5: Build frontend ────────────────────────────────────────────────────

echo "▶ [5/7] Build frontend (trunk build --release)"
(cd crates/frontend && "$CARGO_HOME/bin/trunk" build --release)

# ── Step 6: Install files + service ──────────────────────────────────────────

echo "▶ [6/7] Install files + systemd service"

useradd --system --no-create-home --shell /bin/false gem-finder 2>/dev/null \
    && echo "   Created gem-finder user" \
    || echo "   gem-finder user already exists — skipping"

mkdir -p "$INSTALL_DIR"
cp target/release/gem-finder-api "$INSTALL_DIR/"
cp -r crates/frontend/dist "$INSTALL_DIR/"
cp SECRETS.env "$INSTALL_DIR/.env"
chown -R gem-finder:gem-finder "$INSTALL_DIR"
chmod 600 "$INSTALL_DIR/.env"
chmod +x "$INSTALL_DIR/gem-finder-api"

cp deploy/gem-finder.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable gem-finder
systemctl restart gem-finder
echo "   Service started"

# ── Step 7: Configure Caddy ───────────────────────────────────────────────────

echo "▶ [7/7] Configure Caddy"
sed "s/yourdomain.com/$DOMAIN/g" Caddyfile > /etc/caddy/Caddyfile
systemctl reload caddy
echo "   Caddy configured for $DOMAIN (TLS via Let's Encrypt)"

# ── Done ──────────────────────────────────────────────────────────────────────

echo ""
echo "✓ Deployed to https://$DOMAIN"
echo ""
echo "  Status:     systemctl status gem-finder"
echo "  Logs:       journalctl -u gem-finder -f"
echo "  Populate:   https://$DOMAIN/admin  →  Sync → Enrich → Score"
echo "  Smoke test: bash scripts/smoke-test.sh https://$DOMAIN"
