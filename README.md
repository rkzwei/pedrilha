# Gem Finder

Discover hidden gem movies — incredible films buried under blockbusters and wrongly-rated movies.

## How It Works

Gem Finder surfaces films sitting in the IMDb 6.5–7.9 sweet spot that most people never see. The scoring algorithm weighs:

- **Year decay** — older undiscovered films score higher (dominant factor, 40% weight)
- **Vote ratio** — high rating + low vote count = genuinely undiscovered
- **IMDb sweet spot** — not a crowd favourite, not a dud
- **RT quality gate** — films with RT critic score < 65% are excluded entirely
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

## Admin Panel

Visit `/admin` to populate the database. Operations run entirely on the server — closing the browser tab does not cancel them.

1. **Sync** — pulls movies from TMDB across 4 era windows (1960–present). ~5–15 min.
2. **Enrich** — fetches IMDb ratings and Rotten Tomatoes scores from OMDb. ~10–30 min. User-configurable limit (default 10,000, max 50,000).
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
