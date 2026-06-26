use crate::middleware::auth::{create_jwt, AuthUser};
use crate::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use chrono::{Duration, Utc};
use gem_finder_db::models;
use gem_finder_shared::types::{
    AuthResponse, MagicLinkRequest, MagicLinkVerify, UsernameAvailability, UsernameUpdate,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::Mutex;
use uuid::Uuid;
use webauthn_rs::prelude::*;

// ── Challenge state ───────────────────────────────────────────────────────────

/// In-memory store for pending WebAuthn challenges.
/// Keyed by user_id (registration) or a random session key (authentication).
pub type ChallengeStore<T> = std::sync::Arc<Mutex<HashMap<String, (T, Instant)>>>;

const CHALLENGE_TTL_SECS: u64 = 300;

fn purge_expired<T>(map: &mut HashMap<String, (T, Instant)>) {
    let now = Instant::now();
    map.retain(|_, (_, created)| now.duration_since(*created).as_secs() < CHALLENGE_TTL_SECS);
}

// ── Email validation ──────────────────────────────────────────────────────────

fn is_valid_email(email: &str) -> bool {
    // Must have exactly one @
    let mut parts = email.splitn(2, '@');
    let local = parts.next().unwrap_or("");
    let domain = match parts.next() {
        Some(d) => d,
        None => return false,
    };
    if local.is_empty() || domain.is_empty() {
        return false;
    }
    // Domain must contain a dot not at start or end
    if let Some(dot) = domain.rfind('.') {
        if dot == 0 || dot == domain.len() - 1 {
            return false;
        }
        // TLD must be at least 2 chars
        if domain.len() - dot < 3 {
            return false;
        }
    } else {
        return false;
    }
    // Reject obvious junk (consecutive dots, @ in local part handled by splitn)
    !local.contains("..") && !domain.contains("..")
}

// ── Magic link ────────────────────────────────────────────────────────────────

/// POST /api/auth/magic
///
/// Accepts an email address, creates or finds the user, issues a magic token,
/// and sends the link by email. Always returns 200 (avoids email enumeration).
pub async fn magic_link_request(
    State(state): State<AppState>,
    Json(body): Json<MagicLinkRequest>,
) -> (StatusCode, Json<Value>) {
    let email = body.email.trim().to_lowercase();
    if !is_valid_email(&email) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid email address" })),
        );
    }

    // Rate limit: 5 magic links per email per 10 min
    {
        let mut lim = state.magic_link_limiter.lock().await;
        let now = std::time::Instant::now();
        let win = std::time::Duration::from_secs(600);
        let e = lim.entry(email.clone()).or_insert_with(Vec::new);
        e.retain(|t| now.duration_since(*t) < win);
        if e.len() >= 5 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "rate limited" })),
            );
        }
        e.push(now);
    }
    let conn = match state.db.connect().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    };

    let user_id = Uuid::new_v4().to_string();
    let user = match models::find_or_create_user(&conn, &user_id, &email).await {
        Ok(u) => u,
        Err(e) => {
            tracing::error!("find_or_create_user failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            );
        }
    };

    let token = Uuid::new_v4().to_string();
    let expires_at = (Utc::now() + Duration::minutes(15))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    if let Err(e) = models::create_magic_token(&conn, &token, &user.id, &expires_at).await {
        tracing::error!("create_magic_token failed: {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database error" })),
        );
    }

    let next = body.next.clone();
    tokio::spawn(async move {
        if let Err(e) =
            crate::services::email::send_magic_link(&email, &token, next.as_deref()).await
        {
            tracing::warn!("magic link email failed to send: {}", e);
        }
    });

    (StatusCode::OK, Json(json!({ "status": "sent" })))
}

