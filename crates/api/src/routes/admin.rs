use crate::middleware::auth::verify_jwt;
use crate::services::omdb_sync::OmdbEnrichmentService;
use crate::services::provider_sync::ProviderSyncService;
use crate::services::tmdb_sync::TmdbSyncService;
use crate::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::Datelike;
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

/// Admin auth guard — accepts either:
///   - `Authorization: Bearer <ADMIN_TOKEN>` (static token for CLI/scripts), or
///   - `Authorization: Bearer <JWT>` with `is_admin: true` claim (browser UI).
pub(crate) fn check_admin_token(headers: &HeaderMap, jwt_secret: &str) -> Result<(), StatusCode> {
    let bearer = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    if bearer.is_empty() {
        tracing::warn!("Admin request rejected: missing Authorization header");
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Try JWT path first — valid is_admin JWT is accepted even if ADMIN_TOKEN is unset.
    if let Ok(claims) = verify_jwt(bearer, jwt_secret) {
        if claims.is_admin {
            return Ok(());
        }
        tracing::warn!("Admin request rejected: JWT valid but is_admin is false");
        return Err(StatusCode::FORBIDDEN);
    }

    // Fall back to static ADMIN_TOKEN.
    let expected = env::var("ADMIN_TOKEN").unwrap_or_default();
    if expected.is_empty() {
        tracing::error!("ADMIN_TOKEN is not set and JWT auth failed; rejecting admin request");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    if bearer != expected {
        tracing::warn!("Admin request rejected: invalid bearer token");
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
    check_admin_token(&headers, &state.jwt_secret)?;

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

        // Acclaimed candidates (vote_avg ≥ 7.5, no upper cap) — the only path that
        // ingests audience-canonized classics with full detail (incl. imdb_id).
        let t3 = std::time::Instant::now();
        match svc.sync_acclaimed_candidates(&conn).await {
            Ok(n) => {
                log(
                    "info",
                    "sync_acclaimed_candidates_complete",
                    format!("{} acclaimed candidates in {:?}", n, t3.elapsed()),
                )
                .await
            }
            Err(e) => {
                log(
                    "error",
                    "sync_failed",
                    format!("acclaimed candidates: {}", e),
                )
                .await
            }
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
    check_admin_token(&headers, &state.jwt_secret)?;

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
    check_admin_token(&headers, &state.jwt_secret)?;

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
        match models::classify_acclaimed_films(&conn, chrono::Utc::now().year()).await {
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

/// POST /api/admin/providers-sync
///
/// Fetches streaming availability (TMDB watch-providers / JustWatch) for scored
/// movies whose data is missing or stale. Spawns a background task, returns 202.
pub async fn trigger_provider_sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<EnrichQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    check_admin_token(&headers, &state.jwt_secret)?;

    if state.tmdb_api_key.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let guard = acquire_busy(&state.admin_busy)?;
    let db = state.db.clone();
    let tmdb_key = state.tmdb_api_key.clone();
    let cache = state.movie_cache.clone();
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
                tracing::error!("provider-sync: db connect failed: {}", e);
                return;
            }
        };

        let _ = models::insert_run_log(
            &conn,
            "info",
            "provider_sync_started",
            &format!("Provider sync starting (limit: {})", limit_display_inner),
        )
        .await;

        let svc = ProviderSyncService::new(tmdb_key);
        let t = std::time::Instant::now();
        match svc.sync_providers(&conn, limit).await {
            Ok((synced, total, errors)) => {
                let msg = format!(
                    "{}/{} synced, {} errors in {:?}",
                    synced,
                    total,
                    errors.len(),
                    t.elapsed()
                );
                let _ = models::insert_run_log(&conn, "info", "provider_sync_complete", &msg).await;
            }
            Err(e) => {
                tracing::error!("provider sync failed: {}", e);
                let _ = models::insert_run_log(
                    &conn,
                    "error",
                    "provider_sync_failed",
                    &format!("{}", e),
                )
                .await;
            }
        }

        // New availability data — movie list cache is stale.
        cache.write().await.invalidate();
        tracing::info!("movie list cache invalidated after provider sync");
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "limit": limit_display,
        "message": "Provider sync started in background — watch logs for progress",
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
    check_admin_token(&headers, &state.jwt_secret)?;

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
    check_admin_token(&headers, &state.jwt_secret)?;

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

/// POST /api/admin/sync-acclaimed
///
/// Targeted ingestion for audience-canonized classics: sync acclaimed candidates
/// (full detail incl. imdb_id) → OMDb enrich → classify acclaimed. Cheaper on TMDB
/// quota than a full sync. Spawns a background task and returns immediately.
pub async fn sync_acclaimed(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    check_admin_token(&headers, &state.jwt_secret)?;
    if state.tmdb_api_key.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let guard = acquire_busy(&state.admin_busy)?;
    let db = state.db.clone();
    let tmdb_key = state.tmdb_api_key.clone();
    let omdb_key = state.omdb_api_key.clone();
    let cache = state.movie_cache.clone();

    tokio::spawn(async move {
        let _guard = guard;
        let conn = match db.connect().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("sync_acclaimed: db connect failed: {}", e);
                return;
            }
        };
        let _ = models::insert_run_log(
            &conn,
            "info",
            "sync_acclaimed_started",
            "Acclaimed ingestion starting",
        )
        .await;

        let mut svc = TmdbSyncService::new(tmdb_key);
        if let Err(e) = svc.init_config().await {
            let _ = models::insert_run_log(
                &conn,
                "error",
                "sync_acclaimed_failed",
                &format!("init_config: {}", e),
            )
            .await;
            return;
        }
        match svc.sync_acclaimed_candidates(&conn).await {
            Ok(n) => {
                let _ = models::insert_run_log(
                    &conn,
                    "info",
                    "sync_acclaimed_synced",
                    &format!("{} candidates synced", n),
                )
                .await;
            }
            Err(e) => {
                let _ = models::insert_run_log(
                    &conn,
                    "error",
                    "sync_acclaimed_failed",
                    &format!("candidates: {}", e),
                )
                .await;
                return;
            }
        }

        if !omdb_key.is_empty() {
            let omdb = OmdbEnrichmentService::new(omdb_key);
            match omdb.enrich_movies(&conn, i64::MAX).await {
                Ok((enriched, total, _)) => {
                    let _ = models::insert_run_log(
                        &conn,
                        "info",
                        "sync_acclaimed_enriched",
                        &format!("enriched {}/{}", enriched, total),
                    )
                    .await;
                }
                Err(e) => tracing::warn!("sync_acclaimed: enrich failed: {}", e),
            }
        } else {
            let _ = models::insert_run_log(
                &conn,
                "warn",
                "sync_acclaimed_enrich_skipped",
                "OMDB_API_KEY not set",
            )
            .await;
        }

        match models::classify_acclaimed_films(&conn, chrono::Utc::now().year()).await {
            Ok(n) => {
                let _ = models::insert_run_log(
                    &conn,
                    "info",
                    "acclaimed_classified",
                    &format!("{} acclaimed films", n),
                )
                .await;
            }
            Err(e) => tracing::warn!("sync_acclaimed: classify failed: {}", e),
        }

        cache.write().await.invalidate();
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "message": "Acclaimed ingestion started in background — watch logs for progress",
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn sync_acclaimed_rejects_missing_token() {
        let headers = HeaderMap::new();
        assert_eq!(
            check_admin_token(&headers, "secret"),
            Err(StatusCode::UNAUTHORIZED)
        );
    }
}
