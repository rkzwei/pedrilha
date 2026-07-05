# Gem Finder — Implementation Phases

This document breaks down the project into discrete phases for agent-driven development.
Each phase has clear deliverables and acceptance criteria.

---

## Phase 1: Foundation ✅ COMPLETE

**Goal:** Scaffold the entire project so it compiles and can be run.

**Ethos:** C8 — there's a way to do it: the product exists, compiles, and runs.

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

**Ethos:** C2, C5 — accurate data keeps the promise; a broad population is where obscurity gets found.

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

**Ethos:** C2, C4, C5, C6 — the algorithm is the sentence: somewhat old (C4), few watched (C5), critics loved (C6), and the pick must hold up (C2).

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
- Known blockbusters excluded from pool (big_hits table)
- Bad films (RT < 65%) filtered — To Catch a Killer, Whitney Houston biopic excluded

### Calibration Snapshot (observed, never asserted — see ETHOS.md)
Seeded-gem ranks are calibration diagnostics, not pass/fail conditions. If a
seeded gem stops ranking highly, that is a red flag to investigate — not a
test to make pass. Observed at phase completion:
- Sorcerer (1977): rank 1, ~85%
- The Hurt Locker (2008): rank 2, ~71%
- Dinner in America (2020): rank ~40, ~39%
- Teachers' Lounge (2023, Oscar-nominated): ≥ rank 10 (valid recent gem)

---

## Phase 4: Full Frontend UI ✅ COMPLETE

**Goal:** Complete the Leptos frontend with all views.

**Ethos:** C3, C7 — one named movie, presented so discovery feels like finding a gem.

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

**Ethos:** C1, C8 — passing it on just happens: one command puts it where a friend can reach it.

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

**Ethos:** C2, C5, C6 — sharper obscurity signal and quality gates so the recommendation never disappoints.

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

**Ethos:** C7 — it felt like a hidden gem: character over spreadsheet.

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

## Phase 8: User Features 🔨 IN PROGRESS

**Goal:** Add authenticated user accounts with personal watchlists, public ratings, and theme switching.

**Ethos:** C1, C7 — word of mouth mechanics (public ratings, sharing-ready accounts) and delight (style picker).

### Architectural Decisions (confirmed 2026-06-23)

#### 8a — URL Identity: Opaque prefixed movie IDs ✅ DECIDED
Format: `/movie/mv16` — `mv` prefix + base36 integer (e.g. ID 42 → `mv16`).
- No DB schema change — router decodes prefix, underlying integer PK unchanged
- Chosen over slugs (collision logic + backfill cost) and UUIDs (ugly)
- Opaque but not random — short prefix + encoded integer

#### 8b — Authentication: Magic link via Hostinger SMTP ✅ DECIDED
- Passwordless email auth using `lettre` crate + user's Hostinger SMTP server
- No external email service needed — domain already hosted on Hostinger (port 465/587)
- Magic link = short-lived JWT (15min), one-time-use token stored in `magic_tokens` table
- `users` table: id (UUID), email, created_at, last_login

#### 8c — Watchlist & Ratings: States + public user rating ✅ DECIDED
- States: `want_to_watch | watched | not_interested`
- User rating: 1–10 integer (nullable) — **public aggregate** exposed on movie cards and detail
- User ratings are a future scoring signal: high community avg → candidate for "Gem Finder Acclaimed"
- No freetext notes in v1

#### 8d — Style Picker: Phase 8, CSS vars first ✅ DECIDED
- CSS variable refactor is **prerequisite** — do before any other Phase 8 code
- All hardcoded hex → `:root` CSS vars. All Tailwind `bg-[#hex]` → `bg-sc-*` custom tokens.
- Eliminates hex debt compounding on every new frontend component

### Implementation Order
1. [x] CSS variable refactor (foundation — blocks all UI work)
2. [x] Opaque prefixed ID routing (`mv` prefix + base36 in router + all link hrefs)
3. [ ] DB migrations: `users`, `magic_tokens`, `watchlist` tables
4. [ ] API: magic link send/verify endpoints, JWT session middleware
5. [ ] API: `GET/POST /api/watchlist`, `GET /api/movies/:id` with `avg_user_rating`
6. [ ] Frontend: auth flow (email input → magic link sent → token verify → session)
7. [ ] Frontend: watchlist toggle on MovieCard + WatchlistPage
8. [ ] Frontend: style picker component (CSS var swap + localStorage persistence)

### Deliverables
- [ ] `users` table — id (UUID), email, created_at, last_login
- [ ] `magic_tokens` table — token (UUID), user_id FK, expires_at, used_at
- [ ] `watchlist` table — user_id FK, movie_id FK, state, user_rating (1–10 nullable), created_at, updated_at
- [ ] `avg_user_rating` + `rating_count` on movie API responses
- [ ] Auth middleware (JWT Bearer token, 30-day session)
- [ ] `POST /api/auth/magic` — send magic link
- [ ] `GET /api/auth/verify?token=` — verify token, return JWT
- [ ] `GET/POST/DELETE /api/watchlist`
- [ ] Frontend auth flow + watchlist UI
- [ ] Style picker with CSS var switching

---

## Vendor Lock-in Policy

**Goal:** Ship fast, stay portable. One intentional lock, everything else abstracted.

### Intentional Lock
- **Turso/libSQL** — accepted. Turso's edge replication and offline-first concurrency model is worth the dependency. The `turso` crate wraps libSQL which is SQLite-compatible; migrating later is possible but not planned.

### Current Lock-ins to Watch

| Dependency | Lock-in Type | Mitigation |
|---|---|---|
| TMDB API | Movie data source | `MovieDataSource` trait (see below) |
| OMDb API | RT scores + IMDb ratings | Same trait |
| Hostinger SMTP | Email sending | `SMTP_HOST/PORT/USER/PASS` in env — any provider works |
| Turso/libSQL | Database | Intentional — accepted |
| Leptos 0.7 | Frontend framework | No abstraction needed — it's Rust, not a SaaS |
| Axum 0.8 | HTTP server | No abstraction needed |

### Rule for Future Features
**No hardcoded external service calls.** Every external dependency gets:
1. Credentials/endpoints from env vars (already done for TMDB, OMDb, SMTP)
2. A trait interface if the service could realistically be swapped (data sources)
3. A note in PHASES.md if it introduces new lock-in

### Planned: `MovieDataSource` Trait (Phase 9 or when a second source is needed)
```rust
// crates/api/src/services/movie_source.rs
#[async_trait]
pub trait MovieDataSource: Send + Sync {
    async fn discover_movies(&self, start_year: i32, end_year: Option<i32>) -> Result<Vec<Movie>>;
    async fn fetch_ratings(&self, imdb_id: &str) -> Result<Option<ExternalRatings>>;
    async fn poster_base_url(&self) -> Result<String>;
}
// TmdbSource implements MovieDataSource
// OmdbSource implements MovieDataSource
// Future: LetterboxdSource, local JSON seed, etc.
```
Until a second source is actually needed, keep the concrete implementations — the abstraction boundary is the trait definition, not the refactor.

---

## Phase 9: Production Hardening 📋 PLANNED

**Goal:** Harden for public deployment.

**Ethos:** C2, C8 — trust holds under load: the site stays great when a friend's friend shows up.

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