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

/// POST /api/admin/sync — trigger TMDB sync pipeline.
pub async fn admin_sync() -> Result<serde_json::Value, String> {
    let url = format!("{}/api/admin/sync", API_BASE);
    let response = reqwest::Client::new()
        .post(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// POST /api/admin/enrich — run OMDb enrichment.
pub async fn admin_enrich(limit: i64) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/admin/enrich", API_BASE);
    let response = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "application/json")
        .body(format!(r#"{{"limit":{}}}"#, limit))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// POST /api/admin/score — run gem scoring.
pub async fn admin_score() -> Result<serde_json::Value, String> {
    let url = format!("{}/api/admin/score", API_BASE);
    let response = reqwest::Client::new()
        .post(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// GET /api/admin/logs — fetch recent run logs.
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

/// Fetch a single movie by its ID.
pub async fn fetch_movie(id: i64) -> Result<Movie, String> {
    let url = format!("{}/api/movies/{}", API_BASE, id);

    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if response.status().is_success() {
        let movie = response
            .json::<Movie>()
            .await
            .map_err(|e| format!("Parse error: {}", e))?;
        Ok(movie)
    } else {
        Err(format!("Movie not found (status: {})", response.status()))
    }
}
