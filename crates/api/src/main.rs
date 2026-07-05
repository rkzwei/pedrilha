use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
mod middleware;
mod routes;
mod services;
use gem_finder_db::{migrations, models, Database};
use gem_finder_shared::types::{
    HealthResponse, Movie, MovieProvider, MovieSummary, PaginatedResponse, ProviderInfo,
    WatchBadge, WatchTier,
};
use routes::auth::ChallengeStore;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use webauthn_rs::prelude::*;

/// How long list responses are served from the in-memory cache before a DB refresh.
/// Lists only change when the admin runs a score/sync job, so 5 minutes is conservative.
const CACHE_TTL: Duration = Duration::from_secs(300);

/// A single cached value with an expiry timestamp.
struct Cached<T> {
    data: T,
    expires: Instant,
}

impl<T> Cached<T> {
    fn new(data: T) -> Self {
        Self {
            data,
            expires: Instant::now() + CACHE_TTL,
        }
    }
    fn is_valid(&self) -> bool {
        Instant::now() < self.expires
    }
}

/// In-memory TTL cache for the three public movie lists.
///
/// Each list changes only when the admin runs a scoring or sync job. Caching
/// the full sorted list and applying user filters + pagination in memory means
/// the DB is hit at most once per CACHE_TTL period regardless of concurrency.
#[derive(Default)]
pub(crate) struct MovieCache {
    pub gems: Option<Cached<Vec<MovieSummary>>>,
    pub acclaimed: Option<Cached<Vec<MovieSummary>>>,
    pub wildcards: Option<Cached<Vec<MovieSummary>>>,
    /// Provider rows keyed by movie_id, per region — shared by all three lists
    /// for in-memory watch filtering. Loaded lazily on first filtered request.
    pub providers_us: Option<Cached<HashMap<i64, Vec<MovieProvider>>>>,
    pub providers_br: Option<Cached<HashMap<i64, Vec<MovieProvider>>>>,
}

impl MovieCache {
    /// Drop all cached lists. The next request for each will trigger a DB fetch.
    pub fn invalidate(&mut self) {
        self.gems = None;
        self.acclaimed = None;
        self.wildcards = None;
        self.providers_us = None;
        self.providers_br = None;
    }
}

/// Application state shared across all handlers.
#[derive(Clone)]
struct AppState {
    db: Arc<Database>,
    tmdb_api_key: String,
    omdb_api_key: String,
    /// True when SMTP_HOST and SMTP_USER are present at startup.
    /// Used by the status endpoint so the frontend can hide sign-in if email is unconfigured.
    smtp_configured: bool,
    /// Prevents concurrent admin operations (sync, enrich, score, seed).
    admin_busy: Arc<AtomicBool>,
    /// In-memory TTL cache for the three public movie list endpoints.
    movie_cache: Arc<RwLock<MovieCache>>,
    /// Secret used to sign and verify JWTs.
    jwt_secret: String,
    /// WebAuthn relying-party instance — stateless, cheap to clone.
    webauthn: Arc<Webauthn>,
    /// Pending WebAuthn passkey registration challenges keyed by user_id.
    passkey_reg_challenges: ChallengeStore<PasskeyRegistration>,
    /// Pending WebAuthn passkey authentication challenges keyed by session key.
    passkey_auth_challenges: ChallengeStore<PasskeyAuthentication>,
    /// Rate limiter for magic link requests: email -> Vec<Instant> of recent sends.
    magic_link_limiter: Arc<Mutex<HashMap<String, Vec<std::time::Instant>>>>,
    /// Rate limiter for anonymous event tracking: session_hash -> Vec<Instant> of recent events.
    event_rate_limiter: Arc<Mutex<HashMap<String, Vec<std::time::Instant>>>>,
    /// Set of emails that receive is_admin: true in their JWT.
    admin_emails: std::collections::HashSet<String>,
}

#[derive(Deserialize)]
struct GemsQuery {
    page: Option<i32>,
    per_page: Option<i32>,
    min_year: Option<i32>,
    genres: Option<String>,
    q: Option<String>,
    sort: Option<String>,
    sort_dir: Option<String>,
    /// Watch filter (Phase 10): region ('US'|'BR'), csv of selected TMDB provider
    /// ids, and whether to include rentals.
    region: Option<String>,
    providers: Option<String>,
    rentals: Option<String>,
}

#[derive(Deserialize)]
struct AclaimedQuery {
    page: Option<i32>,
    per_page: Option<i32>,
    min_year: Option<i32>,
    genres: Option<String>,
    q: Option<String>,
    sort: Option<String>,
    sort_dir: Option<String>,
    /// Watch filter (Phase 10): region ('US'|'BR'), csv of selected TMDB provider
    /// ids, and whether to include rentals.
    region: Option<String>,
    providers: Option<String>,
    rentals: Option<String>,
}

#[derive(Deserialize)]
struct WildcardsQuery {
    page: Option<i32>,
    per_page: Option<i32>,
    min_year: Option<i32>,
    genres: Option<String>,
    q: Option<String>,
    sort: Option<String>,
    sort_dir: Option<String>,
    /// Watch filter (Phase 10): region ('US'|'BR'), csv of selected TMDB provider
    /// ids, and whether to include rentals.
    region: Option<String>,
    providers: Option<String>,
    rentals: Option<String>,
}

#[derive(Deserialize)]
struct ProvidersQuery {
    region: Option<String>,
}

