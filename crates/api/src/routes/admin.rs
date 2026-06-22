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

#[derive(Deserialize)]
pub struct SyncQuery {
    pub start_year: Option<i32>,
}

#[derive(Deserialize)]
pub struct EnrichQuery {
    pub limit: Option<i64>,
}

/// Minimal bearer-token guard.
/// Reads ADMIN_TOKEN from the environment; rejects requests that don't match.
/// Returns Err(UNAUTHORIZED) if the token is missing or wrong.
fn check_admin_token(headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected = env::var("ADMIN_TOKEN").unwrap_or_default();
    if expected.is_empty() {
        // ADMIN_TOKEN not configured — open access (dev mode). Log a warning.
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

/// Log a run event to the database (best-effort, non-fatal on error).
async fn log_event(conn: &turso::Connection, level: &str, event_type: &str, message: &str) {
    if let Err(e) = models::insert_run_log(conn, level, event_type, message).await {
        tracing::warn!("Failed to write run log: {}", e);
    }
}

/// POST /api/admin/sync
///
/// Triggers a three-phase sync:
/// 1. Hidden gem candidates (rated 6.0–8.0, ≥500 votes)
/// 2. Blockbusters (top by popularity, ≥100k votes) → written to big_hits table
///    so the "obscured by big hit" signal has real data to work with.
/// 3. Seed known gems (The Sorcerer, Dinner in America, The Hurt Locker)
pub async fn trigger_sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SyncQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    check_admin_token(&headers)?;

    let mut sync_service = TmdbSyncService::new().map_err(|e| {
        tracing::error!("Failed to create TMDB sync service: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Log sync start
    log_event(&conn, "info", "sync_started", "TMDB sync pipeline starting").await;

    sync_service.init_config().await.map_err(|e| {
        tracing::error!("Failed to init TMDB config: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let start_year = payload.start_year.unwrap_or(1990);

    // Phase A: gem candidates
    let phase_a_start = std::time::Instant::now();
    sync_service
        .sync_movies(&conn, start_year, None)
        .await
        .map_err(|e| {
            tracing::error!("Gem candidate sync failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let phase_a_duration = phase_a_start.elapsed();
    log_event(
        &conn,
        "info",
        "sync_phase_a_complete",
        &format!(
            "Gem candidate sync from {} completed in {:?}",
            start_year, phase_a_duration
        ),
    )
    .await;

    // Phase B: blockbusters → big_hits
    let phase_b_start = std::time::Instant::now();
    let blockbusters_synced = sync_service.sync_blockbusters(&conn).await.map_err(|e| {
        tracing::error!("Blockbuster sync failed: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let phase_b_duration = phase_b_start.elapsed();
    log_event(
        &conn,
        "info",
        "sync_phase_b_complete",
        &format!(
            "Blockbuster sync complete: {} records, took {:?}",
            blockbusters_synced, phase_b_duration
        ),
    )
    .await;

    // Phase C: ensure the algorithm's ground-truth validation set is in the DB
    let phase_c_start = std::time::Instant::now();
    let (gems_seeded, gems_total, gem_results) =
        sync_service.seed_known_gems(&conn).await.map_err(|e| {
            tracing::error!("Seeding known gems failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    let phase_c_duration = phase_c_start.elapsed();
    log_event(
        &conn,
        "info",
        "sync_phase_c_complete",
        &format!(
            "Seeded {}/{} known gems in {:?}",
            gems_seeded, gems_total, phase_c_duration
        ),
    )
    .await;

    // Log sync complete
    log_event(
        &conn,
        "info",
        "sync_complete",
        &format!(
            "TMDB sync complete: phase_a={:?}, phase_b={:?}, phase_c={:?}",
            phase_a_duration, phase_b_duration, phase_c_duration
        ),
    )
    .await;

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Sync completed",
        "start_year": start_year,
        "blockbusters_synced": blockbusters_synced,
        "known_gems_seeded": gems_seeded,
        "known_gems_total": gems_total,
        "gem_results": gem_results.into_iter().map(|(title, result)| {
            serde_json::json!({ "title": title, "result": result })
        }).collect::<Vec<_>>(),
    })))
}

/// POST /api/admin/enrich
///
/// Enriches movies with IMDb ratings and Rotten Tomatoes scores via OMDb API.
/// Requires OMDB_API_KEY env var to be set.
pub async fn trigger_enrich(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<EnrichQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    check_admin_token(&headers)?;

    let enrich_service = OmdbEnrichmentService::new().map_err(|e| {
        tracing::error!("Failed to create OMDb enrichment service: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let limit = payload.limit.unwrap_or(100);
    log_event(
        &conn,
        "info",
        "enrich_started",
        &format!("OMDb enrichment starting (limit: {})", limit),
    )
    .await;

    let start = std::time::Instant::now();
    let (enriched, total, errors) =
        enrich_service
            .enrich_movies(&conn, limit)
            .await
            .map_err(|e| {
                tracing::error!("OMDb enrichment failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    let duration = start.elapsed();

    log_event(
        &conn,
        "info",
        "enrich_complete",
        &format!(
            "OMDb enrichment: {}/{} enriched, {} errors, took {:?}",
            enriched,
            total,
            errors.len(),
            duration
        ),
    )
    .await;

    let error_details: Vec<String> = errors.into_iter().take(10).collect(); // show first 10 errors

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Enrichment completed",
        "enriched": enriched,
        "total_candidates": total,
        "errors_count": error_details.len(),
        "errors": error_details,
        "duration_secs": duration.as_secs_f64(),
    })))
}

/// GET /api/admin/logs
///
/// Returns the most recent run log entries.
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

    Ok(Json(serde_json::json!({
        "status": "success",
        "logs": logs.into_iter().map(|entry| {
            serde_json::json!({
                "id": entry.id,
                "level": entry.level,
                "event_type": entry.event_type,
                "message": entry.message,
                "created_at": entry.created_at,
            })
        }).collect::<Vec<_>>(),
    })))
}
