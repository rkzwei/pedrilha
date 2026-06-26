use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// JWT claims embedded in every session token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the user's UUID.
    pub sub: String,
    pub email: String,
    pub username: Option<String>,
    /// Whether this user has admin privileges (checked against ADMIN_EMAILS at mint time).
    #[serde(default)]
    pub is_admin: bool,
    /// Expiry (Unix timestamp seconds).
    pub exp: usize,
    /// Issued-at (Unix timestamp seconds).
    pub iat: usize,
}

/// JWT session duration: 30 days.
const JWT_EXPIRY_SECS: u64 = 30 * 24 * 3600;

/// Create a signed JWT for a user.
pub fn create_jwt(
    user_id: &str,
    email: &str,
    username: Option<&str>,
    secret: &str,
    is_admin: bool,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id.to_owned(),
        email: email.to_owned(),
        username: username.map(|s| s.to_owned()),
        is_admin,
        exp: now + JWT_EXPIRY_SECS as usize,
        iat: now,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Decode and validate a JWT. Returns the claims on success.
pub fn verify_jwt(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    // Explicitly require exp so a malformed type cannot bypass validation (CVE jsonwebtoken <10.3.0)
    let mut validation = Validation::default();
    validation.required_spec_claims = std::collections::HashSet::from(["exp".to_string()]);
    let decoded = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;
    Ok(decoded.claims)
}

/// Axum extractor that validates the `Authorization: Bearer <token>` header
/// and injects the decoded claims into the handler.
///
/// Use as a handler parameter for any route that requires authentication:
/// ```rust
/// async fn my_handler(auth: AuthUser, ...) { ... }
/// ```
#[derive(Debug, Clone)]
pub struct AuthUser(pub Claims);

impl FromRequestParts<crate::AppState> for AuthUser {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &crate::AppState,
    ) -> Result<Self, Self::Rejection> {
        let reject = |msg: &'static str| (StatusCode::UNAUTHORIZED, Json(json!({ "error": msg })));

        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| reject("missing Authorization header"))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| reject("Authorization header must be 'Bearer <token>'"))?;

        let claims =
            verify_jwt(token, &state.jwt_secret).map_err(|_| reject("invalid or expired token"))?;

        Ok(AuthUser(claims))
    }
}