/// Sort a filtered movie slice in place.
///
/// `field`: "score" | "rating" | "rt" | "year" | "title"
/// `dir`:   "asc" | "desc"
/// NULLs sort last regardless of direction.
fn apply_sort(items: &mut Vec<&MovieSummary>, field: &str, dir: &str) {
    let desc = dir != "asc";
    match field {
        "rating" => items.sort_by(|a, b| {
            let av = a.imdb_rating.unwrap_or(f64::NEG_INFINITY);
            let bv = b.imdb_rating.unwrap_or(f64::NEG_INFINITY);
            let o = av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal);
            if desc {
                o.reverse()
            } else {
                o
            }
        }),
        "rt" => items.sort_by(|a, b| {
            let av = a.rt_critic_score.unwrap_or(-1);
            let bv = b.rt_critic_score.unwrap_or(-1);
            let o = av.cmp(&bv);
            if desc {
                o.reverse()
            } else {
                o
            }
        }),
        "year" => items.sort_by(|a, b| {
            let av = a.year.unwrap_or(0);
            let bv = b.year.unwrap_or(0);
            let o = av.cmp(&bv);
            if desc {
                o.reverse()
            } else {
                o
            }
        }),
        "title" => items.sort_by(|a, b| {
            let o = a.title.cmp(&b.title);
            if desc {
                o.reverse()
            } else {
                o
            }
        }),
        _ => items.sort_by(|a, b| {
            // "score" — gem_score DESC is already the cache order; re-applying is a stable no-op.
            let av = a.gem_score.unwrap_or(f64::NEG_INFINITY);
            let bv = b.gem_score.unwrap_or(f64::NEG_INFINITY);
            let o = av.partial_cmp(&bv).unwrap_or(std::cmp::Ordering::Equal);
            if desc {
                o.reverse()
            } else {
                o
            }
        }),
    }
}

// ── Phase 10: watch-provider filter helpers ──────────────────────────────────

/// Normalise a region param to one of the two supported codes.
fn normalize_region(region: &str) -> &'static str {
    if region.eq_ignore_ascii_case("BR") {
        "BR"
    } else {
        "US"
    }
}

/// Parse a csv of TMDB provider ids into a set.
fn parse_provider_ids(csv: &Option<String>) -> HashSet<i32> {
    csv.as_deref()
        .unwrap_or("")
        .split(',')
        .filter_map(|s| s.trim().parse::<i32>().ok())
        .collect()
}

/// Best watch badge for a movie under the active filter, or `None` if the movie
/// is not watchable under it. Match rule: an included-tier offering on a selected
/// provider (preferred), else — if rentals are enabled — any rent/buy offering.
fn watch_badge_for(
    rows: &[MovieProvider],
    selected: &HashSet<i32>,
    rentals: bool,
) -> Option<WatchBadge> {
    let is_included = |a: &str| matches!(a, "flatrate" | "free" | "ads");
    if let Some(p) = rows
        .iter()
        .find(|p| is_included(&p.access) && selected.contains(&p.provider_id))
    {
        return Some(WatchBadge {
            provider_id: p.provider_id,
            provider_name: p.provider_name.clone(),
            logo_path: p.logo_path.clone(),
            tier: WatchTier::Included,
        });
    }
    if rentals {
        if let Some(p) = rows
            .iter()
            .find(|p| matches!(p.access.as_str(), "rent" | "buy"))
        {
            return Some(WatchBadge {
                provider_id: p.provider_id,
                provider_name: p.provider_name.clone(),
                logo_path: p.logo_path.clone(),
                tier: WatchTier::Rent,
            });
        }
    }
    None
}

/// Load the provider map for a region from cache, falling back to a DB read that
/// then populates the cache. Cloned per request, matching the list-cache pattern.
async fn provider_map_for(
    state: &AppState,
    region: &str,
) -> Result<HashMap<i64, Vec<MovieProvider>>, StatusCode> {
    {
        let cache = state.movie_cache.read().await;
        let slot = if region == "BR" {
            &cache.providers_br
        } else {
            &cache.providers_us
        };
        if let Some(c) = slot {
            if c.is_valid() {
                return Ok(c.data.clone());
            }
        }
    }
    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let map = models::get_providers_for_movies(&conn, region)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut cache = state.movie_cache.write().await;
    if region == "BR" {
        cache.providers_br = Some(Cached::new(map.clone()));
    } else {
        cache.providers_us = Some(Cached::new(map.clone()));
    }
    Ok(map)
}

/// Resolved watch filter for one request. `active` is false when the caller sent
/// no region, or a region but neither selected providers nor the rentals toggle.
struct WatchFilter {
    active: bool,
    selected: HashSet<i32>,
    rentals: bool,
    map: HashMap<i64, Vec<MovieProvider>>,
}

impl WatchFilter {
    /// Build from raw query params, loading the region provider map when active.
    async fn resolve(
        state: &AppState,
        region: &Option<String>,
        providers: &Option<String>,
        rentals: &Option<String>,
    ) -> Result<Self, StatusCode> {
        let selected = parse_provider_ids(providers);
        let rentals = matches!(rentals.as_deref(), Some("1") | Some("true"));
        let active = region.is_some() && (!selected.is_empty() || rentals);
        let map = if active {
            let region = normalize_region(region.as_deref().unwrap_or("US"));
            provider_map_for(state, region).await?
        } else {
            HashMap::new()
        };
        Ok(Self {
            active,
            selected,
            rentals,
            map,
        })
    }

    /// Whether a movie passes the filter (always true when inactive).
    fn passes(&self, movie_id: i64) -> bool {
        if !self.active {
            return true;
        }
        let rows = self.map.get(&movie_id).map(|v| v.as_slice()).unwrap_or(&[]);
        watch_badge_for(rows, &self.selected, self.rentals).is_some()
    }

    /// Badge for a movie when the filter is active, else `None`.
    fn badge(&self, movie_id: i64) -> Option<WatchBadge> {
        if !self.active {
            return None;
        }
        let rows = self.map.get(&movie_id).map(|v| v.as_slice()).unwrap_or(&[]);
        watch_badge_for(rows, &self.selected, self.rentals)
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    // Initialize tracing: write to BOTH stdout and gem_finder.log.
    // Two separate fmt layers share the same filter via registry().
    let log_rotation = std::env::var("LOG_ROTATION").unwrap_or_else(|_| "never".to_string());
    let log_file = match log_rotation.to_lowercase().as_str() {
        "daily" => tracing_appender::rolling::daily(".", "gem_finder.log"),
        "hourly" => tracing_appender::rolling::hourly(".", "gem_finder.log"),
        _ => tracing_appender::rolling::never(".", "gem_finder.log"),
    };
    let (file_writer, _guard) = tracing_appender::non_blocking(log_file);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // File layer: no ANSI escape codes so the log file is clean plain text.
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false);

