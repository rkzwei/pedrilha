# Gem Finder

Discover hidden gem movies — incredible films buried under blockbusters and wrongly-rated movies.

## How It Works

Gem Finder surfaces films sitting in the 6.5–7.9 community rating sweet spot that most people never see. The scoring algorithm weighs:

- **Year decay** — older undiscovered films score higher (dominant factor, 40% weight)
- **Vote ratio** — high rating + low vote count = genuinely undiscovered
- **Rating sweet spot** — not a crowd favourite, not a dud
- **Critic quality gate** — films with RT critic score < 65% are excluded entirely
- **Obscured by blockbuster** — released within 6 weeks of a cultural juggernaut

Films are scored against the whole population; the top-ranked gem in any run = 100%.

---

## Deploy to VPS (Systemd + Caddy)

This is the recommended production setup — a single Linux VPS, no Docker, managed by systemd with Caddy as the reverse proxy.

### Prerequisites

**On your local machine:**
- Rust toolchain with the Linux target: `rustup target add x86_64-unknown-linux-musl`
- Or build directly on the VPS (see step 3b)
- [Trunk](https://trunkrs.dev/) for the WASM frontend: `cargo install trunk`

**On the VPS (Debian/Ubuntu):**
```bash
# System packages
sudo apt update && sudo apt install -y build-essential pkg-config libssl-dev git curl

# Rust (if building on VPS)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Trunk (if building on VPS)
cargo install trunk
rustup target add wasm32-unknown-unknown
```

### Step 1 — Point your DNS

Create an A record pointing your domain to the VPS IP. Caddy needs this to issue a TLS certificate.

```
yourdomain.com  A  <VPS IP>
```

Propagation takes seconds to minutes with Hostinger DNS.

### Step 2 — Install Caddy on the VPS

```bash
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https curl
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
  | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
  | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update && sudo apt install caddy
```

### Step 3 — Build the app

**Option A: Build on VPS directly (simplest)**

```bash
# Clone and build on the VPS
git clone https://github.com/youruser/gem-finder /opt/gem-finder-src
cd /opt/gem-finder-src

# Build the backend
cargo build --release --package gem-finder-api

# Build the frontend
cd crates/frontend
trunk build --release
cd ../..
```

**Option B: Cross-compile on macOS/Linux, then scp to VPS**

```bash
# Local machine
cross build --release --target x86_64-unknown-linux-musl  # requires `cross` + Docker
cd crates/frontend && trunk build --release && cd ../..

# Copy binaries to VPS
scp target/x86_64-unknown-linux-musl/release/gem-finder-api user@vps:/tmp/
scp -r crates/frontend/dist user@vps:/tmp/
```

### Step 4 — Create SECRETS.env

On the VPS, create `/opt/gem-finder/SECRETS.env` (the install script copies this to `.env`):

```bash
sudo mkdir -p /opt/gem-finder
sudo nano /opt/gem-finder/SECRETS.env
```

Minimum required for a working production deployment:

```env
# Data sources (required)
TMDB_API_KEY=your_tmdb_key
OMDB_API_KEY=your_omdb_key

# Admin panel protection (required in production — generate with: openssl rand -hex 32)
ADMIN_TOKEN=your_random_token_here

# App URL (required — used in magic-link emails)
APP_URL=https://yourdomain.com

# CORS — restrict API to your domain
CORS_ORIGINS=https://yourdomain.com

# Serve the compiled WASM frontend
SERVE_FRONTEND=1

# Log rotation (never/daily/hourly — restart required to change)
LOG_ROTATION=never
```

Add SMTP vars if you want auth/watchlist features (see Configuration Reference below).

### Step 5 — Run the install script

From the source directory on the VPS:

```bash
sudo bash deploy/install.sh
```

This script:
- Creates a `gem-finder` system user with no shell access
- Copies the binary to `/opt/gem-finder/gem-finder-api`
- Copies `dist/` to `/opt/gem-finder/dist/`
- Copies `SECRETS.env` to `/opt/gem-finder/.env`
- Installs and enables the systemd unit
- Starts the service

Check it started:

```bash
systemctl status gem-finder
journalctl -u gem-finder -f
```

You should see `Listening on 0.0.0.0:3000` in the logs.

### Step 6 — Configure Caddy

Edit the `Caddyfile` in the project root and replace `yourdomain.com` with your actual domain:

```bash
sudo nano /opt/gem-finder-src/Caddyfile
```

Copy it to Caddy's config location and start:

```bash
sudo cp /opt/gem-finder-src/Caddyfile /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

Caddy will automatically obtain a TLS certificate from Let's Encrypt. Visit `https://yourdomain.com` — you should see the Gem Finder UI.

> **Note:** If Axum's `CompressionLayer` is active in the binary, remove the `encode gzip zstd` block from the Caddyfile to avoid double-encoding responses.

### Step 7 — Populate the database

Open `https://yourdomain.com/admin` and run operations in order:

1. **Sync** — pulls movies from TMDB (4 era windows, 1960–present). Takes 5–15 min.
2. **Enrich** — fetches IMDb community ratings and RT critic scores. Takes 10–30 min. Default limit: 10,000 movies.
3. **Score** — runs the gem algorithm and ranks all movies. Takes < 1 min.

After scoring, the main page will show results.

### Step 8 — Smoke test

```bash
bash scripts/smoke-test.sh https://yourdomain.com
```

All 8 tests should pass.

---

## Updating

```bash
# On the VPS (or rebuild locally and scp)
cd /opt/gem-finder-src
git pull

# Rebuild backend
cargo build --release --package gem-finder-api

# Rebuild frontend (only if frontend changed)
cd crates/frontend && trunk build --release && cd ../..

# Deploy
sudo systemctl stop gem-finder
sudo cp target/release/gem-finder-api /opt/gem-finder/
sudo cp -r crates/frontend/dist /opt/gem-finder/
sudo systemctl start gem-finder
```

---

## Database Backup

The database is a single SQLite file at `/opt/gem-finder/gem_finder.db`.

**Manual backup:**
```bash
sqlite3 /opt/gem-finder/gem_finder.db ".backup /opt/gem-finder/backups/gem_finder_$(date +%Y%m%d).db"
```

**Automated daily backup via cron:**
```bash
sudo mkdir -p /opt/gem-finder/backups
sudo crontab -u gem-finder -e
# Add:
# 0 3 * * * sqlite3 /opt/gem-finder/gem_finder.db ".backup /opt/gem-finder/backups/gem_finder_$(date +\%Y\%m\%d).db"
```

---

## Logs

```bash
# Follow live logs
journalctl -u gem-finder -f

# Last 100 lines
journalctl -u gem-finder -n 100

# Since last restart
journalctl -u gem-finder --since "$(systemctl show -p ActiveEnterTimestamp gem-finder | cut -d= -f2)"
```

If `LOG_ROTATION` is set to `daily` or `hourly`, a `gem_finder.log` file is also written in the working directory (`/opt/gem-finder/`). Requires service restart to take effect.

---

## Configuration Reference

Copy `SECRETS.env.example` to `SECRETS.env` and fill in values. The `.env` file at `/opt/gem-finder/.env` is read by the systemd unit via `EnvironmentFile=`.

| Variable | Required | Description |
|---|---|---|
| `TMDB_API_KEY` | **Yes** | Movie discovery and metadata — [get one free](https://developer.themoviedb.org) |
| `OMDB_API_KEY` | **Yes** | Community ratings and RT critic scores — [get one free](https://www.omdbapi.com/apikey.aspx) |
| `ADMIN_TOKEN` | **Production** | Bearer token for `/api/admin/*`. Leave empty only for local dev — admin routes are unprotected without it. Generate: `openssl rand -hex 32` |
| `APP_URL` | **Production** | Public base URL (no trailing slash), e.g. `https://gemfinder.example.com`. Used in magic-link emails. |
| `CORS_ORIGINS` | **Production** | Comma-separated allowed origins, e.g. `https://yourdomain.com`. Empty = permissive (dev only). |
| `SERVE_FRONTEND` | **Production** | Set to `1` to serve the compiled WASM from `dist/`. Leave unset for local dev (Trunk serves the frontend). |
| `LOG_ROTATION` | No | `never` (default), `daily`, or `hourly`. Controls rolling log file in addition to journald. Restart required. |
| `DATABASE_URL` | No | Explicit path for the SQLite file. Defaults to `gem_finder.db` in the working directory. |
| `SYNC_INTERVAL_HOURS` | No | Background re-sync interval in hours. Defaults to 24. |
| `JWT_SECRET` | Auth | Signs session tokens. Generate: `openssl rand -hex 32`. Rotating this invalidates all sessions. |
| `SMTP_HOST` | Auth | SMTP server hostname |
| `SMTP_PORT` | Auth | SMTP port (`465` for SSL, `587` for STARTTLS) |
| `SMTP_USER` | Auth | SMTP login username |
| `SMTP_PASSWORD` | Auth | SMTP password or app password |
| `SMTP_FROM` | Auth | From header, e.g. `Gem Finder <noreply@yourdomain.com>` |
| `WEBAUTHN_ORIGIN` | Auth | Full origin for passkeys, e.g. `https://gemfinder.example.com` |
| `WEBAUTHN_RP_ID` | Auth | Relying-party domain for passkeys, e.g. `gemfinder.example.com` |

Auth variables (`JWT_SECRET`, `SMTP_*`, `WEBAUTHN_*`) are all optional. Leave them unset to run without user accounts — the app works fully for film discovery; watchlist and sign-in UI are hidden.

---

## Local Development

```bash
# Backend (hot-reload with cargo-watch optional)
export TMDB_API_KEY=...
export OMDB_API_KEY=...
cargo run --package gem-finder-api

# Frontend (separate terminal — proxied to :3000)
cd crates/frontend && trunk serve
```

Open `http://localhost:3000`.

---

## Tech Stack

| | |
|---|---|
| Frontend | Leptos 0.7 (Rust → WASM, CSR mode) |
| Backend | Axum 0.8 (Tokio async) |
| Database | libSQL / Turso (local SQLite — no remote sync) |
| Styling | Tailwind CSS 3.4 |
| Build | Trunk (WASM bundler), Cargo workspace monorepo |
| Reverse proxy | Caddy (auto TLS) |
| Process management | systemd |

## License

MIT
