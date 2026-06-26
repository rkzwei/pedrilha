use crate::services::omdb_sync::OmdbEnrichmentService;
use crate::services::tmdb_sync::TmdbSyncService;
use crate::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use gem_finder_db::models;
use serde::Deserialize;
use std::env;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

/// RAII guard that clears the admin_busy flag on drop.
/// Ensures the flag is always released even if the background task panics.
pub(crate) struct BusyGuard(pub(crate) Arc<AtomicBool>);
impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Try to acquire the admin busy flag.
/// Returns `Ok(BusyGuard)` if acquired, `Err(409)` if another operation is running.
fn acquire_busy(flag: &Arc<AtomicBool>) -> Result<BusyGuard, StatusCode> {
    flag.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .map(|_| BusyGuard(flag.clone()))
        .map_err(|_| StatusCode::CONFLICT)
}

#[derive(Deserialize)]
pub struct EnrichQuery {
    pub limit: Option<i64>,
}

/// Minimal bearer-token guard.
pub(crate) fn check_admin_token(headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected = env::var("ADMIN_TOKEN").unwrap_or_default();
    if expected.is_empty() {
        tracing::warn!("ADMIN_TOKEN is not set; admin endpoints are unprotected");
        return Ok(());
    }

    let provided = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if provided != expected {
        tracing::warn!("Admin request rejected: invalid or missing bearer token");
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(())
}

/// POST /api/admin/sync
///
/// Spawns a background task and returns 202 immediately.
/// The sync runs on the server regardless of whether the client stays connected.
pub async fn trigger_sync(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    check_admin_token(&headers)?;

    if state.tmdb_api_key.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let guard = acquire_busy(&state.admin_busy)?;
    let db = state.db.clone();
    let tmdb_key = state.tmdb_api_key.clone();
    let cache = state.movie_cache.clone();

    tokio::spawn(async move {
        let _guard = guard; // released when this task ends (or panics)
        let conn = match db.connect().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("sync: db connect failed: {}", e);
                return;
            }
        };

        let log = |level: &'static str, event: &'static str, msg: String| {
            let conn_ref = &conn;
            async move {
                if let Err(e) = models::insert_run_log(conn_ref, level, event, &msg).await {
                    tracing::warn!("Failed to write run log: {}", e);
                }
            }
        };

        log("info", "sync_started", "TMDB sync pipeline starting".into()).await;

        let mut svc = TmdbSyncService::new(tmdb_key);
        if let Err(e) = svc.init_config().await {
            tracing::error!("sync: init_config failed: {}", e);
            log("error", "sync_failed", format!("init_config: {}", e)).await;
            return;
        }

        let era_windows: &[(i32, Option<i32>, &str)] = &[
            (1960, Some(1984), "classics 1960–1984"),
            (1984, Some(1999), "modern classics 1984–1999"),
            (1999, Some(2012), "2000s 1999–2012"),
            (2012, None, "recent 2012–present"),
        ];

        let t0 = std::time::Instant::now();
        for (start, end, label) in era_windows {
            if let Err(e) = svc.sync_movies(&conn, *start, *end).await {
                tracing::error!("sync era window {} failed: {}", label, e);
                log("error", "sync_era_failed", format!("{}: {}", label, e)).await;
                return;
            }
        }
        log(
            "info",
            "sync_phase_a_complete",
            format!("{} era windows in {:?}", era_windows.len(), t0.elapsed()),
        )
        .await;

        let t1 = std::time::Instant::now();
        match svc.sync_blockbusters(&conn).await {
            Ok(n) => {
                log(
                    "info",
                    "sync_phase_b_complete",
                    format!("{} blockbusters in {:?}", n, t1.elapsed()),
                )
                .await
            }
            Err(e) => {
                log("error", "sync_failed", format!("blockbusters: {}", e)).await;
                return;
            }
        }

        let t2 = std::time::Instant::now();
        match svc.seed_known_gems(&conn).await {
            Ok((seeded, total, _)) => {
                log(
                    "info",
                    "sync_complete",
                    format!(
                        "done — seeded {}/{} known gems in {:?}",
                        seeded,
                        total,
                        t2.elapsed()
                    ),
                )
                .await
            }
            Err(e) => log("error", "sync_failed", format!("known gems: {}", e)).await,
        }

        // Sync added new movies — movie list cache is stale.
        cache.write().await.invalidate();
        tracing::info!("movie list cache invalidated after sync");
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "message": "Sync started in background — watch logs for progress",
    })))
}

