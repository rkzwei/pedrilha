use gem_finder_shared::types::{AuthResponse, Movie, MovieSummary, PaginatedResponse, WatchlistEntry, WatchlistUpsert, WatchState};

/// Base URL for the API server.
/// In development, this is the Axum backend running on localhost:3000.
/// In production, this should be set via environment or build-time config.
const API_BASE: &str = "http://localhost:3000";

/// Fetch a paginated list of hidden gems.
pub async fn fetch_gems(
    page: i32,
    per_page: i32,
    min_year: Option<i32>,
    genre: Option<String>,
    q: Option<String>,
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let mut url = format!("{}/api/gems?page={}&per_page={}", API_BASE, page, per_page);
    if let Some(y) = min_year {
        url.push_str(&format!("&min_year={}", y));
    }
    if let Some(g) = genre {
        url.push_str(&format!("&genre={}", g));
    }
    if let Some(s) = q {
        url.push_str(&format!("&q={}", s));
    }

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    let data = response
        .json::<PaginatedResponse<MovieSummary>>()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(data)
}

/// Fetch a paginated list of acclaimed films (IMDb ≥ 8.0, RT ≥ 80%).
pub async fn fetch_acclaimed(
    page: i32,
    per_page: i32,
    min_year: Option<i32>,
    genre: Option<String>,
    q: Option<String>,
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let mut url = format!(
        "{}/api/acclaimed?page={}&per_page={}",
        API_BASE, page, per_page
    );
    if let Some(y) = min_year {
        url.push_str(&format!("&min_year={}", y));
    }
    if let Some(g) = genre {
        url.push_str(&format!("&genre={}", g));
    }
    if let Some(s) = q {
        url.push_str(&format!("&q={}", s));
    }

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    let data = response
        .json::<PaginatedResponse<MovieSummary>>()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(data)
}

/// Fetch a paginated list of wildcard films.
pub async fn fetch_wildcards(
    page: i32,
    per_page: i32,
    min_year: Option<i32>,
    genre: Option<String>,
    q: Option<String>,
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let mut url = format!(
        "{}/api/wildcards?page={}&per_page={}",
        API_BASE, page, per_page
    );
    if let Some(y) = min_year {
        url.push_str(&format!("&min_year={}", y));
    }
    if let Some(g) = genre {
        url.push_str(&format!("&genre={}", g));
    }
    if let Some(s) = q {
        url.push_str(&format!("&q={}", s));
    }

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    let data = response
        .json::<PaginatedResponse<MovieSummary>>()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(data)
}

/// Fetch a single movie by its mv+base36 encoded ID.
/// The encoded_id is the full `mv`-prefixed string (e.g. "mv16").
pub async fn fetch_movie(encoded_id: &str) -> Result<Movie, String> {
    let url = format!("{}/api/movies/{}", API_BASE, encoded_id);

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if response.status().as_u16() == 404 {
        return Err("Movie not found".to_string());
    }
    if response.status().as_u16() == 400 {
        return Err("Invalid movie ID".to_string());
    }

    let movie = response
        .json::<Movie>()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(movie)
}

/// POST /api/admin/seed — full pipeline in background.
pub async fn admin_seed() -> Result<serde_json::Value, String> {
    admin_post("/api/admin/seed", serde_json::json!({})).await
}

/// POST /api/admin/sync — TMDB sync in background.
pub async fn admin_sync() -> Result<serde_json::Value, String> {
    admin_post("/api/admin/sync", serde_json::json!({})).await
}

/// POST /api/admin/enrich — OMDb enrichment in background.
pub async fn admin_enrich(limit: i64) -> Result<serde_json::Value, String> {
    admin_post("/api/admin/enrich", serde_json::json!({ "limit": limit })).await
}

/// POST /api/admin/score — batch scoring in background.
pub async fn admin_score() -> Result<serde_json::Value, String> {
    admin_post("/api/admin/score", serde_json::json!({})).await
}

/// GET /api/admin/logs — recent run log entries.
pub async fn admin_logs() -> Result<serde_json::Value, String> {
    let url = format!("{}/api/admin/logs", API_BASE);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

// ── Auth ──────────────────────────────────────────────────────────────────────

/// `POST /api/auth/magic` — request a magic-link email.
pub async fn send_magic_link(email: &str) -> Result<(), String> {
    let url = format!("{}/api/auth/magic", API_BASE);
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "email": email }))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        let msg = resp.text().await.unwrap_or_else(|_| "Unknown error".into());
        Err(msg)
    }
}

/// `GET /api/auth/verify?token=<token>` — exchange magic-link token for JWT.
pub async fn verify_token(token: &str) -> Result<AuthResponse, String> {
    let url = format!("{}/api/auth/verify?token={}", API_BASE, token);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        resp.json::<AuthResponse>()
            .await
            .map_err(|e| format!("Parse error: {}", e))
    } else {
        let msg = resp.text().await.unwrap_or_else(|_| "Invalid or expired link".into());
        Err(msg)
    }
}

// ── Watchlist ─────────────────────────────────────────────────────────────────

/// `GET /api/watchlist/movie/:movie_id` — check if a specific movie is on the user's watchlist.
/// Returns `None` if not on the list (404 from server).
pub async fn get_watchlist_entry(
    movie_id: i64,
    token: &str,
) -> Result<Option<WatchlistEntry>, String> {
    let url = format!("{}/api/watchlist/movie/{}", API_BASE, movie_id);
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    match resp.status().as_u16() {
        200 => resp
            .json::<WatchlistEntry>()
            .await
            .map(Some)
            .map_err(|e| format!("Parse error: {}", e)),
        404 => Ok(None),
        _ => Err(format!("Server error: {}", resp.status())),
    }
}

/// `POST /api/watchlist` — upsert a watchlist entry.
pub async fn upsert_watchlist(
    movie_id: i64,
    state: WatchState,
    user_rating: Option<i32>,
    token: &str,
) -> Result<(), String> {
    let url = format!("{}/api/watchlist", API_BASE);
    let client = reqwest::Client::new();
    let body = WatchlistUpsert { movie_id, state, user_rating };
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// `DELETE /api/watchlist/movie/:movie_id` — remove a movie from the watchlist.
pub async fn delete_watchlist(movie_id: i64, token: &str) -> Result<(), String> {
    let url = format!("{}/api/watchlist/movie/{}", API_BASE, movie_id);
    let client = reqwest::Client::new();
    let resp = client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Shared helper for admin POST endpoints that return JSON.
async fn admin_post(path: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", API_BASE, path);
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}
