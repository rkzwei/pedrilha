# Gem Finder — Compiled Agent Memory
<!--
  This file is a flattened export of all persistent memory files for this project.
  A future agent can split this back into individual memory files by section.
  Each section maps to one memory file (type noted in the header).
  Generated: 2026-06-24
-->

---

## [PROJECT] Gem Finder — Codebase & Status

**Stack:** Rust workspace — `shared`, `db`, `api`, `frontend` crates.
Axum 0.8 + Leptos 0.7 CSR + Turso/libSQL (SQLite-compatible, intentional lock-in).

**Repository layout:**
```
crates/
  shared/   — types, constants, id_encode (mv+base36 movie IDs)
  db/       — migrations, models (Turso/libSQL)
  api/      — Axum server, routes, services (tmdb_sync, omdb_sync, gem_score, email)
  frontend/ — Leptos 0.7 CSR (main.rs, pages.rs, api.rs, components.rs)
```

**Build environment (Windows):**
- Cargo + rustup, wasm32 target for frontend
- OpenSSL at `C:\Program Files\OpenSSL-Win64`; libs at `lib\VC\x64\MD\`
- For `cargo check` on Windows, must set:
  ```
  OPENSSL_DIR=C:\Program Files\OpenSSL-Win64
  OPENSSL_LIB_DIR=C:\Program Files\OpenSSL-Win64\lib\VC\x64\MD
  OPENSSL_INCLUDE_DIR=C:\Program Files\OpenSSL-Win64\include
  ```
- `run_check.bat` in repo root sets these and runs `cargo check -p gem-finder-api -p gem-finder-db`
- `trunk` for WASM frontend build; `SERVE_FRONTEND=1` env var serves dist/ from Axum in production

**Algorithm (current):**
- IMDb sweet spot: 6.5–7.9 (films too low = bad, too high = not hidden)
- Weights: IMDB=0.30, VOTE_RATIO=0.25, YEAR_DECAY=0.40, OBSCURED=0.05
- RT is a **credibility multiplier** on vote_ratio (not additive): rt≥70→1.0, rt∈[40,70)→linear, rt<40→0.0, rt=None→0.8
- No hard RT gate. Films with rt<50 are classified **wildcards** post-scoring
- `get_top_gems` filters `rt_critic_score IS NULL OR rt_critic_score >= 50`

**Three movie categories (all have `/api/...`, frontend page, cache, filters):**
- **Gems** `/` — hidden gems (IMDb 6.5–7.9, rt≥50 or null)
- **Acclaimed** `/acclaimed` — IMDb ≥ 8.0, RT ≥ 80% (constants: ACCLAIMED_MIN_IMDB, ACCLAIMED_MIN_RT)
- **Wildcards** `/wildcards` — gem_score IS NOT NULL, rt_critic_score < 50

**In-memory cache:** 5-minute TTL, invalidated on admin sync/score/seed. All three lists cached separately.

**Auth (Phase 8, complete):**
- Magic link via SMTP (`lettre` crate, `tokio1-native-tls` feature)
- JWT signed with `JWT_SECRET` env var
- WebAuthn passkey support (via `webauthn-rs` v0.5)
- `users`, `magic_tokens`, `watchlist` tables in DB
- `/signin` page (dedicated, not modal) → POST `/api/auth/magic` → email → `/auth/verify?token=` → JWT stored in localStorage

**Watchlist:**
- States: `WantToWatch | Watched | NotInterested` + optional `user_rating` (1–10)
- User ratings are PUBLIC — shown as aggregate on movie cards
- Routes: `GET/POST /api/watchlist`, `GET/DELETE /api/watchlist/movie/{id}`

**Scheduled sync:** `SYNC_INTERVAL_HOURS` env var (default 24). Background tokio task: sync era windows → blockbusters → acclaimed candidates → OMDb enrich → score → classify acclaimed → classify wildcards → invalidate cache.

**Admin status endpoint:** `GET /api/admin/status` — no auth, returns `{smtp_configured, tmdb_configured, omdb_configured}`. Frontend hides SIGN IN nav when `smtp_configured=false` and shows warning banners in admin panel.

**Pending work (as of 2026-06-24):**
- #25: Fix Docker build (trunk "root package not found")
- #57: Add Foreign and Indie genre categories
- #58: Fix movie list text rendering white-on-white (investigate)
- #59: Fix Docker image — outdated, missing admin panel features
- #60: Ensure all genre tags shown on movie detail + search pages
- #61: Add category filters to filter sidebar (Acclaimed, Wildcards sections)
- #62: Persist filter state and page position in URL query params (done in this session — URL already has `?page=&genres=&year=&q=`)

**Security constraints (non-negotiable):**
- `SECRETS.env` contains `OMDB_API_KEY`, `TMDB_API_KEY`, `SMTP_*`, `JWT_SECRET` — must NEVER be logged, printed, or committed
- Already in `.gitignore` and `.dockerignore`
- Never commit until change is tested and confirmed working
- No push — only commit

---

## [FEEDBACK] Algorithm Design Decisions

**RT as multiplier, not gate (current).**
RT < 40 kills the score (multiplier = 0.0). Films with RT 40–70 get partial credit. Films RT ≥ 70 are unpenalised. Films with no RT data get 0.8 multiplier (benefit of doubt for old/foreign films).

Prior: RT was a 5% additive weight. Changed to multiplier because RT rewards quality, not obscurity. A 97% RT film with 100k votes is not a hidden gem.

**Never increase RT weight past 0.05 as an additive signal.** The 5% cap is correct: RT must not dominate, otherwise recent acclaimed films flood the gem list.

**Year decay is the dominant lever (YEAR_DECAY=0.40).** If 2023 films dominate, investigate data (era-window sync coverage) before tuning weights.

**MIN_GEM_AGE_YEARS = 3, not 5.** User explicitly: "Teacher's Lounge was a good hit." Recent foreign/indie Oscar-tier films are valid gems. Age is not a quality filter.

**Vote count stays as a weight.** Same-age films with different vote counts are differently hidden. Vote count is a real signal.

**No year-based RT gate.** Never gate on year + RT presence combination. Old films (pre-2000) may lack RT data legitimately. Fix: raise enrichment limit, not year cutoffs.

---

## [FEEDBACK] Commit Policy

**Never commit until the change has been successfully tested.**

Why: User was burned by committing broken code (bad Dockerfile required revert). Committing untested changes wastes commit history and creates revert churn.

How to apply: For any non-trivial change — especially builds, Docker, CI, deployment scripts, or multi-file refactors — run the build/test first, verify it passes, then commit. Stage files and prepare the commit message in advance, but hold the actual `git commit` until success is confirmed. No push — only commit (user pushes manually after review).

---

## [FEEDBACK] Vendor Lock-in Policy

**No hardcoded external service calls. Everything gets env vars; data sources get a trait.**

User wants the project to remain portable (potentially OSS in future). Any hardcoded service URL or credential is a liability.

**Intentional exception:** Turso/libSQL — accepted. User explicitly trusts Turso's concurrency model.

**All other external services must be env-configurable:**
- `TMDB_API_KEY`, `OMDB_API_KEY` — in SECRETS.env ✓
- `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASSWORD`, `SMTP_FROM`, `APP_URL` — in SECRETS.env ✓
- `JWT_SECRET`, `WEBAUTHN_RP_ID`, `WEBAUTHN_ORIGIN` — in SECRETS.env ✓

**Planned:** `MovieDataSource` trait to abstract TMDB/OMDb behind a swappable interface.

**Current external HTTP dependencies:**
- TMDB API — movie discovery and metadata
- OMDb API — IMDb ratings, RT scores
- Google Fonts CDN — Bebas Neue + Barlow (in index.html; self-hostable)

---

## [FEEDBACK] Missing Dependencies Policy

**When a required tool or system dep is missing, ask the user to install it. Never silently stub, degrade, or work around it.**

Why: Stubs create hidden debt and mislead about the state of the codebase. The user would rather spend 5 minutes installing a dep.

How to apply:
- If a build fails due to missing system dep, stop and ask: "X is missing — can you install it?"
- Use the user's existing package manager (Chocolatey, winget, brew) — don't link to unofficial/third-party download sites
- `choco` and `winget` are both available on this machine

---

## [FEEDBACK] Product Philosophy

**User desire drives all product decisions. Technology is in service of the desired experience — never the reverse.**

Explicit user instruction: "think of this webapp through the lens of Steve Jobs — user desire first, technology later."

How to apply:
- Start every feature with "what does the user want to feel/do?" not "what does our data model support?"
- Do not let DB schema constraints dictate filters or categories. If a filter is desirable, build the data pipeline to support it.
- Do not propose technically-convenient shortcuts that produce a worse UX
- Categories (Kids, International, Cult Classics) should be designed around how a user thinks about films, not what DB fields exist
- When in doubt: does this make finding a great film easier? If yes, build it. If no, don't.

---

## [FEEDBACK] Code Quality & Anti-Patterns

**No hardcoded tests.** Tests must work on real data, not magic numbers tuned for a specific dataset. User is critical of tests that pass on current data but will silently break on a fresh database.

**No `disabled="false"` on HTML buttons.** Use Leptos `prop:disabled=move || expr` — HTML boolean attributes work by presence, not value. `disabled="false"` is still disabled.

**Always-render pattern for conditional UI.** For modals and panels that should preserve state on hide/show, render always and toggle via `style:display=move || if open { "" } else { "none" }`. Do NOT use `{move || if open { view!{...} } else { view!{<div/>} }}` — mount/unmount loses form state.

**Leptos 0.7 reactivity rules:**
- `RwSignal<T>` and `Signal<T>` are `Copy` — closures capturing only Copy types are Copy
- Use `Effect::new(move |_| { if is_open.get() { reset state... } })` to reset form state on reopen
- `Signal::derive(move || expr)` to convert a closure into a `Signal<T>`

**Filter URL format:** `?genres=Action,Drama` (comma-separated multi-select). Backend splits by comma and uses OR logic. Never treat the whole string as a single genre query.

**CSS custom properties / Tailwind sc- tokens:** All colors use `--sc-*` CSS variables mapped to Tailwind `sc-*` utility classes. When a new Tailwind class is added at runtime and isn't in the generated CSS (because Tailwind didn't see it at build time), use inline `style="..."` with the CSS variable as fallback: `style="background-color: var(--sc-panel, #1c1917)"`.

**API base URL:** `#[cfg(debug_assertions)] const API_BASE = "http://localhost:3000"` / `#[cfg(not(debug_assertions))] const API_BASE = ""`. Debug builds hit the separate Axum server; release builds use same-origin relative URLs.

