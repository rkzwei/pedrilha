use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use gem_finder_db::models;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::Instant;

use crate::AppState;

/// Valid era values — must match DECADE_OPTIONS in the frontend FilterBar.
const VALID_ERAS: &[i32] = &[1960, 1970, 1980, 1990, 2000, 2010, 2020];

/// Rate limit: max events per session_hash per sliding window.
const RATE_LIMIT_MAX: usize = 60;
const RATE_LIMIT_WINDOW_SECS: u64 = 60;

#[derive(Deserialize)]
pub struct TrackEventRequest {
    pub event_type: String,
    /// Encoded movie ID (e.g. "mv16"). Required when event_type == "movie_view".
    pub movie_id: Option<String>,
    pub genre: Option<String>,
    pub era: Option<i32>,
    /// One of: "gems", "acclaimed", "wildcards"
    pub section: Option<String>,
    pub page_num: Option<i32>,
}

/// POST /api/event — record an anonymous engagement event.
///
/// Caller: WASM frontend. Fire-and-forget — always returns quickly.
/// No auth required; no PII stored. Session identity is a daily-rotated SHA-256 hash
/// of IP + User-Agent so the same visitor gets a different hash each day.
///
/// Returns 204 on success, 400 on invalid payload, 429 on rate-limit breach.
pub async fn track_event(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<TrackEventRequest>,
) -> StatusCode {
    // ── 1. Validate event_type ───────────────────────────────────────────────
    let valid_types = [
        "movie_view",
        "search_used",
        "filter_genre",
        "filter_era",
        "pagination",
        "section_view",
    ];
    if !valid_types.contains(&body.event_type.as_str()) {
        return StatusCode::BAD_REQUEST;
    }

    // ── 2. movie_id is required for movie_view ───────────────────────────────
    let mut decoded_movie_id: Option<i64> = None;
    if body.event_type == "movie_view" {
        match body.movie_id.as_deref() {
            Some(enc) => match gem_finder_shared::id_encode::decode_movie_id(enc) {
                Some(id) => decoded_movie_id = Some(id),
                None => return StatusCode::BAD_REQUEST,
            },
            None => return StatusCode::BAD_REQUEST,
        }
    } else if let Some(ref enc) = body.movie_id {
        // Optional movie_id on other events — decode if present, reject if malformed
        match gem_finder_shared::id_encode::decode_movie_id(enc) {
            Some(id) => decoded_movie_id = Some(id),
            None => return StatusCode::BAD_REQUEST,
        }
    }

    // ── 3. Validate optional fields ──────────────────────────────────────────
    if let Some(ref g) = body.genre {
        if g.len() > 50 {
            return StatusCode::BAD_REQUEST;
        }
    }
    if let Some(era) = body.era {
        if !VALID_ERAS.contains(&era) {
            return StatusCode::BAD_REQUEST;
        }
    }
    if let Some(ref s) = body.section {
        if !["gems", "acclaimed", "wildcards"].contains(&s.as_str()) {
            return StatusCode::BAD_REQUEST;
        }
    }
    if let Some(p) = body.page_num {
        if !(1..=1000).contains(&p) {
            return StatusCode::BAD_REQUEST;
        }
    }

    // ── 4. Compute daily-rotating session hash ───────────────────────────────
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let raw = format!("{}|{}|{}", ip, ua, today);
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    let session_hash = hex::encode(hasher.finalize());

    // ── 5. Rate limit by session_hash ────────────────────────────────────────
    {
        let mut limiter = state.event_rate_limiter.lock().await;
        let window = std::time::Duration::from_secs(RATE_LIMIT_WINDOW_SECS);
        let now = Instant::now();
        let timestamps = limiter.entry(session_hash.clone()).or_default();
        timestamps.retain(|t: &Instant| now.duration_since(*t) < window);
        if timestamps.len() >= RATE_LIMIT_MAX {
            return StatusCode::TOO_MANY_REQUESTS;
        }
        timestamps.push(now);
    }

    // ── 6. Persist to DB ─────────────────────────────────────────────────────
    let conn = match state.db.connect().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("track_event: db connect failed: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR;
        }
    };

    if let Err(e) = models::insert_event(
        &conn,
        &body.event_type,
        decoded_movie_id,
        body.genre.as_deref(),
        body.era,
        body.section.as_deref(),
        body.page_num,
        &session_hash,
    )
    .await
    {
        tracing::warn!("track_event: insert failed: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    StatusCode::NO_CONTENT
}
