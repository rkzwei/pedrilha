#!/usr/bin/env bash
# Gem Finder — VPS system bootstrap (no build)
# Sets up Caddy, systemd service, directories, and deploy permissions.
# Binaries are deployed by GitHub Actions CI after this runs.
#
# Usage: sudo bash deploy/bootstrap.sh yourdomain.com
# Run from the repo root. Requires SECRETS.env in the repo root.

set -euo pipefail

DOMAIN="${1:-}"
INSTALL_DIR=/opt/gem-finder

# ── Preflight ────────────────────────────────────────────────────────────────

if [[ $EUID -ne 0 ]]; then
    echo "Error: run as root — sudo bash deploy/bootstrap.sh yourdomain.com"
    exit 1
fi

if [[ -z "$DOMAIN" ]]; then
    echo "Usage: sudo bash deploy/bootstrap.sh yourdomain.com"
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
echo "║    Gem Finder — VPS bootstrap (no build) ║"
echo "║    Domain: $DOMAIN"
echo "╚══════════════════════════════════════════╝"
echo ""

# ── Step 1: System dependencies ──────────────────────────────────────────────

echo "▶ [1/5] System dependencies"
apt-get update -qq
apt-get install -y -qq \
    build-essential \
    pkg-config \
    libssl-dev \
    rsync \
    git \
    curl

# ── Step 2: Caddy ─────────────────────────────────────────────────────────────

echo "▶ [2/5] Caddy"
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

# ── Step 3: Directories + service user ───────────────────────────────────────

echo "▶ [3/5] Directories + service user"

useradd --system --no-create-home --shell /bin/false gem-finder 2>/dev/null \
    && echo "   Created gem-finder user" \
    || echo "   gem-finder user already exists — skipping"

mkdir -p "$INSTALL_DIR/dist"
cp SECRETS.env "$INSTALL_DIR/.env"
chown -R gem-finder:gem-finder "$INSTALL_DIR"
chmod 600 "$INSTALL_DIR/.env"

# Grant the deploy user (rk) ownership of the binary path so CI can rsync into it
DEPLOY_USER="${SUDO_USER:-rk}"
chown "$DEPLOY_USER":"$DEPLOY_USER" "$INSTALL_DIR" "$INSTALL_DIR/dist" 2>/dev/null || true
echo "   Directories ready under $INSTALL_DIR"

# ── Step 4: Sudoers for CI deploy user ───────────────────────────────────────

echo "▶ [4/5] Sudoers (deploy permissions for $DEPLOY_USER)"
cat > /etc/sudoers.d/gem-finder-deploy <<EOF
$DEPLOY_USER ALL=(ALL) NOPASSWD: /bin/systemctl restart gem-finder, /bin/systemctl stop gem-finder, /bin/mv $INSTALL_DIR/gem-finder-api.new $INSTALL_DIR/gem-finder-api
EOF
chmod 440 /etc/sudoers.d/gem-finder-deploy
echo "   Sudoers configured"

# ── Step 5: Systemd service + Caddy ──────────────────────────────────────────

echo "▶ [5/5] Systemd service + Caddy"

cp deploy/gem-finder.service /etc/systemd/system/
systemctl daemon-reload
systemctl enable gem-finder
echo "   Service enabled (not started — binary not yet deployed)"

sed "s/yourdomain.com/$DOMAIN/g" Caddyfile > /etc/caddy/Caddyfile
systemctl reload caddy
echo "   Caddy configured for $DOMAIN"

# ── Done ──────────────────────────────────────────────────────────────────────

echo ""
echo "✓ Bootstrap complete. Next: push to main — CI will compile and deploy the binary."
echo ""
echo "  Once deployed:"
echo "  Status:     systemctl status gem-finder"
echo "  Logs:       journalctl -u gem-finder -f"
echo "  Populate:   https://$DOMAIN/admin"
