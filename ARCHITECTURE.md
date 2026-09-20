# Gem Finder — Architecture

A Rust + Leptos web application for discovering hidden gem movies.

## Overview

Gem Finder helps you find incredible movies that sit at a mid-7 IMDb rating
and get hidden under piles of blockbusters and wrongly-rated movies.

**Naming note (found stale 2026-07-05, flagging rather than silently deciding):**
the live product was rebranded to **"Pedrilha"** in commit `3652c45` (2026-07-02,
CHANGELOG 0.4.1) — every user-facing surface (page `<title>`, nav wordmark, footer,
all i18n strings in `crates/frontend/src/i18n.rs`) says "Pedrilha," not "Gem Finder."
This doc, `README.md`, `PHASES.md`, `FIXES.md`, and `GEM_FINDER_MEMORY.md` all still
use "Gem Finder" as if it were the current product name. Until confirmed otherwise,
treat "Gem Finder" as the internal/engineering name (repo, crate names
`gem-finder-*`, these docs) and "Pedrilha" as the public brand users see — but this
split has never been stated anywhere, so a new contributor has no way to know it's
intentional rather than an oversight.

## Ethos

Every feature must derive from the founding sentence in [ETHOS.md](ETHOS.md) —
the Sorcerer Test. Each phase in [PHASES.md](PHASES.md) cites the clauses
(C1–C8) it derives from; a feature that cannot cite a clause does not get
built. ETHOS.md is the single source of truth for the clauses and the
component→clause map — it is not duplicated here.

## Tech Stack

| Layer | Technology | Role |
|---|---|---|
| **Frontend** | Leptos (0.8.x) | Rust WASM frontend with fine-grained reactivity |
| **Backend** | Axum | Rust web framework (Tokio-native) |
| **Database** | Turso/libSQL | SQLite-compatible embedded database (local only in Docker; optional Turso cloud sync in dev) |
| **CSS** | Tailwind CSS | Utility-first styling |
| **Package Mgr** | Cargo workspace | Monorepo with shared types |

## Project Structure