/// POST /api/admin/enrich
///
/// Spawns a background task and returns 202 immediately.
pub async fn trigger_enrich(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<EnrichQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    check_admin_token(&headers)?;

    if state.omdb_api_key.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let guard = acquire_busy(&state.admin_busy)?;
    let db = state.db.clone();
    let omdb_key = state.omdb_api_key.clone();
    let limit = payload.limit.unwrap_or(i64::MAX);
    let limit_display = if limit == i64::MAX {
        "unlimited".to_string()
    } else {
        limit.to_string()
    };
    let limit_display_inner = limit_display.clone();

    tokio::spawn(async move {
        let _guard = guard;
        let conn = match db.connect().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("enrich: db connect failed: {}", e);
                return;
            }
        };

        if let Err(e) = models::insert_run_log(
            &conn,
            "info",
            "enrich_started",
            &format!("OMDb enrichment starting (limit: {})", limit_display_inner),
        )
        .await
        {
            tracing::warn!("log write failed: {}", e);
        }

        let svc = OmdbEnrichmentService::new(omdb_key);
        let t = std::time::Instant::now();
        match svc.enrich_movies(&conn, limit).await {
            Ok((enriched, total, errors)) => {
                let msg = format!(
                    "{}/{} enriched, {} errors in {:?}",
                    enriched,
                    total,
                    errors.len(),
                    t.elapsed()
                );
                let _ = models::insert_run_log(&conn, "info", "enrich_complete", &msg).await;
            }
            Err(e) => {
                tracing::error!("enrich failed: {}", e);
                let _ = models::insert_run_log(&conn, "error", "enrich_failed", &format!("{}", e))
                    .await;
            }
        }
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "limit": limit_display,
        "message": "Enrichment started in background — watch logs for progress",
    })))
}

/// POST /api/admin/score
///
/// Spawns a background task and returns 202 immediately.
pub async fn trigger_score(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    check_admin_token(&headers)?;

    let guard = acquire_busy(&state.admin_busy)?;
    let db = state.db.clone();
    let cache = state.movie_cache.clone();

    tokio::spawn(async move {
        let _guard = guard;
        let conn = match db.connect().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("score: db connect failed: {}", e);
                return;
            }
        };

        let _ = models::insert_run_log(&conn, "info", "score_started", "Gem scoring run starting")
            .await;

        let t = std::time::Instant::now();
        match crate::services::gem_score::run_batch_scoring(&conn).await {
            Ok(scored) => {
                let msg = format!("{} movies scored in {:?}", scored, t.elapsed());
                let _ = models::insert_run_log(&conn, "info", "score_complete", &msg).await;
            }
            Err(e) => {
                tracing::error!("scoring failed: {}", e);
                let _ =
                    models::insert_run_log(&conn, "error", "score_failed", &format!("{}", e)).await;
            }
        }

        // Classify acclaimed after scoring
        match models::classify_acclaimed_films(&conn).await {
            Ok(n) => {
                let _ = models::insert_run_log(
                    &conn,
                    "info",
                    "acclaimed_classified",
                    &format!("{} acclaimed films", n),
                )
                .await;
            }
            Err(e) => tracing::warn!("acclaimed classification failed: {}", e),
        }

        // Classify wildcards after scoring
        match models::classify_wildcards(&conn).await {
            Ok(n) => {
                let _ = models::insert_run_log(
                    &conn,
                    "info",
                    "wildcards_classified",
                    &format!("{} wildcards", n),
                )
                .await;
            }
            Err(e) => tracing::warn!("wildcard classification failed: {}", e),
        }

        // Scores changed — movie list cache is stale.
        cache.write().await.invalidate();
        tracing::info!("movie list cache invalidated after scoring");
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "message": "Scoring started in background — watch logs for progress",
    })))
}

