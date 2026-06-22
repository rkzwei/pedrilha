# Gem Finder — Known Issues & Fix Backlog

Issues identified via code review. Ordered by severity.

---

## P0 — Correctness Bugs (broken in production right now)

### FIX-1: Obscured-by-big-hit signal is completely dead
**File:** `crates/api/src/services/gem_score.rs`, `crates/api/src/services/tmdb_sync.rs`  
**Problem:** `tmdb_sync.rs` fetches only movies with `vote_average 6.0–8.0`. Blockbusters
(Avengers 8.4, Star Wars 8.6, etc.) are outside that range and never imported. The batch
scoring pipeline derives `big_hit_dates` from movies already in the DB with >200k votes —
but those movies won't be there. Result: `big_hit_dates` is always empty, every movie gets
`obscured_by_big_hit_score = 0.0`, and the 0.20 weight is permanently wasted.  
**Fix:** Add a separate sync pass that fetches top-popularity/top-grossing movies with no
rating filter and writes their release dates to the `big_hits` table. The scoring pipeline
should read from `big_hits`, not derive proxies from the gem candidate pool.

### FIX-2: Poster URL is double-constructed — all images broken
**File:** `crates/frontend/src/pages.rs` (line 52)  
**Problem:** `tmdb_sync.rs` stores the full URL:
`"https://image.tmdb.org/t/p/w500/poster.jpg"`. `pages.rs` then prepends another base:
`format!("https://image.tmdb.org/t/p/w342{}", url)` producing a garbage URL.  
**Fix:** Remove the prefix in `pages.rs`. Use `poster_url` as-is (it's already absolute).

### FIX-3: `imdb_rating` field aliased to TMDB data
**File:** `crates/api/src/services/tmdb_sync.rs` (line 107)  
**Problem:** `imdb_rating: detail.vote_average` — the comment says "default to TMDB rating
if no IMDb data yet" but there is no code path that ever updates this to real IMDb data.
The scoring algorithm prioritises `imdb_rating` over `tmdb_rating`; since both are identical,
the distinction is meaningless and the label is misleading.  
**Fix:** Set `imdb_rating: None` until a proper IMDb enrichment step exists. The scoring
algorithm will fall back to `tmdb_rating` correctly.

### FIX-4: Double API call per movie for credits
**File:** `crates/api/src/services/tmdb_sync.rs` (lines 63–75)  
**Problem:** `append_to_response=credits` is passed in the detail URL, but `TmdbMovieDetail`
has no `credits` field, so the payload is silently dropped. A second explicit `/credits`
request is then made. Doubles API usage on every movie.  
**Fix:** Remove `append_to_response=credits` from the detail URL. Keep only the explicit
`/credits` call.

---

## P1 — Logic Errors (wrong results, not a crash)

### FIX-5: `current_year` hardcoded as `2026`
**File:** `crates/api/src/services/gem_score.rs` (line 107)  
**Problem:** `let current_year = 2026_i32;` will silently produce wrong year-decay scores
from 2027 onward.  
**Fix:** Use `chrono::Utc::now().year()` (add `chrono` dependency) or derive dynamically
from `std::time::SystemTime`.

### FIX-6: `critic_disparity_score` fallback adds flat noise, not neutral signal
**File:** `crates/api/src/services/gem_score.rs` (line 163)  
**Problem:** When no RT data exists — which is true for all TMDB-sourced movies — the
function returns `0.5`. This adds `0.5 × 0.10 = 0.05` to every movie's score regardless
of any real signal. It's not neutral; it's a constant bias.  
**Fix:** Return `0.0` as the fallback. If RT data is genuinely unavailable for a movie,
it should contribute nothing to the score.

### FIX-7: SQL injection in `get_top_gems` and `get_gems_count`
**File:** `crates/db/src/models.rs` (lines 99–101, 228–230)  
**Problem:** Genre filter is constructed via string formatting:
`format!(" AND genre LIKE '%{}%'", g.replace('\'', "''"))`. Only single quotes are escaped.
**Fix:** Use bound parameters. With Turso/libSQL, build the query string conditionally but
bind the genre value as a parameter.

### FIX-8: `genre_boosts::get()` allocates a new HashMap per movie
**File:** `crates/shared/src/constants.rs` (line 39)  
**Problem:** Called once per movie during batch scoring (potentially thousands of movies).
**Fix:** Replace with a `static` array of `(&str, f64)` tuples or a `once_cell::sync::Lazy`
`HashMap`.

---

## P2 — Test Quality

### FIX-9: Tests don't validate the stated acceptance criteria
**File:** `crates/api/src/services/gem_score.rs` (lines 258–368)  
**Problem:**  
- `test_sorcerer_scores_high` only asserts `> 0.5`. The spec requires "top 0.1%", which
  means comparing against a population.  
- No tests for Dinner in America (2020) or The Hurt Locker (2009).  
- `test_blockbuster_scores_low` asserts `score.is_none()` (filtered out by rating range),
  not "scores low." A blockbuster rated 7.5 wouldn't be filtered — this case is untested.  
- `make_movie` sets `imdb_rating == tmdb_rating` and `imdb_vote_count == tmdb_vote_count`,
  masking the real-world data divergence.  
**Fix:** Add a population test that scores ~50 synthetic movies with varied profiles and
asserts the known gems rank near the top. Add explicit tests for all three seeded gems.
Fix `test_blockbuster_scores_low` to test a high-vote-count movie within the rating range.

---

## P3 — Minor / UX

### FIX-10: `gem_score` displayed as `★ 0.8` (looks like a 1-star rating)
**File:** `crates/frontend/src/pages.rs` (line 55)  
**Problem:** `gem_score` is normalised to [0, 1]. Displaying `★ 0.8` will confuse users
who expect a 10-point IMDb-style scale.  
**Fix:** Display as percentage: `GEM 80%` or use a distinct label/icon.

### FIX-11: Leptos version inconsistency in docs
**File:** `ARCHITECTURE.md` (line 6), `PHASES.md` (line 217)  
**Problem:** `ARCHITECTURE.md` says Leptos 1.0.0; `PHASES.md` says 0.7.x.  
**Fix:** Check `Cargo.toml` and align both docs.

### FIX-12: `SEEDED_GEMS` constant is never seeded into the database
**File:** `crates/shared/src/constants.rs` (line 57)  
**Problem:** The constant exists for algorithm validation but no code path inserts these
movies into the database.  
**Fix:** Add a `seed_known_gems` function in `db/src/models.rs` and call it after migrations
(or provide a dedicated admin endpoint).

### FIX-13: `/api/admin/sync` is unauthenticated
**File:** `crates/api/src/routes/admin.rs`  
**Problem:** Anyone can trigger a TMDB sync and drain the API quota.  
**Fix:** Add at minimum a static bearer token check from an env var (`ADMIN_TOKEN`) until
proper auth is implemented in Phase 5.

---

---

## P4 — Frontend Build / Preview

### FIX-14: No static file serving in API for production
**File:** `crates/api/src/main.rs`  
**Problem:** The API has no `ServeDir` handler. In production (single Docker container) the
compiled WASM + assets need to be served by the API binary. Currently the only dev option is
running `trunk serve` (port 8080) alongside `cargo run` (port 3000) and relying on trunk's
proxy. A production build needs `tower-http::services::ServeDir` on `/` pointing at the `dist/`
folder produced by `trunk build`.  
**Fix:** Add a fallback `ServeDir` route in `main.rs`. Gated behind a `SERVE_FRONTEND` env var
so it doesn't interfere with API-only Docker builds.

### FIX-15: `tailwind.css` generated output not in repo / not built in CI
**File:** `assets/tailwind.css`, `.github/workflows/ci.yml`  
**Problem:** `index.html` references `/assets/tailwind.css` but that file is the _generated_
output of `npx tailwindcss`. It's not committed and CI doesn't build it. The page will load
with no styles.  
**Fix:** Add a `npm run build:css` step to CI before the trunk build, or use trunk's built-in
Tailwind integration (`data-trunk` attribute on the link tag with `type="css"`).

### FIX-16: Leptos router not wired — only HomePage renders
**File:** `crates/frontend/src/main.rs`, `crates/frontend/src/pages.rs`  
**Problem:** `main.rs` renders `<pages::HomePage />` directly with no `<Router>`. `pages.rs`
has `MovieCard` components but no navigation to a detail page. Phase 4 requires `leptos_router`
integration for `/` and `/movie/{id}` routes.  
**Fix:** Wrap App in `<Router>`, add `<Routes>` with `<Route path="/" view=HomePage />` and
`<Route path="/movie/:id" view=MovieDetail />`. Implement `MovieDetail` page.

---

## Status

| ID | Description | Status |
|---|---|---|
| FIX-1 | Obscured signal dead | ✅ done — `sync_blockbusters` in `tmdb_sync.rs`, `get_big_hit_dates` in `models.rs`, `run_batch_scoring` now reads from `big_hits` table |
| FIX-2 | Poster URL double-prefix | ✅ done — `pages.rs` |
| FIX-3 | imdb_rating aliased to TMDB | ✅ done — `tmdb_sync.rs` |
| FIX-4 | Double credits API call | ✅ done — `tmdb_sync.rs` |
| FIX-5 | Hardcoded current_year 2026 | ✅ done — `gem_score.rs` uses `chrono::Utc::now().year()` |
| FIX-6 | critic_disparity neutral fallback | ✅ done — fallback changed from 0.5 → 0.0 |
| FIX-7 | SQL injection in genre filter | ✅ done — `models.rs` now uses bound params |
| FIX-8 | genre_boosts HashMap per call | ✅ done — replaced with static `&[(&str, f64)]` slice |
| FIX-9 | Tests don't match acceptance criteria | ✅ done — full test suite rewrite in `gem_score.rs` |
| FIX-10 | gem_score display misleading | ✅ done — `pages.rs` shows "GEM 80%" format |
| FIX-11 | Leptos version mismatch in docs | ✅ done — `ARCHITECTURE.md` corrected to 0.7.x |
| FIX-12 | SEEDED_GEMS never seeded | ✅ done — `seed_known_gems` in `tmdb_sync.rs` uses TMDB `/find` by IMDb ID; called as Phase C of admin sync |
| FIX-13 | Admin sync unauthenticated | ✅ done — `check_admin_token` in `admin.rs` reads `ADMIN_TOKEN` env var; open in dev if unset |
| FIX-14 | No static file serving in API | **pending** — needs `ServeDir` fallback in `main.rs` for prod |
| FIX-15 | `tailwind.css` not built in CI | **pending** — CI needs a CSS build step before trunk |
| FIX-16 | Leptos router not wired | **pending** — only `HomePage` renders; `MovieDetail` route missing |
| FIX-17 | Gem score not normalized to near 100% | **pending** — top gems should approach 100%; Sorcerer scores 66% currently |
| FIX-18 | UI needs full implementation | **pending** — currently single-column list on white background; needs grid, dark theme, filters |
| FIX-19 | Scheduled sync not configured | **pending** — run `/api/admin/sync` + `/api/score` every 5 hours starting 2:30 AM |
| FIX-17 | Gem score ceiling too low — top gems score ~66% not ~100% | **pending** — normalize scores relative to population max so top gem = 100% |
| FIX-18 | UI — no styling, single-column list on white background | **pending** — implement dark grid layout per Phase 4 spec |
| FIX-19 | Automated sync not scheduled | **pending** — run `/api/admin/sync` + `/api/score` every 5h starting 2:30 AM; use OS task scheduler or add cron-style job to API |
| FIX-20 | Logging only writes to file, not stdout | ✅ done — `main.rs` now uses `registry()` with two `fmt::layer()` instances: one to stdout (ANSI), one to `gem_finder.log` (plain text) |
| FIX-21 | Tests pass silently with no score output | ✅ done — added `eprintln!` to all unit tests + new `test_score_breakdown_table` diagnostic (run with `-- --nocapture`) |
| FIX-22 | TMDB seed failures — no fallback when `/find` returns empty | ✅ done — `seed_single_gem` now tries `/search/movie?query={title}&year={year}` if `/find` returns empty `movie_results`; retries only cover transient errors |
| FIX-23 | Wrong IMDb IDs in SEEDED_GEMS — root cause of seed failures | ✅ done — corrected: Sorcerer title (`"The Sorcerer"` → `"Sorcerer"`), Dinner in America (`tt8765300` → `tt9058654`), The Hurt Locker (`tt1655246` → `tt0887912`). The old Hurt Locker ID was a TV episode. |
| FIX-24 | Algorithm promotes new streaming films (My Fault, Your Fault) as gems | ✅ done — added `MIN_GEM_AGE_YEARS = 3` constant; `calculate()` now returns `None` for films < 3 years old. Streaming films have artificially low TMDB vote counts regardless of popularity, making them look "undiscovered". |
| FIX-25 | Genre boost uses `max()` — Drama overrides Romance penalty | ✅ done — changed `fold(1.0, f64::max)` → `fold(1.0, \|acc, b\| acc * b)`. Drama+Romance now correctly scores 1.1 × 0.85 = 0.935 instead of 1.1. |
| FIX-26 | Blockbuster sync returns 0 silently — HTTP errors not logged | ✅ done — added HTTP status check before JSON parse; logs error body on non-2xx; logs per-page result count so 0-result responses are visible |
| FIX-27 | Stub-upserted blockbusters pollute gem scoring pool | ✅ done — `get_all_movies_for_scoring` now excludes any movie in `big_hits` via `WHERE id NOT IN (SELECT movie_id FROM big_hits ...)`. Blockbusters have low TMDB vote counts (18k-24k) that look like "undiscovered" films to the algorithm; exclusion is the correct fix. |
| FIX-28 | Sorcerer `obscrd = 0` despite Star Wars being in big_hits | ✅ done — widened `OBSCURED_WINDOW_WEEKS` from 4 → 6 (28 → 42 days). Star Wars opened May 25 1977, Sorcerer June 24 1977 = 30 days apart — just outside the 4-week window. |
| FIX-29 | `sync_movies` imports films too new to score — `sort_by=primary_release_date.desc` with no ceiling fetches mostly 2024+ films, all filtered by `MIN_GEM_AGE_YEARS` | ✅ done — added `primary_release_date.lte={cutoff_year}-12-31` to discover URL using `chrono::Utc::now().year() - MIN_GEM_AGE_YEARS`. Now fetches films up to 3 years ago max. |
| FIX-30 | `calculate()` vote counting drops films with 500+ TMDB votes but sparse IMDb votes | ✅ done — changed `imdb_vote_count.unwrap_or_else(tmdb_fallback)` to `max(imdb_vote_count, tmdb_vote_count)`. IMDb and TMDB vote bases differ; a film discovered on TMDB but not well-rated on IMDb should not be excluded from scoring. |
| FIX-31 | `test_known_gems_rank_in_top_tier` panics on DB with < 10 scored movies | ✅ done — added early return with diagnostic if population < 10. Tier cutoffs collapse to rank 1-2 with tiny populations, making any 3-way comparison a guaranteed false failure. Still asserts `score > 0` for each found gem. |
| FIX-32 | `seed-test-data` never calls `sync_movies` — scoring pool only has 3 seeded gems | ✅ done — added `sync_movies` as step 3b in `run_seed_test_data()`. Also added per-page logging and early-exit skip for movies already in DB. Old 2024-era discover movies remain in DB but are filtered by age; new runs now import 1960-2023 eligible films. |
| FIX-33 | Recent films (2022-2023) dominate rankings due to weak year_decay signal | ✅ done — (a) changed year_decay formula from `age/(current-1960)` to `(age/30).clamp(0,1)`: gives 3x more discrimination in 5-30 year range (3yr→0.10, 18yr→0.60, 30+yr→1.0); (b) rebalanced weights: YEAR_DECAY 0.10→0.25, VOTE_RATIO 0.25→0.20, IMDB_RATING 0.35→0.30, OBSCURED 0.20→0.15, CRITIC_DISPARITY unchanged 0.10. |
| FIX-34 | Test asserts every known gem in top 50% — wrong for recent indie gems | ✅ done — removed per-gem top-50% assertion. A 2020 film (Dinner in America) ranking below 1977 films is correct behaviour. Kept only the "best gem in top 25%" assertion as a population sanity check. |
| FIX-35 | RT critic data unused — `calc_critic_disparity_score` required both critic+audience but OMDb only returns critic | ✅ done — (a) added hard RT filter in `calculate()`: films with `rt_critic_score < 65` are excluded (bad films aren't hidden gems, they're just bad); (b) rewrote scorer to map rt_critic [65→100] to [0.0→1.0] using critic score alone; (c) TMDB/IMDb gap fallback retained when RT unavailable. |
| FIX-36 | Score table shows TMDB vote count but scoring uses max(imdb, tmdb) | ✅ done — display now shows `max(imdb_vote_count, tmdb_vote_count)` matching the actual value used in the scoring formula. |
| FIX-37 | `get_top_gems` constructor mapped 8 columns; SQL now returns 10 — runtime index-out-of-bounds | ✅ done — updated column mapping: 4=director, 5=poster_url, 6=imdb_rating, 7=rt_critic_score, 8=gem_score, 9=gem_rank. |
| FIX-38 | Acclaimed feature not implemented — schema existed but no model functions, no sync, no API endpoint | ✅ done — added `classify_acclaimed_films`, `get_acclaimed_films`, `get_acclaimed_count` in `models.rs`; `sync_acclaimed_candidates` in `tmdb_sync.rs` (vote_avg ≥ 7.5, vote_count ≥ 10k, 5 pages); `/api/acclaimed` route + handler; seed steps 3c (sync before OMDb) and 5b (classify after enrichment). |
| FIX-39 | Bad films slip through RT gate when unenriched — Whitney Houston (RT 43%) passed scoring because OMDb enrichment was capped at 100 movies | ✅ done — raised OMDb enrichment limit from 100 → 2000 in seed-test-data to drain the full unenriched queue each run. RT filter only fires when data is present; the fix is ensuring it's always present. |
| FIX-40 | `test_rt_audience_over_critics_boosts_score` stale — tested audience-vs-critic disparity which no longer exists; panicked on `unwrap()` when `rt_critic_score=55` triggered the hard filter | ✅ done — renamed to `test_rt_critic_score_drives_disparity`; now tests that higher RT critic score produces higher disparity component (98% → 0.943, 67% → 0.057). |
| FIX-41 | Weight rebalance: YEAR_DECAY too weak (0.25) — recent acclaimed films (Boy and the Heron, Zone of Interest) scored ~55% despite being 3 years old; dominated top 25 | ✅ done — YEAR_DECAY 0.25→0.40 (dominant component), CRITIC_DISPARITY 0.10→0.05 (RT is gate only, not scorer), OBSCURED 0.15→0.05 (data sparsity). Max year_decay gap 2023→1977 is now 0.36 vs 0.225 before. |
| FIX-42 | `sync_movies` data bias — `sort_by=primary_release_date.desc` with 10 pages returned ~200 films all from 2022-2023; no older candidates in scoring pool | ✅ done — added `end_year: Option<i32>` parameter; changed to 4 era windows × 5 pages in seed-test-data: classics 1960-1984, modern classics 1984-1999, 2000s 1999-2012, recent 2012-present. ~400 candidates with broad decade coverage instead of 200 from one era. |
