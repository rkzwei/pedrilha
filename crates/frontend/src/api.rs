use gem_finder_shared::types::{AuthResponse, Movie, MovieSummary, PaginatedResponse, WatchlistEntry, WatchlistUpsert, WatchState};

/// Returns the current page origin (e.g. `https://example.com`) at runtime.
/// Reqwest in WASM requires absolute URLs; reading the origin from the browser
/// means API calls always resolve against the correct host regardless of environment.
fn api_base() -> String {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_else(|| "http://localhost:3000".to_string())
}

/// Fetch a paginated list of hidden gems.
pub async fn fetch_gems(
    page: i32,
    per_page: i32,
    min_year: Option<i32>,
    genre: Option<String>,
    q: Option<String>,
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let mut url = format!("{}/api/gems?page={}&per_page={}", api_base(), page, per_page);
    if let Some(y) = min_year { url.push_str(&format!("&min_year={}", y)); }
    if let Some(g) = genre    { url.push_str(&format!("&genres={}", g)); }
    if let Some(s) = q        { url.push_str(&format!("&q={}", s)); }
    reqwest::get(&url).await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<PaginatedResponse<MovieSummary>>().await
        .map_err(|e| format!("Parse error: {}", e))
}

/// Fetch a paginated list of acclaimed films (IMDb ≥ 8.0, RT ≥ 80%).
pub async fn fetch_acclaimed(
    page: i32,
    per_page: i32,
    min_year: Option<i32>,
    genre: Option<String>,
    q: Option<String>,
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let mut url = format!("{}/api/acclaimed?page={}&per_page={}", api_base(), page, per_page);
    if let Some(y) = min_year { url.push_str(&format!("&min_year={}", y)); }
    if let Some(g) = genre    { url.push_str(&format!("&genres={}", g)); }
    if let Some(s) = q        { url.push_str(&format!("&q={}", s)); }
    reqwest::get(&url).await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<PaginatedResponse<MovieSummary>>().await
        .map_err(|e| format!("Parse error: {}", e))
}

/// Fetch a paginated list of wildcard films.
pub async fn fetch_wildcards(
    page: i32,
    per_page: i32,
    min_year: Option<i32>,
    genre: Option<String>,
    q: Option<String>,
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let mut url = format!("{}/api/wildcards?page={}&per_page={}", api_base(), page, per_page);
    if let Some(y) = min_year { url.push_str(&format!("&min_year={}", y)); }
    if let Some(g) = genre    { url.push_str(&format!("&genres={}", g)); }
    if let Some(s) = q        { url.push_str(&format!("&q={}", s)); }
    reqwest::get(&url).await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<PaginatedResponse<MovieSummary>>().await
        .map_err(|e| format!("Parse error: {}", e))
}

/// Fetch a single movie by its mv+base36 encoded ID.
pub async fn fetch_movie(encoded_id: &str) -> Result<Movie, String> {
    let url = format!("{}/api/movies/{}", api_base(), encoded_id);
    let resp = reqwest::get(&url).await.map_err(|e| format!("Network error: {}", e))?;
    match resp.status().as_u16() {
        404 => Err("Movie not found".to_string()),
        400 => Err("Invalid movie ID".to_string()),
        _   => resp.json::<Movie>().await.map_err(|e| format!("Parse error: {}", e)),
    }
}

/// POST /api/admin/seed — full pipeline in background.
pub async fn admin_seed(token: &str) -> Result<serde_json::Value, String> {
    admin_post("/api/admin/seed", serde_json::json!({}), token).await
}

/// POST /api/admin/sync — TMDB sync in background.
pub async fn admin_sync(token: &str) -> Result<serde_json::Value, String> {
    admin_post("/api/admin/sync", serde_json::json!({}), token).await
}

/// POST /api/admin/enrich — OMDb enrichment in background.
pub async fn admin_enrich(limit: i64, token: &str) -> Result<serde_json::Value, String> {
    admin_post("/api/admin/enrich", serde_json::json!({ "limit": limit }), token).await
}