/// POST /api/admin/seed
///
/// Full pipeline: sync known gems → OMDb enrich → score → classify wildcards.
/// Spawns a background task and returns 202 immediately.
pub async fn trigger_seed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    check_admin_token(&headers)?;

    let guard = acquire_busy(&state.admin_busy)?;
    let db = state.db.clone();
    let tmdb_key = state.tmdb_api_key.clone();
    let omdb_key = state.omdb_api_key.clone();

    tokio::spawn(async move {
        let _guard = guard;
        let conn = match db.connect().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("seed: db connect failed: {}", e);
                return;
            }
        };

        let log = |level: &'static str, event: &'static str, msg: String| {
            let conn_ref = &conn;
            async move {
                if let Err(e) = models::insert_run_log(conn_ref, level, event, &msg).await {
                    tracing::warn!("log write failed: {}", e);
                }
            }
        };

        log("info", "seed_started", "Seed test data starting".into()).await;

        // Step 1: TMDB sync for known gems
        let mut tmdb = TmdbSyncService::new(tmdb_key);
        if let Err(e) = tmdb.init_config().await {
            log("error", "seed_failed", format!("init_config: {}", e)).await;
            return;
        }
        match tmdb.seed_known_gems(&conn).await {
            Ok((seeded, total, _)) => {
                log(
                    "info",
                    "seed_gems_done",
                    format!("seeded {}/{} known gems", seeded, total),
                )
                .await
            }
            Err(e) => {
                log("error", "seed_failed", format!("seed_known_gems: {}", e)).await;
                return;
            }
        }

        // Step 2: OMDb enrichment (best-effort; log warning if key absent)
        if omdb_key.is_empty() {
            log("warn", "seed_enrich_skipped", "OMDB_API_KEY not set".into()).await;
        } else {
            let omdb = OmdbEnrichmentService::new(omdb_key);
            match omdb.enrich_movies(&conn, i64::MAX).await {
                Ok((enriched, total, _)) => {
                    log(
                        "info",
                        "seed_enrich_done",
                        format!("enriched {}/{}", enriched, total),
                    )
                    .await
                }
                Err(e) => log("warn", "seed_enrich_failed", format!("omdb: {}", e)).await,
            }
        }

        // Step 3: Batch scoring
        let t = std::time::Instant::now();
        match crate::services::gem_score::run_batch_scoring(&conn).await {
            Ok(scored) => {
                log(
                    "info",
                    "seed_score_done",
                    format!("{} movies scored in {:?}", scored, t.elapsed()),
                )
                .await
            }
            Err(e) => {
                log("error", "seed_score_failed", format!("{}", e)).await;
                return;
            }
        }

        // Step 4: Classify wildcards
        match models::classify_wildcards(&conn).await {
            Ok(n) => {
                log(
                    "info",
                    "seed_wildcards_done",
                    format!("{} wildcards classified", n),
                )
                .await
            }
            Err(e) => log("warn", "seed_wildcards_failed", format!("{}", e)).await,
        }

        log("info", "seed_complete", "Seed test data complete".into()).await;
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "message": "Seed started in background — watch logs for progress",
    })))
}

/// GET /api/admin/status
///
/// Returns which external services are configured. Used by the frontend to
/// conditionally hide sign-in (if SMTP is absent) and show admin warnings.
/// No auth required — contains no secrets, only boolean capability flags.
pub async fn get_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "smtp_configured":  state.smtp_configured,
        "tmdb_configured":  !state.tmdb_api_key.is_empty(),
        "omdb_configured":  !state.omdb_api_key.is_empty(),
        "log_rotation": std::env::var("LOG_ROTATION").unwrap_or_else(|_| "never".to_string()),
    }))
}

/// GET /api/admin/logs
///
/// Returns the 50 most recent run log entries.
pub async fn get_run_logs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    check_admin_token(&headers)?;

    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let logs = models::get_recent_run_logs(&conn, 50)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "logs": logs })))
}
