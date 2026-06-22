# Gem Finder — Architecture

A Rust + Leptos web application for discovering hidden gem movies.

## Overview

Gem Finder helps you find incredible movies that sit at a mid-7 IMDb rating
and get hidden under piles of blockbusters and wrongly-rated movies.

## Tech Stack

| Layer | Technology | Role |
|---|---|---|
| **Frontend** | Leptos (0.7.x) | Rust WASM frontend with fine-grained reactivity |
| **Backend** | Axum | Rust web framework (Tokio-native) |
| **Database** | Turso | SQLite-compatible edge database with cloud sync |
| **CSS** | Tailwind CSS | Utility-first styling |
| **Package Mgr** | Cargo workspace | Monorepo with shared types |

## Project Structure

```
gem-finder/
├── Cargo.toml              # Workspace root
├── docker/
│   ├── Dockerfile          # Debian-based multi-stage build (stable for RPi)
│   └── docker-compose.yml  # Local dev orchestration
├── crates/
│   ├── shared/             # Domain types, constants, scoring weights
│   ├── db/                 # Turso client, migrations, CRUD models
│   ├── api/                # Axum HTTP server
│   └── frontend/           # Leptos WASM app
├── assets/
│   ├── input.css           # Tailwind source (with custom scrollbar, base styles)
│   └── tailwind.css        # Generated output
├── tailwind.config.js
├── package.json            # Tailwind CLI only
├── .github/
│   └── workflows/
│       └── ci.yml          # Build, clippy, test pipeline
├── ARCHITECTURE.md         # This file
├── README.md
└── .gitignore
```

## Crate Dependencies

```
shared ← db ← api
  ↓
frontend (depends on shared)
```

### `shared`
Domain types shared between backend and frontend:
- `Movie`, `GemScore`, `WatchlistEntry`, `BigHit`, `MovieSummary` (used by both `/api/gems` and `/api/acclaimed`)
- API response wrappers (`PaginatedResponse<T>`)
- Constants: weights, genre boosts, seeded gems, acclaimed thresholds (`ACCLAIMED_MIN_IMDB=8.0`, `ACCLAIMED_MIN_RT=80`)

### `db`
Turso/libSQL database layer:
- `connection.rs` — Wrapper around Turso `Database`, supports local + remote + env vars
- `migrations.rs` — Schema creation: `movies`, `watchlist`, `big_hits`, `acclaimed`, `run_logs` tables + indexes
- `models.rs` — CRUD: gem scoring, acclaimed classification (`classify_acclaimed_films`, `get_acclaimed_films`, `get_acclaimed_count`), run logs, enrichment queue

### `api`
Axum backend:
- Routes: `/health`, `/api/gems`, `/api/acclaimed`, `/api/movies/{id}`, `/api/score`, `/api/admin/sync`, `/api/admin/enrich`, `/api/admin/logs`
- Middleware: CORS, tracing, Tower HTTP
- Uses Turso for persistence

### `frontend`
Leptos WASM frontend:
- `main.rs` — Router, layout shell, navigation
- `pages.rs` — HomePage (gem grid), MovieDetail, AboutPage
- `api.rs` — HTTP client to backend (reqwest)
- `components.rs` — Future shared components

## Database Schema

### `movies`
| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `tmdb_id` | INTEGER UNIQUE |
| `imdb_id` | TEXT |
| `title` | TEXT |
| `year` | INTEGER |
| `genre` | TEXT |
| `director` | TEXT |
| `overview` | TEXT |
| `poster_url` | TEXT |
| `tmdb_rating` | REAL |
| `tmdb_vote_count` | INTEGER |
| `imdb_rating` | REAL |
| `imdb_vote_count` | INTEGER |
| `rt_critic_score` | INTEGER |
| `rt_audience_score` | INTEGER |
| `gem_score` | REAL (calculated) |
| `gem_rank` | INTEGER |
| `release_date` | TEXT |