/// POST /api/admin/score — batch scoring in background.
pub async fn admin_score(token: &str) -> Result<serde_json::Value, String> {
    admin_post("/api/admin/score", serde_json::json!({}), token).await
}

/// GET /api/admin/logs — recent run log entries.
pub async fn admin_logs(token: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/admin/logs", api_base());
    let mut req = reqwest::Client::new().get(&url);
    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }
    req.send().await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<serde_json::Value>().await
        .map_err(|e| format!("Parse error: {}", e))
}

/// GET /api/admin/status — capability flags (no auth required).
pub async fn fetch_admin_status() -> Result<serde_json::Value, String> {
    let url = format!("{}/api/admin/status", api_base());
    reqwest::get(&url).await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<serde_json::Value>().await
        .map_err(|e| format!("Parse error: {}", e))
}

// ── Auth ──────────────────────────────────────────────────────────────────────

/// `POST /api/auth/magic` — request a magic-link email.
pub async fn send_magic_link(email: &str) -> Result<(), String> {
    let url = format!("{}/api/auth/magic", api_base());
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({ "email": email }))
        .send().await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(resp.text().await.unwrap_or_else(|_| "Unknown error".into()))
    }
}

/// `GET /api/auth/verify?token=<token>` — exchange magic-link token for JWT.
pub async fn verify_token(token: &str) -> Result<AuthResponse, String> {
    let url = format!("{}/api/auth/verify?token={}", api_base(), token);
    let resp = reqwest::get(&url).await.map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        resp.json::<AuthResponse>().await.map_err(|e| format!("Parse error: {}", e))
    } else {
        Err(resp.text().await.unwrap_or_else(|_| "Invalid or expired link".into()))
    }
}

// ── Watchlist ─────────────────────────────────────────────────────────────────

/// `GET /api/watchlist` — return the user's full watchlist.
pub async fn get_watchlist(token: &str) -> Result<Vec<WatchlistEntry>, String> {
    let url = format!("{}/api/watchlist", api_base());
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send().await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        resp.json::<Vec<WatchlistEntry>>().await
            .map_err(|e| format!("Parse error: {}", e))
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// `GET /api/watchlist/movie/:movie_id` — check if a specific movie is on the watchlist.
/// Returns `None` if not on the list (404 from server).
pub async fn get_watchlist_entry(movie_id: i64, token: &str) -> Result<Option<WatchlistEntry>, String> {
    let url = format!("{}/api/watchlist/movie/{}", api_base(), movie_id);
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send().await
        .map_err(|e| format!("Network error: {}", e))?;
    match resp.status().as_u16() {
        200 => resp.json::<WatchlistEntry>().await.map(Some).map_err(|e| format!("Parse error: {}", e)),
        404 => Ok(None),
        _   => Err(format!("Server error: {}", resp.status())),
    }
}

/// `POST /api/watchlist` — upsert a watchlist entry.
pub async fn upsert_watchlist(movie_id: i64, state: WatchState, user_rating: Option<i32>, token: &str) -> Result<(), String> {
    let url = format!("{}/api/watchlist", api_base());
    let resp = reqwest::Client::new()
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&WatchlistUpsert { movie_id, state, user_rating })
        .send().await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() { Ok(()) } else { Err(format!("Server error: {}", resp.status())) }
}

/// `DELETE /api/watchlist/movie/:movie_id` — remove a movie from the watchlist.
pub async fn delete_watchlist(movie_id: i64, token: &str) -> Result<(), String> {
    let url = format!("{}/api/watchlist/movie/{}", api_base(), movie_id);
    let resp = reqwest::Client::new()
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send().await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() { Ok(()) } else { Err(format!("Server error: {}", resp.status())) }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn admin_post(path: &str, body: serde_json::Value, token: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", api_base(), path);
    let mut req = reqwest::Client::new().post(&url).json(&body);
    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }
    req.send().await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<serde_json::Value>().await
        .map_err(|e| format!("Parse error: {}", e))
}
