use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use gem_finder_db::models;
use gem_finder_shared::types::{WatchlistEntry, WatchlistUpsert};

use crate::middleware::auth::AuthUser;
use crate::AppState;

/// `GET /api/watchlist` — return the authenticated user's full watchlist.
pub async fn get_watchlist(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<WatchlistEntry>>, StatusCode> {
    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    models::get_user_watchlist(&conn, &auth.0.sub)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!("get_user_watchlist failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// `GET /api/watchlist/movie/:movie_id` — check a single movie's watchlist state.
/// Returns 200 with the entry if present, 404 if the movie is not on the watchlist.
pub async fn get_watchlist_movie(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(movie_id): Path<i64>,
) -> Result<Json<WatchlistEntry>, StatusCode> {
    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match models::get_watchlist_entry(&conn, &auth.0.sub, movie_id)
        .await
        .map_err(|e| {
            tracing::error!("get_watchlist_entry failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })? {
        Some(entry) => Ok(Json(entry)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// `POST /api/watchlist` — upsert a watchlist entry (add or update state/rating).
pub async fn upsert_watchlist(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<WatchlistUpsert>,
) -> Result<StatusCode, StatusCode> {
    // Validate rating range when state is Watched.
    if let Some(r) = body.user_rating {
        if !(1..=10).contains(&r) {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    models::upsert_watchlist_entry(
        &conn,
        &auth.0.sub,
        body.movie_id,
        body.state.as_str(),
        body.user_rating,
    )
    .await
    .map(|_| StatusCode::NO_CONTENT)
    .map_err(|e| {
        tracing::error!("upsert_watchlist_entry failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })
}

/// `DELETE /api/watchlist/movie/:movie_id` — remove a movie from the watchlist.
pub async fn delete_watchlist_movie(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(movie_id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    models::delete_watchlist_entry(&conn, &auth.0.sub, movie_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            tracing::error!("delete_watchlist_entry failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}
