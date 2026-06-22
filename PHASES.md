# Gem Finder — Implementation Phases

This document breaks down the project into discrete phases for agent-driven development.
Each phase has clear deliverables and acceptance criteria.

---

## Phase 1: Foundation ✅ COMPLETE

**Goal:** Scaffold the entire project so it compiles and can be run.

### Deliverables
- [x] Cargo workspace monorepo with 4 crates: `shared`, `db`, `api`, `frontend`
- [x] `shared` — Domain types (`Movie`, `GemScore`, `BigHit`, `WatchlistEntry`, `PaginatedResponse`, `MovieSummary`, `HealthResponse`)
- [x] `shared` — Constants: weights, genre boosts, seeded gems list
- [x] `db` — Turso libSQL connection wrapper (`Database` struct supporting local + remote via env vars)
- [x] `db` — Migrations: `movies`, `watchlist`, `big_hits` tables + indexes
- [x] `db` — CRUD models: `upsert_movie`, `get_top_gems`, `get_movie_by_id`, `get_gems_count`
- [x] `api` — Axum server with `/health`, `/api/gems`, `/api/movies/{id}` routes
- [x] `api` — CORS, tracing middleware, `AppState` with Turso connection pool
- [x] `frontend` — Leptos CSR app: `App` shell, `HomePage`, `MovieDetail` placeholder, `AboutPage`
- [x] `frontend` — HTTP client (`api.rs`) using `reqwest`
- [x] `docker/Dockerfile` — Debian bookworm multi-stage (stable for Raspberry Pi arm64)
- [x] `docker/docker-compose.yml` — Turso env vars, persistent volume
- [x] `tailwind.config.js` + `assets/input.css`
- [x] `.github/workflows/ci.yml` — fmt + clippy + build + test
- [x] `ARCHITECTURE.md` + `README.md` + `.gitignore`
- [x] `PHASES.md` — This file

### Verification
```bash
cargo check --package gem-finder-api   # Must pass
cargo check --package gem-finder-db    # Must pass
```

### Notes
- Frontend Leptos 0.7 scaffolding is functional but may need minor API refinement
- Backend `gem-finder-api` compiles cleanly
- Database layer uses Turso `sync` feature for remote Turso Cloud + local fallback

---

## Phase 2: TMDB Data Ingestion ✅ COMPLETE

**Goal:** Populate the database with real movie data from TMDB.

### Deliverables
- [x] `api/src/services/tmdb_sync.rs` — TMDB API client & Ingestion service
- [x] `/api/admin/sync` endpoint
- [x] Trigger manual sync via API

### TMDB API Endpoints Needed
- `/discover/movie` — Paginated movie discovery with filters
- `/movie/{id}` — Details for a specific movie (IMDb ID, ratings, genres)
- `/movie/{id}/credits` — Director, cast
- `/configuration` — Poster base URL

### Implementation Notes
1. Fetch movies released from 1960 onward
2. Filter to IMDb rating 6.0–8.0 range (wider than final gem range to allow algorithm scoring)
3. Minimum vote count: 500 (avoid noise)
4. Store: title, year, genre, director, overview, poster URL, TMDB rating/votes, IMDb ID
5. Run initially as a one-time seed, then schedule periodic syncs
6. Use TMDB API key from `TMDB_API_KEY` env var

### Files to Modify/Create
```
crates/api/src/services/tmdb_sync.rs   # New
crates/api/src/routes/admin.rs          # New (future auth)
crates/db/src/tmdb_sync.rs              # New (optional, can be in api)
```

### Acceptance Criteria
```bash
# After seeding
cargo run --package gem-finder-api
# Database should contain ~N movies spanning 1960–present
```

---

## Phase 3: Hidden Gem Scoring Algorithm ✅ COMPLETE

**Goal:** Implement the weighted scoring algorithm that identifies hidden gems.

### Deliverables
- [x] `api/src/services/gem_score.rs` — `GemScoreCalculator`, batch scoring, RT hard gate
- [x] `api/src/services/tmdb_sync.rs` — era-windowed sync, blockbuster sync, acclaimed candidates sync
- [x] `api/src/services/omdb_sync.rs` — OMDb enrichment (IMDb rating, votes, RT critic score)
- [x] `db/src/models.rs` — acclaimed classification (`classify_acclaimed_films`, `get_acclaimed_films`, `get_acclaimed_count`)
- [x] `db/src/migrations.rs` — `acclaimed` table
- [x] `/api/acclaimed` endpoint
- [x] `seed-test-data` CLI subcommand wires full pipeline