    // Stdout layer: with ANSI colours for human reading in the terminal.
    let stdout_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    // Check for CLI subcommands
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "seed-test-data" => {
                run_seed_test_data().await;
                return;
            }
            _ => {
                eprintln!("Unknown subcommand: {}. Valid: seed-test-data", args[1]);
                std::process::exit(1);
            }
        }
    }

    // Connect to Turso (or local SQLite fallback)
    let db = Database::from_env()
        .await
        .expect("Failed to connect to database");

    tracing::info!("Connected to database");

    // Run migrations
    let conn = db
        .connect()
        .await
        .expect("Failed to get database connection");
    migrations::run(&conn)
        .await
        .expect("Failed to run migrations");

    tracing::info!("Database migrations complete");

    // Log startup to run_logs (before moving db into Arc)
    let startup_conn = db
        .connect()
        .await
        .expect("Failed to get database connection for startup log");
    if let Err(e) = models::insert_run_log(
        &startup_conn,
        "info",
        "startup",
        &format!("Gem Finder API v{} starting", env!("CARGO_PKG_VERSION")),
    )
    .await
    {
        tracing::warn!("Failed to write startup log: {}", e);
    }

    // Read API keys at startup — warn if missing (server still starts; admin ops will fail).
    let tmdb_api_key = std::env::var("TMDB_API_KEY").unwrap_or_default();
    let omdb_api_key = std::env::var("OMDB_API_KEY").unwrap_or_default();

    if tmdb_api_key.is_empty() {
        tracing::warn!("TMDB_API_KEY not set — sync will be unavailable");
    }
    if omdb_api_key.is_empty() {
        tracing::warn!("OMDB_API_KEY not set — enrichment will be unavailable");
    }

    // SMTP — required for magic-link auth. Non-fatal: server starts, sign-in just won't work.
    let smtp_configured = !std::env::var("SMTP_HOST").unwrap_or_default().is_empty()
        && !std::env::var("SMTP_USER").unwrap_or_default().is_empty();
    if !smtp_configured {
        tracing::warn!("SMTP_HOST/SMTP_USER not set — magic-link email will be unavailable");
    }

    // JWT secret — required when SMTP is configured. Fails hard on weak/default values.
    let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_default();
    if smtp_configured {
        if jwt_secret.is_empty() {
            panic!(
                "JWT_SECRET must be set when SMTP is configured (auth would be broken without it)"
            );
        }
        if jwt_secret == "change-me-in-production" {
            panic!(
                "JWT_SECRET is set to the default value — tokens are forgeable. Set a strong secret."
            );
        }
    }
    let jwt_secret = if jwt_secret.is_empty() {
        // Auth is disabled (no SMTP) — random per-boot secret is safe.
        uuid::Uuid::new_v4().to_string()
    } else {
        jwt_secret
    };

    // Admin emails — comma-separated list of emails that get is_admin: true in their JWT.
    let admin_emails: std::collections::HashSet<String> = std::env::var("ADMIN_EMAILS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    // WebAuthn relying-party configuration.
    let webauthn_rp_id =
        std::env::var("WEBAUTHN_RP_ID").unwrap_or_else(|_| "localhost".to_string());
    let webauthn_origin =
        std::env::var("WEBAUTHN_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let webauthn_origin_url =
        Url::parse(&webauthn_origin).expect("WEBAUTHN_ORIGIN must be a valid URL");

    let webauthn = WebauthnBuilder::new(&webauthn_rp_id, &webauthn_origin_url)
        .expect("invalid WebAuthn config")
        .build()
        .expect("failed to build WebAuthn");

    let state = AppState {
        db: Arc::new(db),
        tmdb_api_key,
        omdb_api_key,
        smtp_configured,
        admin_busy: Arc::new(AtomicBool::new(false)),
        movie_cache: Arc::new(RwLock::new(MovieCache::default())),
        jwt_secret,
        webauthn: Arc::new(webauthn),
        passkey_reg_challenges: Arc::new(Mutex::new(HashMap::new())),
        passkey_auth_challenges: Arc::new(Mutex::new(HashMap::new())),
        magic_link_limiter: Arc::new(Mutex::new(HashMap::new())),
        event_rate_limiter: Arc::new(Mutex::new(HashMap::new())),
        admin_emails,
    };

    // CORS: allow origins from CORS_ORIGINS env var (comma-separated). Falls back to permissive in dev.
    let allowed_origins: Vec<axum::http::HeaderValue> = std::env::var("CORS_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    let cors = if allowed_origins.is_empty() {
        CorsLayer::permissive()
    } else {
        CorsLayer::new()
            .allow_origin(allowed_origins)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
                axum::http::Method::OPTIONS,
            ])
            .allow_headers(tower_http::cors::Any)
    };
    let router = Router::new()
        .route("/health", get(health_check))
        .route("/api/gems", get(get_gems))
        .route("/api/acclaimed", get(get_acclaimed))
        .route("/api/wildcards", get(get_wildcards))
        .route("/api/providers", get(get_providers))
        .route("/api/movies/{id}", get(get_movie))
        // Auth
        .route("/api/auth/magic", post(routes::auth::magic_link_request))
        .route("/api/auth/verify", get(routes::auth::magic_link_verify))
        .route(
            "/api/auth/passkey/register/start",
            post(routes::auth::passkey_register_start),
        )
        .route(
            "/api/auth/passkey/register/finish",
            post(routes::auth::passkey_register_finish),
        )
        .route(
            "/api/auth/passkey/authenticate/start",
            post(routes::auth::passkey_auth_start),
        )
        .route(
            "/api/auth/passkey/authenticate/finish",
            post(routes::auth::passkey_auth_finish),
        )
        // Watchlist — JWT protected
        .route(
            "/api/watchlist",
            get(routes::watchlist::get_watchlist).post(routes::watchlist::upsert_watchlist),
        )
        .route(
            "/api/watchlist/movie/{movie_id}",
            get(routes::watchlist::get_watchlist_movie)
                .delete(routes::watchlist::delete_watchlist_movie),
        )
        // User — JWT protected
        .route(
            "/api/user/providers",
            get(routes::user_providers::get_user_providers)
                .put(routes::user_providers::put_user_providers),
        )
        .route("/api/user/me", get(routes::auth::get_me))
        .route(
            "/api/user/username",
            axum::routing::patch(routes::auth::set_username),
        )
        .route(
            "/api/user/username/check",
            get(routes::auth::check_username),
        )
        // Admin
        .route("/api/event", post(routes::events::track_event))
        .route("/api/score", post(run_scoring))
        .route("/api/admin/sync", post(routes::admin::trigger_sync))
        .route("/api/admin/enrich", post(routes::admin::trigger_enrich))
        .route("/api/admin/score", post(routes::admin::trigger_score))
        .route(
            "/api/admin/providers-sync",
            post(routes::admin::trigger_provider_sync),
        )
        .route("/api/admin/seed", post(routes::admin::trigger_seed))
        .route("/api/admin/logs", get(routes::admin::get_run_logs))
        .route("/api/admin/status", get(routes::admin::get_status))
        .layer(TraceLayer::new_for_http())
        .layer(cors);

    // Clone state fields needed by the scheduled sync task BEFORE state is moved into the router.
    let sched_db_pre = state.db.clone();
    let sched_tmdb_pre = state.tmdb_api_key.clone();
    let sched_omdb_pre = state.omdb_api_key.clone();
    let sched_busy_pre = state.admin_busy.clone();
    let sched_cache_pre = state.movie_cache.clone();
    let cleanup_db_pre = state.db.clone();

    let router = router.with_state(state);

    // In production (SERVE_FRONTEND=1), serve the compiled WASM frontend from dist/.
    // trunk build writes index.html + wasm assets there. The fallback serves index.html
    // for all unmatched paths so the Leptos client-side router handles navigation.
    // In API-only or development mode (trunk serve handles the frontend), leave this off.
    let app = if std::env::var("SERVE_FRONTEND").is_ok() {
        use tower_http::services::{ServeDir, ServeFile};
        let serve_dir = ServeDir::new("dist").not_found_service(ServeFile::new("dist/index.html"));
        tracing::info!("Frontend serving enabled from dist/");
        router.fallback_service(serve_dir)
    } else {
        router
    };

    let addr = "0.0.0.0:3000";
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind address");

    // Scheduled sync — runs every SYNC_INTERVAL_HOURS (default 24).
    // Skipped silently if TMDB_API_KEY is absent or another admin op is already running.
    {
        let sched_db = sched_db_pre.clone();
        let sched_tmdb = sched_tmdb_pre.clone();
        let sched_omdb = sched_omdb_pre.clone();
        let sched_busy = sched_busy_pre.clone();
        let sched_cache = sched_cache_pre.clone();

        let interval_hours: u64 = std::env::var("SYNC_INTERVAL_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(24);

        if interval_hours > 0 && !sched_tmdb.is_empty() {
            tokio::spawn(async move {
                let period = std::time::Duration::from_secs(interval_hours * 3600);
                loop {
                    tokio::time::sleep(period).await;

                    // Skip if another admin operation is in progress.
                    use std::sync::atomic::Ordering;
                    if sched_busy
                        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                        .is_err()
                    {
                        tracing::info!("scheduled_sync: skipped — admin op in progress");
                        continue;
                    }
                    let _guard = crate::routes::admin::BusyGuard(sched_busy.clone());

                    tracing::info!("scheduled_sync: starting");
                    let conn = match sched_db.connect().await {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!("scheduled_sync: db connect failed: {}", e);
                            continue;
                        }
                    };

                    let mut svc =
                        crate::services::tmdb_sync::TmdbSyncService::new(sched_tmdb.clone());
                    if let Err(e) = svc.init_config().await {
                        tracing::error!("scheduled_sync: init_config failed: {}", e);
                        continue;
                    }

                    let era_windows: &[(i32, Option<i32>)] = &[
                        (1960, Some(1984)),
                        (1984, Some(1999)),
                        (1999, Some(2012)),
                        (2012, None),
                    ];
                    for (start, end) in era_windows {
                        if let Err(e) = svc.sync_movies(&conn, *start, *end).await {
                            tracing::warn!("scheduled_sync: era window failed: {}", e);
                        }
                    }
                    if let Err(e) = svc.sync_blockbusters(&conn).await {
                        tracing::warn!("scheduled_sync: blockbusters failed: {}", e);
                    }
                    if let Err(e) = svc.sync_acclaimed_candidates(&conn).await {
                        tracing::warn!("scheduled_sync: acclaimed candidates failed: {}", e);
                    }

                    if !sched_omdb.is_empty() {
                        let omdb = crate::services::omdb_sync::OmdbEnrichmentService::new(
                            sched_omdb.clone(),
                        );
                        if let Err(e) = omdb.enrich_movies(&conn, i64::MAX).await {
                            tracing::warn!("scheduled_sync: enrich failed: {}", e);
                        }
                    }

                    if let Err(e) = crate::services::gem_score::run_batch_scoring(&conn).await {
                        tracing::warn!("scheduled_sync: scoring failed: {}", e);
                    }
                    if let Err(e) = gem_finder_db::models::classify_acclaimed_films(&conn).await {
                        tracing::warn!("scheduled_sync: classify acclaimed failed: {}", e);
                    }
                    if let Err(e) = gem_finder_db::models::classify_wildcards(&conn).await {
                        tracing::warn!("scheduled_sync: classify wildcards failed: {}", e);
                    }

                    // Refresh streaming availability for the (now re-scored) catalog.
                    let providers =
                        crate::services::provider_sync::ProviderSyncService::new(sched_tmdb.clone());
                    if let Err(e) = providers.sync_providers(&conn, i64::MAX).await {
                        tracing::warn!("scheduled_sync: provider sync failed: {}", e);
                    }

                    sched_cache.write().await.invalidate();
                    tracing::info!("scheduled_sync: complete, cache invalidated");
                    let _ = gem_finder_db::models::insert_run_log(
                        &conn,
                        "info",
                        "scheduled_sync_complete",
                        &format!("Scheduled sync complete (interval: {}h)", interval_hours),
                    )
                    .await;
                }
            });
            tracing::info!("Scheduled sync enabled — interval: {}h", interval_hours);
        }
    }

    // Scheduled cleanup — runs every CLEANUP_INTERVAL_HOURS (default 24).
    // Independent of TMDB key; purges unverified ghost accounts and stale tokens.
    {
        let cleanup_db = cleanup_db_pre;
        let cleanup_interval_hours: u64 = std::env::var("CLEANUP_INTERVAL_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(24);

        tokio::spawn(async move {
            let period = std::time::Duration::from_secs(cleanup_interval_hours * 3600);
            loop {
                tokio::time::sleep(period).await;

                let conn = match cleanup_db.connect().await {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::error!("scheduled_cleanup: db connect failed: {}", e);
                        continue;
                    }
                };

                if let Err(e) = gem_finder_db::models::delete_unverified_users(&conn).await {
                    tracing::warn!("scheduled_cleanup: delete_unverified_users failed: {}", e);
                } else {
                    tracing::info!("scheduled_cleanup: unverified user sweep complete");
                }
                if let Err(e) = gem_finder_db::models::delete_expired_tokens(&conn).await {
                    tracing::warn!("scheduled_cleanup: delete_expired_tokens failed: {}", e);
                } else {
                    tracing::info!("scheduled_cleanup: expired token sweep complete");
                }
            }
        });
        tracing::info!(
            "Scheduled cleanup enabled — interval: {}h",
            cleanup_interval_hours
        );
    }

    axum::serve(listener, app).await.expect("Server failed");
}