/// GET /api/auth/verify?token=<uuid>
///
/// Validates the magic token, marks it consumed, and returns a JWT.
pub async fn magic_link_verify(
    State(state): State<AppState>,
    Query(params): Query<MagicLinkVerify>,
) -> (StatusCode, Json<Value>) {
    if params.token.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "token is required" })),
        );
    }

    // Rate limit: 5 attempts per token per 10 min (prevent hammering)
    {
        let mut lim = state.magic_link_limiter.lock().await;
        let now = std::time::Instant::now();
        let win = std::time::Duration::from_secs(600);
        let e = lim.entry(params.token.clone()).or_insert_with(Vec::new);
        e.retain(|t| now.duration_since(*t) < win);
        if e.len() >= 5 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "rate limited" })),
            );
        }
        e.push(now);
    }

    let conn = match state.db.connect().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    };

    let user = match models::consume_magic_token(&conn, &params.token).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "token is invalid, expired, or already used" })),
            )
        }
        Err(e) => {
            tracing::error!("consume_magic_token failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            );
        }
    };

    let _ = models::touch_user_login(&conn, &user.id).await;

    let is_admin = state.admin_emails.contains(&user.email.to_lowercase());
    let jwt = match create_jwt(
        &user.id,
        &user.email,
        user.username.as_deref(),
        &state.jwt_secret,
        is_admin,
    ) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("JWT creation failed: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to create session" })),
            );
        }
    };

    (
        StatusCode::OK,
        Json(
            serde_json::to_value(AuthResponse {
                token: jwt,
                user_id: user.id,
                email: user.email,
            })
            .unwrap_or_default(),
        ),
    )
}

// ── Username ──────────────────────────────────────────────────────────────────

/// GET /api/user/username/check?username=<name>
pub async fn check_username(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let username = match params.get("username") {
        Some(u) => u.clone(),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "username query param required" })),
            )
        }
    };

    let check = UsernameUpdate {
        username: username.clone(),
    };
    if let Err(msg) = check.validate() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": msg })),
        );
    }

    // Rate limit: 5 checks per username per 10 min (prevent enumeration)
    {
        let mut lim = state.magic_link_limiter.lock().await;
        let now = std::time::Instant::now();
        let win = std::time::Duration::from_secs(600);
        let e = lim.entry(username.clone()).or_insert_with(Vec::new);
        e.retain(|t| now.duration_since(*t) < win);
        if e.len() >= 5 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "rate limited" })),
            );
        }
        e.push(now);
    }

    let conn = match state.db.connect().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    };

    match models::username_available(&conn, &username).await {
        Ok(available) => (
            StatusCode::OK,
            Json(
                serde_json::to_value(UsernameAvailability {
                    username,
                    available,
                })
                .unwrap_or_default(),
            ),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database error" })),
        ),
    }
}

/// GET /api/user/me — return the authenticated user's profile from JWT claims.
pub async fn get_me(auth: AuthUser) -> Json<Value> {
    Json(json!({
        "user_id":  auth.0.sub,
        "email":    auth.0.email,
        "username": auth.0.username,
    }))
}

