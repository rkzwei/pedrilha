# Gem Finder

Discover hidden gem movies — incredible films buried under blockbusters and wrongly-rated movies.

## Ethos

Built from one sentence: *"My friend recommended me a movie another friend
showed him, said it was great, and I watched it… It felt like a hidden gem."*
Every feature must derive from a clause of that sentence — the
[Sorcerer Test](ETHOS.md). No clause, no feature.

## How It Works

Gem Finder surfaces films sitting in the 6.5–7.9 community rating sweet spot that most people never see. The scoring algorithm weighs:

- **Year decay** — older undiscovered films score higher (dominant factor, 40% weight)
- **Vote ratio** — high rating + low vote count = genuinely undiscovered
- **Rating sweet spot** — not a crowd favourite, not a dud
- **Critic quality gate** — films with RT critic score < 65% are excluded entirely
- **Obscured by blockbuster** — released within 6 weeks of a cultural juggernaut

Films are scored against the whole population; the top-ranked gem in any run = 100%.

---

## Deploy to VPS

Four steps. The bootstrap script handles everything after DNS.

### 1. Point DNS

Create an A record pointing your domain to the VPS IP. Caddy needs this to issue a TLS certificate.

```
yourdomain.com  A  <VPS IP>
```

### 2. Clone the repo on the VPS

```bash
git clone https://github.com/rkzwei/gem-finder
cd gem-finder
```

### 3. Create SECRETS.env

```bash
cp SECRETS.env.example SECRETS.env
nano SECRETS.env
```

Minimum required values:

```env
TMDB_API_KEY=your_tmdb_key
OMDB_API_KEY=your_omdb_key
ADMIN_TOKEN=<openssl rand -hex 32>
APP_URL=https://yourdomain.com
CORS_ORIGINS=https://yourdomain.com
SERVE_FRONTEND=1
```

See [Configuration Reference](#configuration-reference) for the full list.

### 4. Run the bootstrap script

```bash
sudo bash deploy/install.sh yourdomain.com
```

This handles everything: installs system dependencies, Caddy, Rust, Trunk; builds the backend and frontend; creates the `gem-finder` system user; installs and starts the systemd service; configures Caddy with auto TLS.

Takes 5–15 minutes on first run (Rust compile). Re-running is safe (idempotent).

### 5. Populate the database

Open `https://yourdomain.com/admin` and run in order:

1. **Sync** — pulls movies from TMDB. ~5–15 min.
2. **Enrich** — fetches ratings and critic scores. ~10–30 min.
3. **Score** — ranks all movies. < 1 min.

### 6. Smoke test

```bash
bash scripts/smoke-test.sh https://yourdomain.com
```

---

## Updating

```bash
git pull
sudo bash deploy/install.sh yourdomain.com
```

The script rebuilds and redeploys. The database at `/opt/gem-finder/gem_finder.db` is preserved.

---

## Database Backup

The database is a local libsql file at `/opt/gem-finder/gem_finder.db`. Back it up with a brief service stop (prevents a torn copy mid-write):

```bash
# One-off
systemctl stop gem-finder
cp /opt/gem-finder/gem_finder.db /opt/gem-finder/backups/gem_finder_$(date +%Y%m%d).db
systemctl start gem-finder
```

```bash
# Daily cron at 3am (add to root crontab: crontab -e)
mkdir -p /opt/gem-finder/backups
# 0 3 * * * systemctl stop gem-finder && cp /opt/gem-finder/gem_finder.db /opt/gem-finder/backups/gem_finder_$(date +\%Y\%m\%d).db && systemctl start gem-finder
```

---

## Logs

```bash
journalctl -u gem-finder -f          # live
journalctl -u gem-finder -n 100      # last 100 lines
```

If `LOG_ROTATION=daily` or `hourly` is set in `.env`, a `gem_finder.log` file is also written to `/opt/gem-finder/`. Requires service restart to take effect.

---

## Local Development

```bash
# Backend
export TMDB_API_KEY=... OMDB_API_KEY=...
cargo run --package gem-finder-api

# Frontend (separate terminal)
cd crates/frontend && trunk serve
```

Open `http://localhost:3000`.

---

## Configuration Reference

Copy `SECRETS.env.example` to `SECRETS.env`. On VPS, values are read from `/opt/gem-finder/.env`.

| Variable | Required | Description |
|---|---|---|
| `TMDB_API_KEY` | **Yes** | Movie discovery — [get one free](https://developer.themoviedb.org) |
| `OMDB_API_KEY` | **Yes** | Ratings + critic scores — [get one free](https://www.omdbapi.com/apikey.aspx) |
| `ADMIN_TOKEN` | **Production** | Bearer token for `/api/admin/*`. Generate: `openssl rand -hex 32` |
| `APP_URL` | **Production** | Public base URL, e.g. `https://gemfinder.example.com`. Used in magic-link emails. |
| `CORS_ORIGINS` | **Production** | Comma-separated allowed origins. Empty = permissive (dev only). |
| `SERVE_FRONTEND` | **Production** | Set to `1` to serve compiled WASM from `dist/`. |
| `LOG_ROTATION` | No | `never` (default), `daily`, or `hourly`. Restart required. |
| `DATABASE_URL` | No | Explicit SQLite path. Defaults to `gem_finder.db` in working directory. |
| `SYNC_INTERVAL_HOURS` | No | Background re-sync interval. Defaults to 24. |
| `JWT_SECRET` | Auth | Signs session tokens. Generate: `openssl rand -hex 32`. |
| `SMTP_HOST` | Auth | SMTP server hostname |
| `SMTP_PORT` | Auth | `465` (SSL) or `587` (STARTTLS) |
| `SMTP_USER` | Auth | SMTP login username |
| `SMTP_PASSWORD` | Auth | SMTP password or app password |
| `SMTP_FROM` | Auth | e.g. `Gem Finder <noreply@yourdomain.com>` |
| `WEBAUTHN_ORIGIN` | Auth | Full origin for passkeys, e.g. `https://gemfinder.example.com` |
| `WEBAUTHN_RP_ID` | Auth | Relying-party domain, e.g. `gemfinder.example.com` |

Auth variables (`JWT_SECRET`, `SMTP_*`, `WEBAUTHN_*`) are all optional. Leave unset to run without user accounts — film discovery works fully; sign-in and watchlist UI are hidden.

---

## Tech Stack

| | |
|---|---|
| Frontend | Leptos 0.7 (Rust → WASM, CSR mode) |
| Backend | Axum 0.8 (Tokio async) |
| Database | libSQL / Turso (local SQLite) |
| Styling | Tailwind CSS 3.4 |
| Build | Trunk (WASM bundler), Cargo workspace monorepo |
| Reverse proxy | Caddy (auto TLS) |
| Process management | systemd |

## License

MIT