---

## [PROJECT] Phase 8 Architecture Decisions

**Movie URL Identity:** `mv` prefix + base36 integer, e.g. `/movie/mv1k`. No DB migration — router strips prefix, underlying integer PK unchanged. Chosen for IDOR protection without schema cost, mirrors IMDb's `tt` pattern.

**Auth:** Magic link via Hostinger SMTP. `lettre` crate, port 465 (SMTPS) or 587 (STARTTLS). `JWT_SECRET` env var. `magic_tokens` table with 15-minute expiry, one-time-use.

**Watchlist scope for v1:** `WantToWatch | Watched | NotInterested` + `user_rating (INT 1–10, nullable)`. User ratings are PUBLIC — aggregate shown to build community dataset for future scoring.

**CSS Variables First:** CSS custom properties refactored before any Phase 8 feature code. All hardcoded hex values moved to `:root` vars, all Tailwind arbitrary values replaced with `sc-*` tokens. "Prioritize the least amount of debt in EVERY step."

---

## [FEEDBACK] UI/UX Decisions

**Sign-in must be a dedicated page `/signin`**, not a modal. Modal appeared at bottom of page and was unintuitive. The dedicated page has both sign-in and register copy ("New here? Your account is created automatically.").

**Filters must not displace the movie grid.** Use a floating overlay panel: transparent backdrop (`fixed inset-0 z-20`, `background: rgba(0,0,0,0.45)`) + absolute panel (`z-30`, `background-color: var(--sc-panel, #1c1917)`). Genres are multi-select (OR logic, panel stays open); Era is single-select (panel closes on selection).

**Active filter chips** shown below the search input row, each dismissible independently.

**Filter panel backdrop must have a non-zero background** to block pointer events visually. Pure transparent backdrop with `z-20` blocks pointer events but is visually confusing. Use `rgba(0,0,0,0.45)`.

---

## [USER] Profile

Technically sharp. Identifies root causes quickly (distinguished algorithm vs data problems). Communicates concisely and expects the same — no preambles, no recaps.

Asks for opinions when genuinely interested. Engages with technical reasoning. Does not want hardcoded tests or magic numbers.

Wants `FIXES.md`, `ARCHITECTURE.md`, and `PHASES.md` kept current — explicitly requested end-of-session doc updates.

**Working environment:** Windows 11, VS Code, Git, Rust/cargo, Node/npm, Docker. Email: `sec@krono.group`. OpenSSL at `C:\Program Files\OpenSSL-Win64`.

**Security non-negotiable:** SECRETS.env contains API keys — must never be logged, printed, or committed.