/// CLI subcommand: seed the database with test data for algorithm validation.
///
/// Steps:
/// 1. Init TMDB service + config
/// 2. Seed the 3 known gems (The Sorcerer, Dinner in America, The Hurt Locker)
/// 3. Sync blockbusters (for obscured-by-big-hit signal)
/// 4. Run OMDb enrichment (for real IMDb ratings + RT scores)
/// 5. Run the batch scoring pipeline
/// 6. Log results and print a summary to stdout
async fn run_seed_test_data() {
    tracing::info!("=== seed-test-data: starting ===");

    let db = match Database::from_env().await {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to connect to database: {}", e);
            std::process::exit(1);
        }
    };

    let conn = match db.connect().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to get connection: {}", e);
            std::process::exit(1);
        }
    };

    // Step 0: migrations
    if let Err(e) = migrations::run(&conn).await {
        eprintln!("Migration failed: {}", e);
        std::process::exit(1);
    }
    tracing::info!("Migrations complete");

    // Log start
    let _ = models::insert_run_log(&conn, "info", "seed_started", "seed-test-data starting").await;

    // Step 1: Init TMDB
    let tmdb_key = match std::env::var("TMDB_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("TMDB_API_KEY must be set. Add it to SECRETS.env.");
            std::process::exit(1);
        }
    };
    let mut tmdb = services::tmdb_sync::TmdbSyncService::new(tmdb_key);
    if let Err(e) = tmdb.init_config().await {
        eprintln!("TMDB config init failed: {}", e);
        std::process::exit(1);
    }
    tracing::info!("TMDB service ready");

    // Step 2: Seed known gems
    tracing::info!("Seeding known gems...");
    match tmdb.seed_known_gems(&conn).await {
        Ok((seeded, total, results)) => {
            println!("\n--- Known Gems Seeding Results ---");
            for (title, result) in &results {
                let icon = match result.as_str() {
                    "seeded" => "✅",
                    "not_found" => "❌",
                    _ => "⚠️",
                };
                println!("  {} {}: {}", icon, title, result);
            }
            println!("  Total: {}/{} seeded\n", seeded, total);
            let _ = models::insert_run_log(
                &conn,
                "info",
                "seed_gems",
                &format!("Seeded {}/{} known gems", seeded, total),
            )
            .await;
        }
        Err(e) => {
            eprintln!("Seed known gems failed: {}", e);
        }
    }

    // Step 3: Sync blockbusters
    tracing::info!("Syncing blockbusters...");
    match tmdb.sync_blockbusters(&conn).await {
        Ok(count) => {
            println!("--- Blockbusters ---");
            println!("  {} blockbusters synced to big_hits table\n", count);
            let _ = models::insert_run_log(
                &conn,
                "info",
                "seed_blockbusters",
                &format!("Synced {} blockbusters", count),
            )
            .await;
        }
        Err(e) => {
            eprintln!("Blockbuster sync failed: {}", e);
        }
    }

    // Step 3b: Discover sync — gem candidates across 4 era windows.
    // Each window uses 5 pages (100 films, vote_count.desc) so we get the most-notable
    // films distributed across ALL years in the era, not just the newest 100.
    // This gives ~400 candidates spread across 1960–present.
    //
    // Windows: classics (1960-1984), modern classics (1984-1999),
    //          2000s (1999-2012), recent (2012-present)
    tracing::info!("Syncing gem candidates across era windows...");
    let era_windows: &[(i32, Option<i32>, &str)] = &[
        (1960, Some(1984), "classics 1960–1984"),
        (1984, Some(1999), "modern classics 1984–1999"),
        (1999, Some(2012), "2000s 1999–2012"),
        (2012, None, "recent 2012–present"),
    ];
    let mut windows_ok = 0usize;
    for (start, end, label) in era_windows {
        tracing::info!("  Era window: {}", label);
        match tmdb.sync_movies(&conn, *start, *end).await {
            Ok(()) => {
                println!("  ✓ {}", label);
                windows_ok += 1;
            }
            Err(e) => eprintln!("  ✗ {}: {}", label, e),
        }
    }
    println!("--- Gem Candidate Sync ---");
    println!(
        "  {}/{} era windows completed (see logs for per-window detail)\n",
        windows_ok,
        era_windows.len()
    );

    // Step 3c: Sync acclaimed candidates before OMDb enrichment so they get enriched
    // in the same run. Films with vote_avg ≥ 7.5 and vote_count ≥ 10,000 are the
    // acclaimed tier; classify_acclaimed_films (step 5b) then filters by IMDb + RT.
    tracing::info!("Syncing acclaimed candidates...");
    match tmdb.sync_acclaimed_candidates(&conn).await {
        Ok(count) => {
            println!("--- Acclaimed Candidate Sync ---");
            println!("  {} new acclaimed candidates synced\n", count);
        }
        Err(e) => {
            eprintln!("Acclaimed candidate sync failed: {}", e);
        }
    }

    // Step 4: OMDb enrichment — drain the entire unenriched queue.
    // Limit must be high enough that no unenriched movie reaches the scoring pipeline,
    // since missing RT data means the RT quality gate can't fire and bad films slip through.
    let omdb_key = std::env::var("OMDB_API_KEY").unwrap_or_default();
    if !omdb_key.is_empty() {
        let omdb = services::omdb_sync::OmdbEnrichmentService::new(omdb_key);
        tracing::info!("Running OMDb enrichment...");
        match omdb.enrich_movies(&conn, i64::MAX).await {
            Ok((enriched, total, errors)) => {
                println!("--- OMDb Enrichment ---");
                println!("  {}/{} movies enriched", enriched, total);
                if !errors.is_empty() {
                    println!("  Errors ({}):", errors.len());
                    for e in errors.iter().take(5) {
                        println!("    - {}", e);
                    }
                }
                println!();
                let _ = models::insert_run_log(
                    &conn,
                    "info",
                    "seed_enrichment",
                    &format!("Enriched {}/{} movies via OMDb", enriched, total),
                )
                .await;
            }
            Err(e) => {
                eprintln!("OMDb enrichment failed: {}", e);
            }
        }
    } else {
        println!("--- OMDb Enrichment ---");
        println!("  Skipped: set OMDB_API_KEY env var to enable\n");
    }

    // Step 5: Run scoring
    tracing::info!("Running batch scoring...");
    match services::gem_score::run_batch_scoring(&conn).await {
        Ok(scored) => {
            println!("--- Scoring ---");
            println!("  {} movies scored\n", scored);
            let _ = models::insert_run_log(
                &conn,
                "info",
                "seed_scoring",
                &format!("Scored {} movies", scored),
            )
            .await;
        }
        Err(e) => {
            eprintln!("Scoring failed: {}", e);
        }
    }

    // Step 5b: Classify acclaimed films (populate acclaimed table from enriched movies)
    tracing::info!("Classifying acclaimed films...");
    match models::classify_acclaimed_films(&conn).await {
        Ok(count) => {
            println!("--- Acclaimed Classification ---");
            println!("  {} films in acclaimed table\n", count);
            let _ = models::insert_run_log(
                &conn,
                "info",
                "seed_acclaimed",
                &format!("Classified {} acclaimed films", count),
            )
            .await;
        }
        Err(e) => {
            eprintln!("Acclaimed classification failed: {}", e);
        }
    }

    // Step 5c: Classify wildcards (divisive films: scored but RT < 50%)
    tracing::info!("Classifying wildcards...");
    match models::classify_wildcards(&conn).await {
        Ok(count) => {
            println!("--- Wildcard Classification ---");
            println!("  {} films in wildcards table\n", count);
            let _ = models::insert_run_log(
                &conn,
                "info",
                "seed_wildcards",
                &format!("Classified {} wildcard films", count),
            )
            .await;
        }
        Err(e) => {
            eprintln!("Wildcard classification failed: {}", e);
        }
    }

    // Step 6: Print summary
    println!("=== seed-test-data complete ===");
    println!();
    println!("To verify rankings, start the server and query:");
    println!("  curl http://localhost:3000/api/gems");
    println!();
    println!("To view run logs:");
    println!("  curl http://localhost:3000/api/admin/logs");

    let _ = models::insert_run_log(&conn, "info", "seed_complete", "seed-test-data finished").await;
}

