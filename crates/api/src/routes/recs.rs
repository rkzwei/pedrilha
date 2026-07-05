//! Friend recommendations — Ethos C1 ("my friend recommended me a movie").
//!
//! Public identifiers: rec token (uuid v4 simple, 122 random bits) and
//! usernames. Integer PKs never leave this module. Unknown token, revoked
//! token, and non-sender revoke all return the SAME 404 body — no oracle.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use gem_finder_db::models;
use gem_finder_shared::types::{
    validate_rec_note, FriendInfo, ReceivedRec, RecCreate, RecCreated, RecPublic, SentRec,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::middleware::auth::AuthUser;
use crate::AppState;

/// Uniform 404 — same body for every "no" so tokens can't be probed.
fn rec_not_found() -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "rec_not_found" })),
    )
}

fn internal() -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "database error" })),
    )
}

/// Sliding-window rate limit on the shared in-memory limiter.
/// Returns true when the caller is over the limit.
async fn over_limit(state: &AppState, key: String, max: usize, window_secs: u64) -> bool {
    let mut lim = state.magic_link_limiter.lock().await;
    let now = std::time::Instant::now();
    let win = std::time::Duration::from_secs(window_secs);
    let e = lim.entry(key).or_insert_with(Vec::new);
    e.retain(|t| now.duration_since(*t) < win);
    if e.len() >= max {
        return true;
    }
    e.push(now);
    false
}

/// Trim + validate an optional note; empty becomes None.
fn clean_note(raw: Option<String>) -> Result<Option<String>, &'static str> {
    match raw {
        None => Ok(None),
        Some(s) => {
            let t = s.trim().to_string();
            if t.is_empty() {
                return Ok(None);
            }
            validate_rec_note(&t)?;
            Ok(Some(t))
        }
    }
}

/// POST /api/recs — mint a share-link rec, or send directly to a friend.
pub async fn create_rec(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<RecCreate>,
) -> Result<Json<RecCreated>, (StatusCode, Json<Value>)> {
    if over_limit(&state, format!("rec_send:{}", auth.0.sub), 20, 3600).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "rate limited" })),
        ));
    }

    let note = clean_note(body.note)
        .map_err(|msg| (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": msg }))))?;

    let mut conn = state.db.connect().await.map_err(|_| internal())?;

    // Username gate — read from DB, NOT from JWT claims (claims go stale:
    // a 30-day token minted before the user picked a username says None).
    let sender_username = models::get_username(&conn, &auth.0.sub)
        .await
        .map_err(|_| internal())?;
    if sender_username.is_none() {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "username_required" })),
        ));
    }

    let token = Uuid::new_v4().simple().to_string();

    match body.to_username {
        None => {
            models::create_share_rec(&conn, &auth.0.sub, body.movie_id, note.as_deref(), &token)
                .await
                .map_err(|e| {
                    tracing::error!("create_share_rec failed: {e}");
                    internal()
                })?;
        }
        Some(to) => {
            let recipient_id = models::get_user_id_by_username(&conn, &to)
                .await
                .map_err(|_| internal())?
                .ok_or_else(|| {
                    (StatusCode::FORBIDDEN, Json(json!({ "error": "not_friends" })))
                })?;
            let friends = models::are_friends(&conn, &auth.0.sub, &recipient_id)
                .await
                .map_err(|_| internal())?;
            if !friends {
                // Same error for "no such user" and "not a friend" — no
                // username-existence oracle beyond the public checker.
                return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "not_friends" }))));
            }
            models::create_direct_rec(
                &mut conn,
                &auth.0.sub,
                &recipient_id,
                body.movie_id,
                note.as_deref(),
                &token,
            )
            .await
            .map_err(|e| {
                tracing::error!("create_direct_rec failed: {e}");
                internal()
            })?;
        }
    }

    Ok(Json(RecCreated { token }))
}

/// GET /api/rec/{token} — public landing payload.
pub async fn get_rec(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<RecPublic>, (StatusCode, Json<Value>)> {
    // Per-token limiter: brute force is already hopeless at 2^122, this just
    // keeps scripted scanning from burning DB reads.
    if over_limit(&state, format!("rec_view:{token}"), 30, 600).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "rate limited" })),
        ));
    }
    let conn = state.db.connect().await.map_err(|_| internal())?;
    match models::get_rec_public(&conn, &token).await {
        Ok(Some(rec)) => Ok(Json(rec)),
        Ok(None) => Err(rec_not_found()),
        Err(e) => {
            tracing::error!("get_rec_public failed: {e}");
            Err(internal())
        }
    }
}

/// POST /api/rec/{token}/claim — receipt + friendship. Idempotent.
pub async fn claim_rec(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(token): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    if over_limit(&state, format!("rec_claim:{}", auth.0.sub), 30, 3600).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "rate limited" })),
        ));
    }
    let mut conn = state.db.connect().await.map_err(|_| internal())?;
    match models::claim_rec(&mut conn, &token, &auth.0.sub).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(rec_not_found()),
        Err(e) => {
            tracing::error!("claim_rec failed: {e}");
            Err(internal())
        }
    }
}

/// DELETE /api/rec/{token} — revoke (sender only).
pub async fn revoke_rec(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(token): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    if over_limit(&state, format!("rec_revoke:{}", auth.0.sub), 30, 3600).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": "rate limited" })),
        ));
    }
    let mut conn = state.db.connect().await.map_err(|_| internal())?;
    match models::revoke_rec(&mut conn, &token, &auth.0.sub).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(rec_not_found()),
        Err(e) => {
            tracing::error!("revoke_rec failed: {e}");
            Err(internal())
        }
    }
}

/// POST /api/rec/{token}/read
pub async fn mark_read(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(token): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let conn = state.db.connect().await.map_err(|_| internal())?;
    models::mark_rec_read(&conn, &token, &auth.0.sub)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            tracing::error!("mark_rec_read failed: {e}");
            internal()
        })
}

/// GET /api/recs/received
pub async fn received(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<ReceivedRec>>, (StatusCode, Json<Value>)> {
    let conn = state.db.connect().await.map_err(|_| internal())?;
    models::get_received_recs(&conn, &auth.0.sub)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("get_received_recs failed: {e}");
            internal()
        })
}

/// GET /api/recs/sent
pub async fn sent(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<SentRec>>, (StatusCode, Json<Value>)> {
    let conn = state.db.connect().await.map_err(|_| internal())?;
    models::get_sent_recs(&conn, &auth.0.sub)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("get_sent_recs failed: {e}");
            internal()
        })
}

/// GET /api/recs/unread_count — nav badge.
pub async fn unread_count(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let conn = state.db.connect().await.map_err(|_| internal())?;
    models::unread_rec_count(&conn, &auth.0.sub)
        .await
        .map(|count| Json(json!({ "count": count })))
        .map_err(|e| {
            tracing::error!("unread_rec_count failed: {e}");
            internal()
        })
}

/// GET /api/friends
pub async fn friends(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<FriendInfo>>, (StatusCode, Json<Value>)> {
    let conn = state.db.connect().await.map_err(|_| internal())?;
    models::get_friends(&conn, &auth.0.sub)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("get_friends failed: {e}");
            internal()
        })
}