### Algorithm (current weights — see `shared/src/constants.rs`)

```
Hard filters (return None):
  imdb < 6.5 or > 7.9 | votes < 500 | age < 3 years | rt_critic < 65

GemScore = weighted_sum(
  IMDb sweet spot [6.5–7.9]    → 0.30
  Vote ratio (low = hidden)    → 0.20
  Year decay (age/30).clamp()  → 0.40  ← dominant; 2023 film = 0.04, 1977 = 0.40
  Obscured by big hit          → 0.05
  RT critic signal [65→100]    → 0.05  (RT is primarily a gate, not a scorer)
) × genre_boost
```

**RT gate**: < 65% → excluded. No RT data → allowed (old films may not be on RT; enrichment must run before scoring).

**Data ingestion**: `sync_movies` runs in 4 era windows × 5 pages = ~400 candidates covering 1960–present. Without era windows, `sort_by=release_date.desc` returns only 2022-2023 films and recent acclaimed films dominate rankings.

### Acclaimed Films (separate category)
- **Table**: `acclaimed` — FK to movies, UNIQUE(movie_id)
- **Threshold**: IMDb ≥ 8.0 AND RT ≥ 80%
- **Endpoint**: `GET /api/acclaimed?page=&per_page=`
- **Sync**: `sync_acclaimed_candidates` (TMDB vote_avg ≥ 7.5, vote_count ≥ 10k) → OMDb enrichment → `classify_acclaimed_films`

### Acceptance Criteria ✅ Verified
- Sorcerer (1977): rank 1, ~85%
- The Hurt Locker (2008): rank 2, ~71%
- Dinner in America (2020): rank ~40, ~39%
- Known blockbusters excluded from pool (big_hits table)
- Bad films (RT < 65%) filtered — To Catch a Killer, Whitney Houston biopic excluded
- Teachers' Lounge (2023, Oscar-nominated) ≥ rank 10 ✓ (valid recent gem)

---

## Phase 4: Full Frontend UI

**Goal:** Complete the Leptos frontend with all views.

### Deliverables
- [ ] Movie grid with infinite scroll or pagination
- [ ] Movie detail page with full metadata
- [ ] Year/genre filter sidebar
- [ ] Sorting: by gem score, year, IMDb rating, newest
- [ ] Watchlist management (localStorage or backend)
- [ ] Responsive design
- [ ] Loading skeletons and error states

### Files to Complete
```
crates/frontend/src/pages.rs    # Fix Leptos router integration
crates/frontend/src/api.rs      # Already has fetch functions
```

### Leptos 0.7 Notes
- Router: `use_navigate()` for programmatic navigation
- Resources: `create_resource` for async data loading
- Images: Use actual TMDB poster URLs, provide SVG placeholder

### Acceptance Criteria
- User sees a grid of 20 movies on `/`
- Movies have posters, titles, years, gem scores, IMDb ratings
- Clicking a card navigates to `/movie/{id}`
- Filtering by year and genre works
- Works on mobile (responsive Tailwind classes)

---

## Phase 5: Polish, Auth & Deploy

**Goal:** Production-ready deployment on Raspberry Pi with future auth.

### Deliverables
- [ ] Authentication (magic link or simple password when going public)
- [ ] Rate limiting on API endpoints
- [ ] API keys for public data access
- [ ] Multi-arch Docker build (amd64 + arm64 for Raspberry Pi)
- [ ] GitHub Actions deploy workflow
- [ ] Health checks and monitoring
- [ ] Turso remote sync with local fallback

### Deployment Flow
```bash
# On Raspberry Pi
docker compose -f docker/docker-compose.yml up -d
```

### Acceptance Criteria
- App runs on Raspberry Pi 4/5 via Docker
- Turso remote sync works (if configured)
- API serves within 50ms p95 latency
- CI passes on every push

---

## Quick Reference: Crate Relationships

```
shared (types + constants)
  ├→ db (Turso connection + migrations + CRUD)
  │    └→ api (Axum backend)
  └→ frontend (Leptos WASM app, imports shared types)
```

## Quick Reference: Tech Versions

| Crate | Version | Notes |
|---|---|---|
| leptos | 0.7.x | CSR mode |
| axum | 0.8.x | Tokio-native |
| turso | 0.7.0-pre.10 | SQLite + sync |
| tokio | 1.x | Async runtime |
| reqwest | 0.12 | HTTP client |
| tailwindcss | 3.4.x | CSS (CLI build) |