### `watchlist`
| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `movie_id` | FK → movies |
| `status` | TEXT CHECK (to_watch, watching, watched) |
| `added_at` | TEXT |

### `big_hits`
| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `movie_id` | FK → movies |
| `year` | INTEGER |
| `box_office_millions` | REAL |
| `popularity_score` | REAL |

### `acclaimed`
Films with IMDb ≥ 8.0 AND RT critic ≥ 80%. Populated by `classify_acclaimed_films()` after OMDb enrichment. Browsable separately from hidden gems.

| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `movie_id` | FK → movies UNIQUE |
| `created_at` | TEXT |

## Hidden Gem Algorithm

```
Hard filters (return None):
  - IMDb < 6.5 or > 7.9 (outside sweet spot)
  - vote_count < 500
  - age < MIN_GEM_AGE_YEARS (3)
  - rt_critic_score present AND < 65 (critics disliked it)

GemScore = weighted_sum(
  IMDb rating in [6.5, 7.9] sweet spot  → 0.30
  High rating / low vote ratio           → 0.20
  Year decay (age/30).clamp(0,1)         → 0.40  ← dominant
  Obscured by nearby blockbuster         → 0.05
  RT critic signal [65→100]→[0→1]        → 0.05
) × genre_boost
```

**Year decay** is the dominant component (0.40 weight). A 2023 film (age=3) scores 0.10; a 1977 film (age=49) scores 1.0. This prevents recent acclaimed films from dominating despite strong RT/IMDb scores.

**RT quality gate**: Films with RT < 65% are excluded entirely regardless of other signals. Films without RT data are allowed through — old films may not be on RT. Enrichment must run before scoring to ensure coverage.

**Data ingestion**: `sync_movies` runs in 4 era windows (1960-1984, 1984-1999, 1999-2012, 2012-present) × 5 pages each = ~400 candidates with balanced decade coverage.

**Genre boosts**: Foreign=1.3, Indie=1.25, Documentary=1.2, Drama=1.1, Thriller=1.05, Comedy=0.95, Horror=0.9, Romance=0.85, Action=0.8, Sci-Fi=0.75

**Seeded gems** (known greats to validate algorithm):
- Sorcerer (1977), Dinner in America (2020), The Hurt Locker (2009)

## Acclaimed Films

Separate browsable category at `/api/acclaimed`. Not hidden gems — films everyone should have seen.
- **Threshold**: IMDb ≥ 8.0 AND RT critic ≥ 80%
- **Sync**: `sync_acclaimed_candidates` discovers films with TMDB vote_avg ≥ 7.5 and vote_count ≥ 10,000
- **Classification**: `classify_acclaimed_films` INSERT OR IGNORE into `acclaimed` from enriched movies

## Deployment

### Local Development
```bash
cargo run --package gem-finder-api    # Start API server on :3000
```

### Docker
```bash
# If you have Turso remote configured
export TURSO_DATABASE_URL=libsql://your-db.turso.io
export TURSO_AUTH_TOKEN=your-token

docker compose -f docker/docker-compose.yml up
```

### Local SQLite (no Turso)
```bash
export DATABASE_URL=file:gem_finder.db
cargo run --package gem-finder-api
```

### Remote (Turso Cloud)
```bash
export TURSO_DATABASE_URL=libsql://your-db.turso.io
export TURSO_AUTH_TOKEN=your-token
cargo run --package gem-finder-api
```

## Merge Rules

See `.github/CODEOWNERS` for per-crate review assignments.

**Branch strategy:** `main` → `develop` → `feature/*`

**Requirements to merge to main:**
1. PR passes CI (build + clippy + test)
2. At least 1 review approval
3. No failing tests
4. `cargo fmt` applied

## Future Enhancements
- [ ] Data ingestion: TMDB sync pipeline
- [ ] Authentication (when going public)
- [ ] Watchlist per user
- [ ] Genre/year filtering in UI
- [ ] Advanced search
- [ ] Sorted by gem score, year, rating
- [ ] API keys for public access
- [ ] Frontend WASM to its own Docker image