/// Health check endpoint.
async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let db_status = match state.db.connect().await {
        Ok(conn) => match conn.execute("SELECT 1", turso::params![]).await {
            Ok(_) => "connected".to_string(),
            Err(e) => format!("error: {}", e),
        },
        Err(e) => format!("error: {}", e),
    };

    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: db_status,
    })
}

/// GET /api/gems — paginated hidden gems with optional year/genre filters.
///
/// Serves from an in-memory TTL cache (CACHE_TTL). Cache miss triggers one DB
/// query that fetches the full sorted list; subsequent requests paginate and
/// filter the in-memory Vec until the TTL expires or admin scoring invalidates it.
async fn get_gems(
    State(state): State<AppState>,
    Query(query): Query<GemsQuery>,
) -> Result<Json<PaginatedResponse<MovieSummary>>, StatusCode> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    // ── 1. Try cache (read lock — cheap, allows concurrent readers) ──────────
    let full_list: Vec<MovieSummary> = {
        let cache = state.movie_cache.read().await;
        if let Some(ref c) = cache.gems {
            if c.is_valid() {
                c.data.clone()
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    };

    // ── 2. Cache miss — fetch full list from DB and populate cache ───────────
    let full_list = if full_list.is_empty() {
        let conn = state
            .db
            .connect()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let list = models::get_all_gems_for_cache(&conn)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        state.movie_cache.write().await.gems = Some(Cached::new(list.clone()));
        list
    } else {
        full_list
    };

    // ── 3. Apply user-supplied filters in memory ─────────────────────────────
    let wf = WatchFilter::resolve(&state, &query.region, &query.providers, &query.rentals).await?;
    let mut filtered: Vec<&MovieSummary> = full_list
        .iter()
        .filter(|m| {
            if let Some(min_y) = query.min_year {
                if m.year.is_none_or(|y| y < min_y) {
                    return false;
                }
            }
            if let Some(ref g) = query.genres {
                let selected: Vec<String> = g
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.trim().to_lowercase())
                    .collect();
                if !selected.is_empty() {
                    let movie_genres = m.genre.as_deref().unwrap_or("").to_lowercase();
                    let movie_keywords = m.keywords.as_deref().unwrap_or("").to_lowercase();
                    if !selected.iter().all(|sel| {
                        if sel == "musical" {
                            movie_keywords.contains("musical")
                        } else {
                            movie_genres.contains(sel.as_str())
                        }
                    }) {
                        return false;
                    }
                }
            }
            if let Some(ref q) = query.q {
                let q_lower = q.to_lowercase();
                if !m.title.to_lowercase().contains(&q_lower) {
                    return false;
                }
            }
            if !wf.passes(m.id) {
                return false;
            }
            true
        })
        .collect();

    // ── 4. Sort ──────────────────────────────────────────────────────────────
    let sort_field = query.sort.as_deref().unwrap_or("score");
    let sort_dir = query.sort_dir.as_deref().unwrap_or("desc");
    apply_sort(&mut filtered, sort_field, sort_dir);

    // ── 5. Paginate ──────────────────────────────────────────────────────────
    let total = filtered.len() as i64;
    let start = ((page - 1) * per_page) as usize;
    let mut data: Vec<MovieSummary> = filtered
        .into_iter()
        .skip(start)
        .take(per_page as usize)
        .cloned()
        .collect();
    if wf.active {
        for m in &mut data {
            m.watch_badge = wf.badge(m.id);
        }
    }

    Ok(Json(PaginatedResponse {
        data,
        total,
        page,
        per_page,
    }))
}

