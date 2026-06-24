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
  imdb < 6.5 or > 7.9 | votes < 500 | released this year | rt_critic < 65

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

## Phase 4: Full Frontend UI ✅ COMPLETE

**Goal:** Complete the Leptos frontend with all views.

### Deliverables
- [x] Movie grid with pagination (20 per page, prev/next controls)
- [x] Movie detail page with full metadata (poster, overview, scores, IMDb link)
- [x] Year/genre filter sidebar
- [x] Sorting: by gem score (default), IMDb rating, year, newest
- [x] Responsive design (5-col grid → 2-col mobile)
- [x] Loading skeletons and error states (`SkeletonCard` component)
- [x] IMDb / RT badges on movie cards
- [x] `AcclaimedPage` — separate view for `GET /api/acclaimed`
- [x] Admin panel (`AdminPage`) — sync, enrich, score, logs; all operations browser-safe
- [ ] Watchlist (deferred — requires auth)

### Notes
- `<A attr:class=` required for Leptos 0.7 component attrs (FIX-46)
- Pagination disabled attrs hoisted to variables before `view!` macro (FIX-47)
- Admin ops use `tokio::spawn` on the server; browser tab close does not cancel them (FIX-57)
- Admin panel includes timing warnings (Sync: 5–15 min, Enrich: 10–30 min, Score: < 1 min)
- OMDb limit is user-passable (1–50,000 input), not hardcoded (FIX-58)

---

## Phase 5: Docker Deployment ✅ COMPLETE (core)

**Goal:** Single-command local and production deployment via Docker.

### Deliverables
- [x] Multi-stage `docker/Dockerfile` — frontend-builder + api-builder + debian:bookworm-slim runtime
- [x] Pre-built trunk v0.21.14 + wasm-bindgen-cli v0.2.125 binaries in Docker (avoids slow `cargo install`)
- [x] `.dockerignore` — excludes `target/`, `node_modules/`, `SECRETS.env`, `.git/`, `*.db`
- [x] `docker-compose.yml` at repo root (not in `docker/`) with named volume for DB persistence
- [x] Image named `gem-finder` (not `docker-app-1`)
- [x] Database always local SQLite in Docker — `TURSO_DATABASE_URL=` overrides prevent cloud use
- [x] API keys loaded from host env / `.env` file via Docker Compose variable substitution
- [x] `Makefile` + `scripts/run.ps1` for single-command local dev build+run
- [x] `SERVE_FRONTEND=1` env var gates static file serving in API binary

### Remaining
- [ ] Authentication (magic link or password for public access)
- [ ] Rate limiting on API endpoints
- [ ] Multi-arch Docker build (amd64 + arm64 for Raspberry Pi)
- [ ] GitHub Actions deploy workflow

### Docker Quick Start
```bash
# Copy secrets for Docker Compose substitution
cp SECRETS.env .env          # Linux/macOS
Copy-Item SECRETS.env .env   # Windows PowerShell

docker compose build && docker compose up
# App at http://localhost:3000
# DB persists in volume 'gem-data'; lost only on: docker compose down -v
```

### Acceptance Criteria
- App runs in Docker with `docker compose up` from repo root ✅
- API keys loaded from `.env` (not baked into image) ✅
- DB survives `docker compose build` and restart ✅
- Browser tab close does not cancel sync/enrich/score ✅
- CI passes on every push ✅

---

---

## Phase 6: Algorithm Refinements & Wildcards ✅ COMPLETE

**Goal:** Improve scoring accuracy and surface divisive films separately.

### Deliverables
- [x] RT as **credibility multiplier** on vote_ratio (not additive): rt≥70→1.0, rt∈[40,70)→linear, rt<40→0.0, None→0.8
- [x] Removed `CRITIC_DISPARITY` weight; `VOTE_RATIO` raised from 0.20 → 0.25 (total stays 1.00)
- [x] `clear_all_gem_scores` — reset gem_score/gem_rank before each batch run (eliminates stale scores)
- [x] `wildcards` table — films that score algorithmically but have rt_critic < 50%
- [x] `classify_wildcards` — INSERT OR IGNORE post-scoring; called in trigger_score, trigger_seed, seed CLI
- [x] `GET /api/wildcards` — paginated, ordered by gem_score DESC
- [x] `WildcardsPage` at `/wildcards` in frontend with contextual description
- [x] OMDb enrichment hard limit removed (2000 → `i64::MAX`) in all three call sites

### Why Wildcards
Low vote counts can mean genuine undiscovery (true gem) or informed avoidance (critics warned audiences off).
The RT multiplier distinguishes: high RT + low votes = critics endorsed + audiences missed = true gem.
Low RT + low votes = audiences heeded critics = wildcard, not gem.

---

