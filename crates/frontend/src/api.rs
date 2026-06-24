use gem_finder_shared::types::{Movie, MovieSummary, PaginatedResponse};

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
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let mut url = format!("{}/api/gems?page={}&per_page={}", API_BASE, page, per_page);
    if let Some(y) = min_year {
        url.push_str(&format!("&min_year={}", y));
    }
    if let Some(g) = genre {
        url.push_str(&format!("&genre={}", g));
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
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let url = format!(
        "{}/api/acclaimed?page={}&per_page={}",
        API_BASE, page, per_page
    );

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
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let url = format!(
        "{}/api/wildcards?page={}&per_page={}",
        API_BASE, page, per_page
    );

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