```
gem-finder/
├── Cargo.toml              # Workspace root
├── docker-compose.yml      # Docker orchestration (run from repo root; alt to VPS deploy)
├── Makefile                # `make run` — build CSS + trunk + cargo run (Linux/macOS)
├── scripts/
│   ├── run.ps1             # Same as Makefile but for Windows PowerShell
│   └── smoke-test.sh       # Post-deployment smoke test (curl-based, see README)
├── deploy/
│   ├── install.sh          # VPS bootstrap: system deps, Caddy, systemd unit, build+deploy
│   └── runner/              # Self-hosted GitHub Actions runner compose config
├── docker/
│   ├── Dockerfile          # Multi-stage build: frontend-builder + api-builder + runtime
│   └── docker-compose.yml  # Stub — main file is at repo root
├── .dockerignore           # Excludes target/, SECRETS.env, .git/, *.db from build context
├── crates/
│   ├── shared/             # Domain types, constants, scoring weights
│   ├── db/                 # Turso/libSQL client, migrations, CRUD models
│   ├── api/                # Axum HTTP server
│   └── frontend/           # Leptos WASM app
├── assets/
│   ├── input.css           # Tailwind source (with custom scrollbar, base styles)
│   └── tailwind.css        # Generated output (from npm run build:css)
├── tailwind.config.js
├── package.json            # Tailwind CLI + build scripts
├── SECRETS.env             # API keys — gitignored, never committed
├── .github/
│   ├── workflows/
│   │   ├── ci.yml          # backend + frontend: fmt + clippy + test, on push/PR to dev|main
│   │   ├── deploy.yml      # on push to main: build + rsync + systemd restart on VPS
│   │   └── release.yml     # release-please automation
│   ├── dependabot.yml
│   └── CODEOWNERS
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
- `connection.rs` — Wrapper around Turso `Database`, supports local + remote + env vars; sets `busy_timeout(5s)` on every connection (SQLite pragmas are connection-scoped, not persisted at the DB-file level like `journal_mode=WAL`)
- `migrations.rs` — Versioned schema migrations `v1`–`v8` (see `schema_migrations` table for tracking). Current tables: `movies`, `watchlist`, `big_hits`, `acclaimed`, `wildcards`, `run_logs`, `users`, `magic_tokens`, `passkeys`, `events`, `movie_providers`, `provider_sync`, `user_providers`, `recommendations`, `rec_receipts`, `friendships` + indexes
- `models.rs` — CRUD: gem scoring, acclaimed classification (`classify_acclaimed_films`, `get_acclaimed_films`, `get_acclaimed_count`), wildcard classification, run logs, enrichment queue, auth (users/magic tokens/passkeys), watchlist, provider sync, friend recommendations (first use of DB transactions in this codebase — `claim`/`revoke`)

### `api`
Axum backend. Current routes (`crates/api/src/main.rs`):
- **Public movie data**: `GET /health`, `/api/gems`, `/api/acclaimed`, `/api/wildcards`, `/api/providers`, `/api/movies/{id}`
- **Auth**: `POST /api/auth/magic`, `GET /api/auth/verify`, `POST /api/auth/passkey/register/start|finish`, `POST /api/auth/passkey/authenticate/start|finish`
- **Watchlist** (JWT): `GET+POST /api/watchlist`, `GET+DELETE /api/watchlist/movie/{movie_id}`
- **User** (JWT): `GET+PUT /api/user/providers`, `GET /api/user/me`, `PATCH /api/user/username`, `GET /api/user/username/check`
- **Recommendations** (Phase 11, Ethos C1) — `crates/api/src/routes/recs.rs`: `POST /api/recs`, `GET+DELETE /api/rec/{token}` (get public, delete = revoke by sender), `POST /api/rec/{token}/claim`, `POST /api/rec/{token}/read`, `GET /api/recs/received`, `GET /api/recs/sent`, `GET /api/recs/unread_count`, `GET /api/friends`
- **Admin/misc**: `POST /api/event`, `POST /api/score`, `POST /api/admin/sync|enrich|score|providers-sync|seed`, `GET /api/admin/logs`, `GET /api/admin/status`
- `AppState` (13 fields): `db: Arc<Database>`, `tmdb_api_key: String`, `omdb_api_key: String`, `smtp_configured: bool`, `admin_busy: Arc<AtomicBool>`, `movie_cache: Arc<RwLock<MovieCache>>`, `jwt_secret: String`, `webauthn: Arc<Webauthn>`, `passkey_reg_challenges`/`passkey_auth_challenges: ChallengeStore<...>`, `magic_link_limiter`/`event_rate_limiter: Arc<Mutex<HashMap<String, Vec<Instant>>>>`, `admin_emails: HashSet<String>`
- API keys read at startup from env; `tracing::warn!` if missing (server still starts)
- Admin endpoints spawn `tokio::spawn` background tasks, return 202 immediately
- Middleware: CORS, tracing, Tower HTTP, `ServeDir` (gated on `SERVE_FRONTEND=1`)
- `services/`: `tmdb_sync.rs`, `omdb_sync.rs`, `gem_score.rs`, `provider_sync.rs` (Phase 10), `email.rs` (SMTP magic-link sending, Origin-header allowlist validation)

### `frontend`
Leptos WASM frontend. Current modules (`crates/frontend/src/`, five files — no `components.rs`):
- `main.rs` — Router, layout shell, navigation, `/r/:token` and `/recs` routes, unread-recs badge context
- `pages.rs` — `HomePage` (gem grid + pagination + filters + Phase 10 watch-filter panel), `MovieDetail`, `AcclaimedPage`, `WildcardsPage`, `AdminPage`, `AboutPage`, `SignInPage`/`VerifyPage` (auth, with `next=` redirect round-trip), `WatchlistPage`
- `api.rs` — HTTP client functions: movie data, admin ops, auth, watchlist, user/username, recommendations
- `i18n.rs` — hand-rolled `Dict` struct with parallel `EN`/`PT` consts, `{}` placeholder interpolation
- `recs.rs` (Phase 11) — `RecommendButton`, `UsernameModal`, `RecLandingPage` (`/r/:token`), `RecsPage` (`/recs` inbox + sent/revoke)
- Phase 10 (shipped): "What can I watch?" filter panel on list pages (region toggle, provider checkboxes, rentals toggle), provider badges on cards, grouped provider display + Stremio deep link on `MovieDetail`; selections persist in localStorage (`gf_watch_region`, `gf_watch_providers_us`, `gf_watch_providers_br`, `gf_watch_rentals`) and sync to the account when signed in

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

### `users`
| Column | Type |
|---|---|
| `id` | TEXT PK (UUID v4) |
| `email` | TEXT UNIQUE NOT NULL |
| `username` | TEXT UNIQUE, NULL until set |
| `created_at` | TEXT |
| `last_login` | TEXT |

### `magic_tokens`
One-time passwordless auth tokens.

| Column | Type |
|---|---|
| `token` | TEXT PK (UUID v4) |
| `user_id` | FK → users, ON DELETE CASCADE |
| `expires_at` | TEXT |
| `used_at` | TEXT, NULL until consumed |
| `created_at` | TEXT |

### `passkeys`
WebAuthn credentials registered after magic-link bootstrap.

| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `user_id` | FK → users, ON DELETE CASCADE |
| `credential_id` | TEXT UNIQUE (base64url) |
| `public_key` | TEXT (webauthn-rs serialized JSON) |
| `sign_count` | INTEGER |
| `name` | TEXT — user label, e.g. "MacBook Touch ID" |
| `created_at` | TEXT |

### `watchlist`
Replaced entirely in migration v2 (the original Phase-1 placeholder schema below no longer exists — do not build against it).

| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `user_id` | FK → users, ON DELETE CASCADE |
| `movie_id` | FK → movies, ON DELETE CASCADE |
| `state` | TEXT CHECK (`want_to_watch`, `watched`, `not_interested`) |
| `user_rating` | INTEGER, 1–10 nullable — public aggregate, only meaningful when `watched` |
| `created_at` | TEXT |
| `updated_at` | TEXT |
| `via_rec_id` | INTEGER, nullable FK → `recommendations` (Phase 11) — tags the row with the rec that brought it in, for the "de {username}" UI tag. Revocation NULLs it explicitly rather than relying on FK cascade (unverified on the pre-release `turso` crate) |

`UNIQUE(user_id, movie_id)`.

### `events`
Anonymous product-analytics events.

| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `event_type` | TEXT CHECK (`movie_view`, `search_used`, `filter_genre`, `filter_era`, `pagination`, `section_view`) |
| `movie_id` | FK → movies, ON DELETE SET NULL |
| `genre`, `era`, `section` | TEXT/INTEGER, nullable — context per event type |

### `schema_migrations`
Tracks applied migration versions (`v1`–`v8` currently). `version INTEGER PK`, `applied_at TEXT`.

### `big_hits`
| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `movie_id` | FK → movies |
| `year` | INTEGER |
| `box_office_millions` | REAL |
| `popularity_score` | REAL |

### `acclaimed`
Films with IMDb ≥ 8.0 AND (RT critic ≥ 80% OR audience-canonized: ≥ 500k IMDb votes and ≥ 20 years old). The audience branch admits critic-snubbed monuments like Forrest Gump (RT 71%). Populated by `classify_acclaimed_films()` after OMDb enrichment. Browsable separately from hidden gems.

| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `movie_id` | FK → movies UNIQUE |
| `created_at` | TEXT |

### `movie_providers` (Phase 10)
Per-movie streaming availability from the TMDB watch-providers API (JustWatch data), normalized for SQL filtering. Regions limited to `US` and `BR` initially.

| Column | Type |
|---|---|
| `movie_id` | FK → movies |
| `region` | TEXT — `US` \| `BR` |
| `provider_id` | INTEGER — TMDB provider id |
| `provider_name` | TEXT |
| `logo_path` | TEXT |
| `access` | TEXT — `flatrate` \| `free` \| `ads` \| `rent` \| `buy` |

PK `(movie_id, region, provider_id, access)`; index on `(region, provider_id, access)` for the "What can I watch?" filter join.

### `provider_sync` (Phase 10)
Per-movie fetch bookkeeping for provider data (7-day refresh TTL).

| Column | Type |
|---|---|
| `movie_id` | INTEGER PK, FK → movies |
| `fetched_at` | TEXT |
| `tmdb_link` | TEXT — TMDB watch page (deep links are not exposed by the API) |

### `user_providers` (Phase 10)
Signed-in sync of a user's selected streaming services (anonymous users use localStorage only).

| Column | Type |
|---|---|
| `user_id` | FK → users |
| `region` | TEXT |
| `provider_id` | INTEGER — TMDB provider id |

PK `(user_id, region, provider_id)`.

### `recommendations` (Phase 11 — friend recommendations, Ethos C1)
One row per "recommend this movie" action. Addressed externally only by `token` (random, uuid v4 simple) — the integer PK never appears in API responses.

| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `token` | TEXT UNIQUE |
| `sender_id` | FK → users |
| `movie_id` | FK → movies |
| `note` | TEXT, ≤140 chars (CHECK) |
| `created_at` | TEXT |

### `rec_receipts` (Phase 11)
Who received/claimed a rec. `UNIQUE(rec_id, recipient_id)` makes re-claims idempotent; a link fanning out to a group chat gets one receipt per claimer.

| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `rec_id` | FK → recommendations |
| `recipient_id` | FK → users |
| `read_at` | TEXT, NULL = unread (nav badge) |
| `created_at` | TEXT |

### `friendships` (Phase 11)
Formed only by a claimed rec in v1 (`origin='rec'`); `origin='search'` is reserved for a future username-search flow. Stored canonically (`user_a < user_b`, `CHECK`) so the pair is unique regardless of direction.

| Column | Type |
|---|---|
| `id` | INTEGER PK |
| `user_a` | FK → users |
| `user_b` | FK → users |
| `origin` | TEXT — `rec` \| `search` |
| `created_at` | TEXT |

`watchlist` also gains `via_rec_id` (nullable FK → `recommendations`, Phase 11): tags a watchlist row with the rec that brought it in for the "de {username}" UI tag. Revocation NULLs it explicitly rather than relying on FK cascade behavior (unverified on the pre-release `turso` crate).

## Hidden Gem Algorithm

```
Hard filters (return None):
  - IMDb rating outside [IMDB_GEM_MIN, IMDB_GEM_MAX] (6.5–7.9), UNLESS
    rt_critic_score >= RT_ENDORSEMENT_THRESHOLD, in which case the floor
    drops to IMDB_GEM_MIN_RT_ENDORSED — a critically-endorsed film with an
    unusually low crowd rating can still qualify
  - vote_count < 500
  - released in the current calendar year (year >= current_year)
  - rt_critic_score present AND < 65 (critics disliked it)