/// GET /api/acclaimed — paginated acclaimed films (IMDb ≥ 8.0, RT ≥ 80%) with optional filters.
async fn get_acclaimed(
    State(state): State<AppState>,
    Query(query): Query<AclaimedQuery>,
) -> Result<Json<PaginatedResponse<MovieSummary>>, StatusCode> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let full_list: Vec<MovieSummary> = {
        let cache = state.movie_cache.read().await;
        if let Some(ref c) = cache.acclaimed {
            if c.is_valid() {
                c.data.clone()
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    };

    let full_list = if full_list.is_empty() {
        let conn = state
            .db
            .connect()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let list = models::get_all_acclaimed_for_cache(&conn)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        state.movie_cache.write().await.acclaimed = Some(Cached::new(list.clone()));
        list
    } else {
        full_list
    };

    let wf = WatchFilter::resolve(&state, &query.region, &query.providers, &query.rentals).await?;
    let mut filtered: Vec<&MovieSummary> = full_list
        .iter()
        .filter(|m| {
            if let Some(min_y) = query.min_year {
                if m.year.is_none_or(|y| y < min_y) {
                    return false;
                }
            }
            if let Some(ref g) = query.genres {
                let selected: Vec<String> = g
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.trim().to_lowercase())
                    .collect();
                if !selected.is_empty() {
                    let movie_genres = m.genre.as_deref().unwrap_or("").to_lowercase();
                    let movie_keywords = m.keywords.as_deref().unwrap_or("").to_lowercase();
                    if !selected.iter().all(|sel| {
                        if sel == "musical" {
                            movie_keywords.contains("musical")
                        } else {
                            movie_genres.contains(sel.as_str())
                        }
                    }) {
                        return false;
                    }
                }
            }
            if let Some(ref q) = query.q {
                if !m.title.to_lowercase().contains(&q.to_lowercase()) {
                    return false;
                }
            }
            if !wf.passes(m.id) {
                return false;
            }
            true
        })
        .collect();

    let sort_field = query.sort.as_deref().unwrap_or("rating");
    let sort_dir = query.sort_dir.as_deref().unwrap_or("desc");
    apply_sort(&mut filtered, sort_field, sort_dir);

    let total = filtered.len() as i64;
    let start = ((page - 1) * per_page) as usize;
    let mut data: Vec<MovieSummary> = filtered
        .into_iter()
        .skip(start)
        .take(per_page as usize)
        .cloned()
        .collect();
    if wf.active {
        for m in &mut data {
            m.watch_badge = wf.badge(m.id);
        }
    }

    Ok(Json(PaginatedResponse {
        data,
        total,
        page,
        per_page,
    }))
}

