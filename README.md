# Gem Finder

Discover hidden gem movies — incredible films buried under blockbusters and wrongly-rated movies.

## How It Works

Gem Finder surfaces films sitting in the 6.5–7.9 community rating sweet spot that most people never see. The scoring algorithm weighs:

- **Year decay** — older undiscovered films score higher (dominant factor, 40% weight)
- **Vote ratio** — high rating + low vote count = genuinely undiscovered
- **Rating sweet spot** — not a crowd favourite, not a dud
- **Critic quality gate** — films with critic score < 65% are excluded entirely
- **Obscured by blockbuster** — released within 6 weeks of a cultural juggernaut

Films are scored against the whole population; the top-ranked gem in any run = 100%.

## Quick Start

### Docker (recommended)

```bash
# 1. Add API keys to SECRETS.env (copy from SECRETS.env.example if provided)
#    TMDB_API_KEY=...
#    OMDB_API_KEY=...
#    ADMIN_TOKEN=...   (optional — leave unset to skip auth on admin endpoints)

# 2. Copy to .env for Docker Compose variable substitution
cp SECRETS.env .env          # Linux/macOS
Copy-Item SECRETS.env .env   # Windows PowerShell

# 3. Build and run
docker compose build && docker compose up
```

Open `http://localhost:3000`.

The database lives in Docker volume `gem-data` and survives rebuilds. It's only wiped by `docker compose down -v`.

### Local Dev

```bash
# Backend (uses local SQLite gem_finder.db by default)
export TMDB_API_KEY=...
export OMDB_API_KEY=...
cargo run --package gem-finder-api

# Frontend (separate terminal, proxied to :3000)
cd crates/frontend && trunk serve
```

Or as a single production build:

```bash
make run          # Linux/macOS
.\scripts\run.ps1 # Windows
```

## Configuration

Copy `SECRETS.env.example` to `SECRETS.env` and fill in values. For Docker Compose, also copy to `.env`.

| Variable | Required | Description |
|---|---|---|
| `TMDB_API_KEY` | Yes | Movie discovery and metadata — [get one free](https://developer.themoviedb.org) |
| `OMDB_API_KEY` | Yes | Community ratings and critic scores — [get one free](https://www.omdbapi.com/apikey.aspx) |
| `ADMIN_TOKEN` | No | Bearer token for `/api/admin/*` endpoints; leave empty to disable auth in dev |
| `JWT_SECRET` | Yes (auth) | Signs session tokens issued after magic-link login — see below |
| `SMTP_HOST` | Yes (auth) | SMTP server hostname |
| `SMTP_PORT` | Yes (auth) | SMTP port (465 for SSL, 587 for STARTTLS) |
| `SMTP_USER` | Yes (auth) | SMTP login username (usually an email address) |
| `SMTP_PASSWORD` | Yes (auth) | SMTP login password or app password |
| `SMTP_FROM` | Yes (auth) | Display name + address in the `From:` header, e.g. `Gem Finder <noreply@example.com>` |
| `APP_URL` | Yes (auth) | Public base URL of the app (no trailing slash), e.g. `https://gemfinder.example.com` — used to build magic-link URLs in emails |
| `TURSO_DATABASE_URL` | No | Turso cloud database URL for remote sync; leave empty to use local SQLite |
| `TURSO_AUTH_TOKEN` | No | Auth token for Turso cloud; required when `TURSO_DATABASE_URL` is set |
| `SYNC_INTERVAL_HOURS` | No | How often the background sync job runs (hours); defaults to 24 |
| `DATABASE_URL` | No | Explicit path override for the local SQLite file; defaults to `gem_finder.db` in cwd |
| `WEBAUTHN_ORIGIN` | No | Full origin for passkey auth, e.g. `https://gemfinder.example.com`; defaults to `http://localhost:3000` |
| `WEBAUTHN_RP_ID` | No | Relying-party domain for passkeys, e.g. `gemfinder.example.com`; defaults to `localhost` |

### JWT setup

`JWT_SECRET` signs and verifies the session tokens issued to users after a successful magic-link login. Any server restart with a different secret invalidates all active sessions.

Generate a secret:

```bash
openssl rand -base64 32
```

Paste the output as the value in `SECRETS.env`:

```env
JWT_SECRET=8J+Xk2mP...your-generated-value...
```

Keep this value secret and out of version control. Rotate it to force all users to re-authenticate.

### SMTP setup

SMTP is used to send the one-time magic-link login email. Any SMTP provider works.

**Recommended providers:**

- **[Resend](https://resend.com)** — generous free tier (3,000/month), developer-friendly
- **[Mailgun](https://mailgun.com)** — reliable, pay-as-you-go
- **Hostinger email** — use if you already have a Hostinger hosting plan (host: `smtp.hostinger.com`)
- **Gmail SMTP** — works for low volume; requires an [App Password](https://support.google.com/accounts/answer/185833) (not your main password)

**Minimal config example (Resend):**

```env
SMTP_HOST=smtp.resend.com
SMTP_PORT=465
SMTP_USER=resend
SMTP_PASSWORD=re_your_api_key_here
SMTP_FROM=Gem Finder <noreply@yourdomain.com>
APP_URL=https://gemfinder.example.com
```

**Gmail example:**

```env
SMTP_HOST=smtp.gmail.com
SMTP_PORT=587
SMTP_USER=you@gmail.com
SMTP_PASSWORD=xxxx-xxxx-xxxx-xxxx   # App Password, not your Gmail password
SMTP_FROM=Gem Finder <you@gmail.com>
APP_URL=http://localhost:3000
```

`APP_URL` must match the domain your users actually visit — it is prepended to the magic-link token path in every login email.

## Admin Panel

Visit `/admin` to populate the database. Operations run entirely on the server — closing the browser tab does not cancel them.

1. **Sync** — pulls movies from TMDB across 4 era windows (1960–present). ~5–15 min.
2. **Enrich** — fetches community ratings and critic scores via an external enrichment service. ~10–30 min. User-configurable limit (default 10,000, max 50,000).
3. **Score** — runs the gem scoring algorithm and ranks all movies. < 1 min.

Run them in order: Sync → Enrich → Score.

## Tech Stack

| | |
|---|---|
| Frontend | Leptos 0.7 (Rust → WASM, CSR mode) |
| Backend | Axum 0.8 (Tokio-native) |
| Database | Turso/libSQL (local SQLite; optional cloud sync in dev) |
| Styling | Tailwind CSS 3.4 |
| Build | Trunk (WASM bundler), Cargo workspace monorepo |

## License

MIT