GemScore = weighted_sum(
  IMDb rating in sweet spot              → 0.30  (IMDB_RATING)
  Vote ratio (low = hidden)              → 0.25  (VOTE_RATIO — raised from 0.20 in Phase 6)
  Year decay (age/30).clamp(0,1)         → 0.40  ← dominant (YEAR_DECAY)
  Obscured by nearby blockbuster         → 0.05  (OBSCURED)
) × genre_boost
```

**RT is a credibility multiplier on vote_ratio, not an additive weight** (changed in Phase 6): rt≥70 → 1.0, rt∈[40,70) → linear, rt<40 → 0.0, no RT data → 0.8. High RT + low votes = critics endorsed, audience missed = true gem; low RT + low votes = audience heeded critics = routed to `wildcards`, not scored as a gem.

**Year decay** is the dominant component (0.40 weight). A 2023 film (age=3) scores ~0.10; a 1977 film (age=49) scores 1.0. This prevents recent acclaimed films from dominating despite strong RT/IMDb scores.

**RT quality gate**: Films with RT < 65% are excluded entirely regardless of other signals (routed to `wildcards` instead — see Phase 6). Films without RT data are allowed through — old films may not be on RT. Enrichment must run before scoring to ensure coverage.

**Data ingestion**: `sync_movies` runs in 4 era windows (1960-1984, 1984-1999, 1999-2012, 2012-present) × 5 pages each = ~400 candidates with balanced decade coverage.

**Genre boosts**: Foreign=1.3, Indie=1.25, Documentary=1.2, Drama=1.1, Thriller=1.05, Comedy=0.95, Horror=0.9, Romance=0.85, Action=0.8, Sci-Fi=0.75

**Seeded gems** (known greats to validate algorithm — diagnostics only, never asserted as pass/fail, per ETHOS.md):
- Sorcerer (1977), Dinner in America (2020), The Hurt Locker (2009), The Messenger (2009)

Current weights and thresholds live in `crates/shared/src/constants.rs` — treat that file as the source of truth over this doc.

## Acclaimed Films

Separate browsable category at `/api/acclaimed`. Not hidden gems — films everyone should have seen.
- **Threshold**: IMDb ≥ 8.0 AND RT critic ≥ 80%
- **Sync**: `sync_acclaimed_candidates` discovers films with TMDB vote_avg ≥ 7.5 and vote_count ≥ 10,000
- **Classification**: `classify_acclaimed_films` INSERT OR IGNORE into `acclaimed` from enriched movies

## Environment Variables

| Variable | Required | Description |
|---|---|---|
| `TMDB_API_KEY` | Yes | Movie discovery |
| `OMDB_API_KEY` | Yes | Ratings + critic scores |
| `ADMIN_TOKEN` | Production | Bearer token for `/api/admin/*`. Admin routes always require credentials — with no valid admin JWT and no `ADMIN_TOKEN` set, requests are rejected, never allowed through |
| `APP_URL` | Production | Public base URL, used in magic-link emails |
| `CORS_ORIGINS` | Production | Comma-separated allowed origins; empty = permissive (dev only) |
| `SERVE_FRONTEND` | Production | Set to `1` to serve compiled WASM from `dist/` |
| `LOG_ROTATION` | No | `never` (default), `daily`, or `hourly`; restart required |
| `RUST_LOG` | Optional | Log level, e.g. `info`, `debug` (default: `info`) |
| `DATABASE_URL` | No | Explicit SQLite path; defaults to `gem_finder.db` in working directory |
| `TURSO_DATABASE_URL` | Optional | Turso remote URL — if empty, uses local SQLite |
| `SYNC_INTERVAL_HOURS` | No | Background re-sync interval; defaults to 24 |
| `JWT_SECRET` | Auth | Signs session tokens |
| `SMTP_HOST` / `SMTP_PORT` / `SMTP_USER` / `SMTP_PASSWORD` / `SMTP_FROM` | Auth | Magic-link email delivery |
| `WEBAUTHN_ORIGIN` / `WEBAUTHN_RP_ID` | Auth | Passkey (WebAuthn) configuration |

Auth variables (`JWT_SECRET`, `SMTP_*`, `WEBAUTHN_*`) are all optional — leave unset to run without user accounts; film discovery works fully, sign-in/watchlist UI are hidden. Full descriptions and setup notes: [README's Configuration Reference](README.md#configuration-reference), the more actively maintained copy of this table.

Store API keys in `SECRETS.env` (gitignored, copy from `SECRETS.env.example`).

## Deployment

### Production: bare-metal VPS (actual current path)
`deploy/install.sh` is the single-command bootstrap: installs system deps, Caddy (auto TLS), Rust, Trunk; builds backend+frontend; creates a `gem-finder` system user; installs/starts a systemd service. `scripts/smoke-test.sh` verifies a deployment. `.github/workflows/deploy.yml` automates this on every push to `main`: builds via self-hosted runner, `rsync`s the binary + frontend dist to the VPS over SSH, swaps the binary, `systemctl restart gem-finder`. Full walkthrough: [README.md](README.md#deploy-to-vps).

This is the path CI/CD actually exercises — treat README's VPS guide as the primary deployment doc.

### Docker (alternative, self-contained)
```bash
cp SECRETS.env .env          # Linux/macOS
Copy-Item SECRETS.env .env   # Windows PowerShell

docker compose build && docker compose up
# App at http://localhost:3000
```
Docker always uses local SQLite. The named volume `gem-data` mounts to `/app` and persists `gem_finder.db` across image rebuilds. Only deleted by `docker compose down -v`. Not used by the automated deploy workflow — a manual/self-hosted option.

### Local Development (two processes)
```bash
# Terminal 1 — API
cargo run --package gem-finder-api

# Terminal 2 — Frontend (proxied to :3000)
cd crates/frontend && trunk serve
```

### Local Development (single process, production build)
```bash
# Linux/macOS
make run

# Windows
.\scripts\run.ps1
```

## Merge Rules

See `.github/CODEOWNERS` for per-crate review assignments.

**Branch strategy:** `main` ← `dev` ← feature work. Day-to-day work lands on `dev`; `main` only receives reviewed merges from `dev`.

**Requirements to merge to main:**
1. PR passes CI (build + clippy + test)
2. At least 1 review approval
3. No failing tests
4. `cargo fmt` applied

## Future Enhancements
- [x] Authentication (magic link + JWT + WebAuthn passkeys) — Phase 8
- [x] Watchlist per user — Phase 8
- [x] Rate limiting on API endpoints — Phase 9
- [x] GitHub Actions deploy workflow — Phase 9
- [x] Automated sync scheduling (`SYNC_INTERVAL_HOURS`) — Phase 9
- [ ] Multi-arch Docker image (amd64 + arm64 for Raspberry Pi)
- [ ] Structured error responses (replace ad-hoc `StatusCode` returns) — Phase 9
- [ ] API versioning (`/api/v1/...`) — Phase 9
- [ ] Health check DB connectivity probe — Phase 9
- [ ] Reply/thread support on recommendations, username-search friendships, email notifications — reserved out of scope, Phase 11