/// GET /api/wildcards — paginated wildcard films (scored but RT < 50%) with optional filters.
async fn get_wildcards(
    State(state): State<AppState>,
    Query(query): Query<WildcardsQuery>,
) -> Result<Json<PaginatedResponse<MovieSummary>>, StatusCode> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let full_list: Vec<MovieSummary> = {
        let cache = state.movie_cache.read().await;
        if let Some(ref c) = cache.wildcards {
            if c.is_valid() {
                c.data.clone()
            } else {
                vec![]
            }
        } else {
            vec![]
        }
    };

    let full_list = if full_list.is_empty() {
        let conn = state
            .db
            .connect()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let list = models::get_all_wildcards_for_cache(&conn)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        state.movie_cache.write().await.wildcards = Some(Cached::new(list.clone()));
        list
    } else {
        full_list
    };

    let wf = WatchFilter::resolve(&state, &query.region, &query.providers, &query.rentals).await?;
    let mut filtered: Vec<&MovieSummary> = full_list
        .iter()
        .filter(|m| {
            if let Some(min_y) = query.min_year {
                if m.year.is_none_or(|y| y < min_y) {
                    return false;
                }
            }
            if let Some(ref g) = query.genres {
                let selected: Vec<String> = g
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.trim().to_lowercase())
                    .collect();
                if !selected.is_empty() {
                    let movie_genres = m.genre.as_deref().unwrap_or("").to_lowercase();
                    let movie_keywords = m.keywords.as_deref().unwrap_or("").to_lowercase();
                    if !selected.iter().all(|sel| {
                        if sel == "musical" {
                            movie_keywords.contains("musical")
                        } else {
                            movie_genres.contains(sel.as_str())
                        }
                    }) {
                        return false;
                    }
                }
            }
            if let Some(ref q) = query.q {
                if !m.title.to_lowercase().contains(&q.to_lowercase()) {
                    return false;
                }
            }
            if !wf.passes(m.id) {
                return false;
            }
            true
        })
        .collect();

    let sort_field = query.sort.as_deref().unwrap_or("score");
    let sort_dir = query.sort_dir.as_deref().unwrap_or("desc");
    apply_sort(&mut filtered, sort_field, sort_dir);

    let total = filtered.len() as i64;
    let start = ((page - 1) * per_page) as usize;
    let mut data: Vec<MovieSummary> = filtered
        .into_iter()
        .skip(start)
        .take(per_page as usize)
        .cloned()
        .collect();
    if wf.active {
        for m in &mut data {
            m.watch_badge = wf.badge(m.id);
        }
    }

    Ok(Json(PaginatedResponse {
        data,
        total,
        page,
        per_page,
    }))
}