/// PATCH /api/user/username
pub async fn set_username(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UsernameUpdate>,
) -> (StatusCode, Json<Value>) {
    if let Err(msg) = body.validate() {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({ "error": msg })),
        );
    }

    // Rate limit: 5 updates per user per 10 min
    {
        let mut lim = state.magic_link_limiter.lock().await;
        let now = std::time::Instant::now();
        let win = std::time::Duration::from_secs(600);
        let e = lim.entry(auth.0.email.clone()).or_insert_with(Vec::new);
        e.retain(|t| now.duration_since(*t) < win);
        if e.len() >= 5 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "rate limited" })),
            );
        }
        e.push(now);
    }

    let conn = match state.db.connect().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    };

    match models::username_available(&conn, &body.username).await {
        Ok(false) => {
            return (
                StatusCode::CONFLICT,
                Json(json!({ "error": "username already taken" })),
            )
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
        Ok(true) => {}
    }

    match models::set_username(&conn, &auth.0.sub, &body.username).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "username": body.username }))),
        Err(e) => {
            tracing::error!("set_username failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    }
}

// ── Passkeys — WebAuthn ───────────────────────────────────────────────────────

/// POST /api/auth/passkey/register/start
///
/// JWT required. Returns a WebAuthn creation challenge for the browser to pass
/// to `navigator.credentials.create()`.
pub async fn passkey_register_start(
    State(state): State<AppState>,
    auth: AuthUser,
) -> (StatusCode, Json<Value>) {
    let user_uuid = match Uuid::parse_str(&auth.0.sub) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "invalid user id" })),
            )
        }
    };

    match state
        .webauthn
        .start_passkey_registration(user_uuid, &auth.0.email, &auth.0.email, None)
    {
        Ok((ccr, reg_state)) => {
            let mut store = state.passkey_reg_challenges.lock().await;
            purge_expired(&mut store);
            store.insert(auth.0.sub.clone(), (reg_state, Instant::now()));
            (
                StatusCode::OK,
                Json(serde_json::to_value(ccr).unwrap_or_default()),
            )
        }
        Err(e) => {
            tracing::error!("passkey register start failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to start registration" })),
            )
        }
    }
}

/// POST /api/auth/passkey/register/finish
///
/// JWT required. Completes registration and stores the credential.
pub async fn passkey_register_finish(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(reg): Json<RegisterPublicKeyCredential>,
) -> (StatusCode, Json<Value>) {
    let reg_state = {
        let mut store = state.passkey_reg_challenges.lock().await;
        store.remove(&auth.0.sub)
    };

    let (reg_state, created_at) = match reg_state {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "no pending registration challenge" })),
            )
        }
    };

    if Instant::now().duration_since(created_at).as_secs() >= CHALLENGE_TTL_SECS {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "registration challenge expired" })),
        );
    }

    let passkey = match state.webauthn.finish_passkey_registration(&reg, &reg_state) {
        Ok(pk) => pk,
        Err(e) => {
            tracing::warn!("passkey register finish failed: {}", e);
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "registration verification failed" })),
            );
        }
    };

    let credential_id = serde_json::to_value(passkey.cred_id())
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let passkey_json = match serde_json::to_string(&passkey) {
        Ok(j) => j,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to serialize passkey" })),
            )
        }
    };

    // Rate limit: 5 registrations per user per 10 min
    {
        let mut lim = state.magic_link_limiter.lock().await;
        let now = std::time::Instant::now();
        let win = std::time::Duration::from_secs(600);
        let e = lim.entry(auth.0.email.clone()).or_insert_with(Vec::new);
        e.retain(|t| now.duration_since(*t) < win);
        if e.len() >= 5 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "rate limited" })),
            );
        }
        e.push(now);
    }

    let conn = match state.db.connect().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    };

    match models::store_passkey(&conn, &auth.0.sub, &credential_id, &passkey_json, None).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "status": "registered" }))),
        Err(e) => {
            tracing::error!("store_passkey failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    }
}

/// POST /api/auth/passkey/authenticate/start
///
/// No JWT required. Accepts `{ "email": "..." }` and returns a challenge.
pub async fn passkey_auth_start(
    State(state): State<AppState>,
    Json(body): Json<MagicLinkRequest>,
) -> (StatusCode, Json<Value>) {
    let email = body.email.trim().to_lowercase();

    // Rate limit: 5 magic links per email per 10 min
    {
        let mut lim = state.magic_link_limiter.lock().await;
        let now = std::time::Instant::now();
        let win = std::time::Duration::from_secs(600);
        let e = lim.entry(email.clone()).or_insert_with(Vec::new);
        e.retain(|t| now.duration_since(*t) < win);
        if e.len() >= 5 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "rate limited" })),
            );
        }
        e.push(now);
    }
    let conn = match state.db.connect().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    };

    let user = match models::find_user_by_email(&conn, &email).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            // Don't reveal whether the email exists
            return (
                StatusCode::OK,
                Json(json!({ "error": "no passkeys registered" })),
            );
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    };

    let passkey_jsons = match models::get_user_passkey_jsons(&conn, &user.id).await {
        Ok(j) => j,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    };

    if passkey_jsons.is_empty() {
        return (
            StatusCode::OK,
            Json(json!({ "error": "no passkeys registered" })),
        );
    }

    let passkeys: Vec<Passkey> = passkey_jsons
        .iter()
        .filter_map(|j| serde_json::from_str(j).ok())
        .collect();

    match state.webauthn.start_passkey_authentication(&passkeys) {
        Ok((rcr, auth_state)) => {
            let session_key = Uuid::new_v4().to_string();
            let mut store = state.passkey_auth_challenges.lock().await;
            purge_expired(&mut store);
            store.insert(session_key.clone(), (auth_state, Instant::now()));
            (
                StatusCode::OK,
                Json(json!({
                    "challenge": serde_json::to_value(rcr).unwrap_or_default(),
                    "session_key": session_key,
                    "user_id": user.id,
                })),
            )
        }
        Err(e) => {
            tracing::error!("passkey auth start failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to start authentication" })),
            )
        }
    }
}