## Phase 7: Visual Design ✅ COMPLETE (base)

**Goal:** Move away from the default dark-blue/emerald "AI aesthetic" toward something with character.

### Deliverables
- [x] **Palette C — Red Latitude**: warm brown-dark backgrounds (`#0d0906`, `#17100a`, `#201610`), rust-orange accent (`#c2410c` / orange-700). No green tint — avoids Steam-era aesthetic.
- [x] **Bebas Neue** for display headings (h1, nav brand) — condensed, uncompromising, 70s poster energy
- [x] **Barlow** body font — functional, readable
- [x] **Film grain** — SVG fractalNoise pseudo-element (opacity 3.8%), fixed overlay. Nod to 35mm practical filmmaking.
- [x] `appearance: none` on select elements + custom SVG chevron — browser native styling no longer overrides dark palette
- [x] Footer Sorcerer reference — clickable IMDb link to Sorcerer (1977), the original hidden gem (buried by Star Wars same summer)

### Planned
- [ ] **Style picker** — CSS custom property switching (see Phase 8). Requires refactoring hardcoded hex values to CSS vars first.

### Reference
William Friedkin's *Sorcerer* (1977) — warm amber headlights in rain, 35mm grain, jungle darkness, brutalist tension. The film was destroyed at the box office by Star Wars opening the same summer, making it arguably the first hidden gem. The design pays tribute.

---

## Phase 8: User Features 📋 PLANNED

**Goal:** Add authenticated user accounts with personal watchlists, ratings, and preferences.

### Why This Phase Requires Early Architectural Decisions

Before implementation begins, confirm the following design choices (they affect the database schema, API shape, and migration strategy):

#### 8a — URL Identity (Movie Slugs)
**Current:** `/movie/{db_integer_id}` — sequential IDs are IDOR risk when user data is attached.
**Proposed:** `/movie/{slug}` e.g. `/movie/sorcerer-1977`
- Slug format: `{title-kebab-case}-{year}`, with disambiguation suffix for collisions (e.g. `-2`)
- Requires: `slug TEXT UNIQUE NOT NULL` column in `movies` table, generated on upsert
- Migration: one-time backfill on existing rows; all new upserts generate slug automatically
- **Decision needed:** slug-only URLs, or support both integer ID and slug during transition?

#### 8b — Authentication Strategy
Options:
1. **Magic link** (email) — no passwords, passwordless flow, requires email sending infrastructure
2. **Password + bcrypt** — simpler server-side, more traditional
3. **OAuth only** (GitHub/Google) — no credentials stored, but external dependency
4. **No auth (public features only)** — watchlist stored locally in browser (localStorage), no server sync
- **Decision needed:** which auth strategy, and is user data server-synced or client-only?

#### 8c — Watchlist & Ratings Model
- Watchlist: per-user list of movie IDs; states = `want_to_watch | watched | not_interested`
- Ratings: user-supplied 1–10 score (separate from gem score)
- Notes: optional freetext per movie
- **Decision needed:** all three features, or start with watchlist-only?

#### 8d — Style Picker Persistence
- Theme choice must survive page refresh → CSS custom properties + localStorage
- Requires: refactor all hardcoded hex values in frontend to CSS vars (`--sc-bg`, `--sc-panel`, etc.)
- Leptos component writes to `document.documentElement.style` on change
- Accessibility case: higher-contrast mode for dim-environment browsing
- **Decision needed:** implement in Phase 8 or defer to Phase 9?

### Deliverables (pending architectural decisions)
- [ ] `users` table — id (UUID), email, created_at, last_login
- [ ] `watchlist` table — user_id FK, movie_id FK, state, rating, notes, timestamps
- [ ] `slug` column on `movies` — backfill migration + generation on upsert
- [ ] Auth middleware (JWT or session cookie)
- [ ] `GET/POST /api/watchlist` endpoints
- [ ] Frontend: watchlist toggle on MovieCard, WatchlistPage, auth flow
- [ ] Style picker component + CSS variable refactor
- [ ] Update router: `/movie/:id` → `/movie/:slug`

---

## Phase 9: Production Hardening 📋 PLANNED

**Goal:** Harden for public deployment.

### Deliverables
- [ ] Rate limiting on API endpoints (tower middleware)
- [ ] Multi-arch Docker build (amd64 + arm64 for Raspberry Pi)
- [ ] GitHub Actions deploy workflow
- [ ] Fix Docker build: trunk "root package not found" (Task #25)
- [ ] Authentication in public deployment
- [ ] Structured error responses (replace ad-hoc StatusCode returns)
- [ ] API versioning (`/api/v1/...`)
- [ ] Health check improvements (DB connectivity probe)
- [ ] HTTPS termination guidance for reverse-proxy setup (nginx/Caddy)

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