/// GET /api/providers?region= — distinct streaming providers present in the
/// catalog for a region (included tiers only), with catalog counts. Powers the
/// "What can I watch?" picker so only services that actually stream ≥1 title show.
async fn get_providers(
    State(state): State<AppState>,
    Query(query): Query<ProvidersQuery>,
) -> Result<Json<Vec<ProviderInfo>>, StatusCode> {
    let region = normalize_region(query.region.as_deref().unwrap_or("US"));
    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let providers = models::get_distinct_providers(&conn, region)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(providers))
}

/// GET /api/movies/:id — single movie by mv+base36 encoded ID.
///
/// Accepts: `/api/movies/mv16` (ID 42), `/api/movies/mvrs` (ID 1000).
/// Returns 400 for malformed IDs, 404 for unknown IDs.
async fn get_movie(
    State(state): State<AppState>,
    Path(encoded_id): Path<String>,
) -> Result<Json<Movie>, StatusCode> {
    let id = gem_finder_shared::id_encode::decode_movie_id(&encoded_id)
        .ok_or(StatusCode::BAD_REQUEST)?;

    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut movie = models::get_movie_by_id(&conn, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Attach streaming availability grouped per region (Phase 10).
    if let Ok(providers) = models::get_movie_providers(&conn, id, movie.tmdb_id).await {
        if !providers.is_empty() {
            movie.watch_providers = Some(providers);
        }
    }

    Ok(Json(movie))
}

/// POST /api/score — run batch scoring synchronously (used from admin UI).
async fn run_scoring(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    routes::admin::check_admin_token(&headers, &state.jwt_secret)?;
    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let scored = crate::services::gem_score::run_batch_scoring(&conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "scored": scored })))
}

#[cfg(test)]
mod watch_filter_tests {
    use super::{parse_provider_ids, watch_badge_for};
    use gem_finder_shared::types::{MovieProvider, WatchTier};
    use std::collections::HashSet;

    fn prov(id: i32, access: &str) -> MovieProvider {
        MovieProvider {
            provider_id: id,
            provider_name: format!("Provider {}", id),
            logo_path: None,
            access: access.to_string(),
        }
    }

    fn selected(ids: &[i32]) -> HashSet<i32> {
        ids.iter().copied().collect()
    }

    #[test]
    fn selected_service_flatrate_matches_included() {
        let rows = vec![prov(8, "flatrate")];
        let badge = watch_badge_for(&rows, &selected(&[8]), false).expect("should match");
        assert_eq!(badge.provider_id, 8);
        assert_eq!(badge.tier, WatchTier::Included);
    }

    #[test]
    fn unselected_service_flatrate_does_not_match() {
        let rows = vec![prov(8, "flatrate")];
        // Netflix (8) streams it, but the user only selected provider 9.
        assert!(watch_badge_for(&rows, &selected(&[9]), false).is_none());
    }

    #[test]
    fn rental_without_toggle_does_not_match() {
        let rows = vec![prov(8, "rent")];
        assert!(watch_badge_for(&rows, &selected(&[8]), false).is_none());
    }

    #[test]
    fn rental_with_toggle_matches_any_provider() {
        let rows = vec![prov(99, "buy")];
        // Rentals need no subscription — any provider qualifies when the toggle is on.
        let badge = watch_badge_for(&rows, &selected(&[]), true).expect("should match");
        assert_eq!(badge.tier, WatchTier::Rent);
    }

    #[test]
    fn included_preferred_over_rent() {
        let rows = vec![prov(8, "rent"), prov(8, "flatrate")];
        let badge = watch_badge_for(&rows, &selected(&[8]), true).expect("should match");
        assert_eq!(badge.tier, WatchTier::Included);
    }

    #[test]
    fn free_and_ads_count_as_included() {
        for access in ["free", "ads"] {
            let rows = vec![prov(7, access)];
            let badge = watch_badge_for(&rows, &selected(&[7]), false)
                .unwrap_or_else(|| panic!("{} should match", access));
            assert_eq!(badge.tier, WatchTier::Included);
        }
    }

    #[test]
    fn parses_csv_provider_ids() {
        let ids = parse_provider_ids(&Some("8, 9,x,337".to_string()));
        assert_eq!(ids, selected(&[8, 9, 337]));
        assert!(parse_provider_ids(&None).is_empty());
    }
}
