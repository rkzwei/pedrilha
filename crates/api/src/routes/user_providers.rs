use axum::{extract::State, http::StatusCode, Json};
use gem_finder_db::models;
use gem_finder_shared::types::UserProvidersPayload;

use crate::middleware::auth::AuthUser;
use crate::AppState;

/// `GET /api/user/providers` — the signed-in user's selected providers per region.
pub async fn get_user_providers(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<UserProvidersPayload>, StatusCode> {
    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rows = models::get_user_providers(&conn, &auth.0.sub)
        .await
        .map_err(|e| {
            tracing::error!("get_user_providers failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut payload = UserProvidersPayload::default();
    for (region, provider_id) in rows {
        if region == "BR" {
            payload.br.push(provider_id);
        } else {
            payload.us.push(provider_id);
        }
    }
    Ok(Json(payload))
}

/// `PUT /api/user/providers` — replace the user's provider selection (last-write-wins).
pub async fn put_user_providers(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UserProvidersPayload>,
) -> Result<StatusCode, StatusCode> {
    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut entries: Vec<(String, i32)> = Vec::new();
    for id in body.us {
        entries.push(("US".to_string(), id));
    }
    for id in body.br {
        entries.push(("BR".to_string(), id));
    }

    models::replace_user_providers(&conn, &auth.0.sub, &entries)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            tracing::error!("replace_user_providers failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}