#[derive(serde::Deserialize)]
pub struct PasskeyAuthFinishBody {
    pub session_key: String,
    pub user_id: String,
    pub credential: PublicKeyCredential,
}

/// POST /api/auth/passkey/authenticate/finish
///
/// Verifies the passkey assertion, updates the sign count, and issues a JWT.
pub async fn passkey_auth_finish(
    State(state): State<AppState>,
    Json(body): Json<PasskeyAuthFinishBody>,
) -> (StatusCode, Json<Value>) {
    let auth_state = {
        let mut store = state.passkey_auth_challenges.lock().await;
        store.remove(&body.session_key)
    };

    let (auth_state, created_at) = match auth_state {
        Some(s) => s,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "no pending authentication challenge" })),
            )
        }
    };

    if Instant::now().duration_since(created_at).as_secs() >= CHALLENGE_TTL_SECS {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "authentication challenge expired" })),
        );
    }

    let auth_result = match state
        .webauthn
        .finish_passkey_authentication(&body.credential, &auth_state)
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("passkey auth finish failed: {}", e);
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "authentication failed" })),
            );
        }
    };

    let conn = match state.db.connect().await {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "database error" })),
            )
        }
    };

    // Rate limit: 5 auth attempts per user_id per 10 min
    {
        let mut lim = state.magic_link_limiter.lock().await;
        let now = std::time::Instant::now();
        let win = std::time::Duration::from_secs(600);
        let e = lim.entry(body.user_id.clone()).or_insert_with(Vec::new);
        e.retain(|t| now.duration_since(*t) < win);
        if e.len() >= 5 {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "rate limited" })),
            );
        }
        e.push(now);
    }

    let cred_id = serde_json::to_value(auth_result.cred_id())
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();
    let _ = models::update_passkey_sign_count(&conn, &cred_id, auth_result.counter() as i64).await;

    let user = match models::find_user_by_id(&conn, &body.user_id).await {
        Ok(Some(u)) => u,
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "user not found" })),
            )
        }
    };

    let _ = models::touch_user_login(&conn, &user.id).await;

    let is_admin = state.admin_emails.contains(&user.email.to_lowercase());
    let jwt = match create_jwt(
        &user.id,
        &user.email,
        user.username.as_deref(),
        &state.jwt_secret,
        is_admin,
    ) {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to create session" })),
            )
        }
    };

    (
        StatusCode::OK,
        Json(
            serde_json::to_value(AuthResponse {
                token: jwt,
                user_id: user.id,
                email: user.email,
            })
            .unwrap_or_default(),
        ),
    )
}
