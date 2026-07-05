use gem_finder_shared::types::{
    AuthResponse, Movie, MovieSummary, PaginatedResponse, ProviderInfo, UserProvidersPayload,
    WatchState, WatchlistEntry, WatchlistUpsert,
};
use serde::Serialize;

/// The active "What can I watch?" filter for a list request: region, csv of
/// selected TMDB provider ids, and whether rentals are included. `None` = no filter.
#[derive(Clone, Default)]
pub struct WatchQuery {
    pub region: String,
    pub providers: String,
    pub rentals: bool,
}

/// Append watch-filter query params to a list URL when a filter is active.
fn append_watch(url: &mut String, watch: &Option<WatchQuery>) {
    if let Some(w) = watch {
        if !w.providers.is_empty() || w.rentals {
            url.push_str(&format!("&region={}", w.region));
            if !w.providers.is_empty() {
                url.push_str(&format!("&providers={}", w.providers));
            }
            if w.rentals {
                url.push_str("&rentals=1");
            }
        }
    }
}

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
    sort: Option<String>,
    sort_dir: Option<String>,
    watch: Option<WatchQuery>,
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let mut url = format!(
        "{}/api/gems?page={}&per_page={}",
        api_base(),
        page,
        per_page
    );
    if let Some(y) = min_year {
        url.push_str(&format!("&min_year={}", y));
    }
    if let Some(g) = genre {
        url.push_str(&format!("&genres={}", g));
    }
    if let Some(s) = q {
        url.push_str(&format!("&q={}", s));
    }
    if let Some(s) = sort {
        url.push_str(&format!("&sort={}", s));
    }
    if let Some(d) = sort_dir {
        url.push_str(&format!("&sort_dir={}", d));
    }
    append_watch(&mut url, &watch);
    reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<PaginatedResponse<MovieSummary>>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// Fetch a paginated list of acclaimed films (IMDb ≥ 8.0, RT ≥ 80%).
pub async fn fetch_acclaimed(
    page: i32,
    per_page: i32,
    min_year: Option<i32>,
    genre: Option<String>,
    q: Option<String>,
    sort: Option<String>,
    sort_dir: Option<String>,
    watch: Option<WatchQuery>,
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let mut url = format!(
        "{}/api/acclaimed?page={}&per_page={}",
        api_base(),
        page,
        per_page
    );
    if let Some(y) = min_year {
        url.push_str(&format!("&min_year={}", y));
    }
    if let Some(g) = genre {
        url.push_str(&format!("&genres={}", g));
    }
    if let Some(s) = q {
        url.push_str(&format!("&q={}", s));
    }
    if let Some(s) = sort {
        url.push_str(&format!("&sort={}", s));
    }
    if let Some(d) = sort_dir {
        url.push_str(&format!("&sort_dir={}", d));
    }
    append_watch(&mut url, &watch);
    reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<PaginatedResponse<MovieSummary>>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// Fetch a paginated list of wildcard films.
pub async fn fetch_wildcards(
    page: i32,
    per_page: i32,
    min_year: Option<i32>,
    genre: Option<String>,
    q: Option<String>,
    sort: Option<String>,
    sort_dir: Option<String>,
    watch: Option<WatchQuery>,
) -> Result<PaginatedResponse<MovieSummary>, String> {
    let mut url = format!(
        "{}/api/wildcards?page={}&per_page={}",
        api_base(),
        page,
        per_page
    );
    if let Some(y) = min_year {
        url.push_str(&format!("&min_year={}", y));
    }
    if let Some(g) = genre {
        url.push_str(&format!("&genres={}", g));
    }
    if let Some(s) = q {
        url.push_str(&format!("&q={}", s));
    }
    if let Some(s) = sort {
        url.push_str(&format!("&sort={}", s));
    }
    if let Some(d) = sort_dir {
        url.push_str(&format!("&sort_dir={}", d));
    }
    append_watch(&mut url, &watch);
    reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<PaginatedResponse<MovieSummary>>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// GET /api/user/providers — the signed-in user's saved provider selections.
pub async fn get_user_providers(token: &str) -> Result<UserProvidersPayload, String> {
    let url = format!("{}/api/user/providers", api_base());
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        resp.json::<UserProvidersPayload>()
            .await
            .map_err(|e| format!("Parse error: {}", e))
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// PUT /api/user/providers — replace the user's provider selections (last-write-wins).
pub async fn put_user_providers(
    payload: UserProvidersPayload,
    token: &str,
) -> Result<(), String> {
    let url = format!("{}/api/user/providers", api_base());
    let resp = reqwest::Client::new()
        .put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// Fetch the streaming providers available in a region (for the picker).
pub async fn fetch_providers(region: &str) -> Result<Vec<ProviderInfo>, String> {
    let url = format!("{}/api/providers?region={}", api_base(), region);
    reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<Vec<ProviderInfo>>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// Fetch a single movie by its mv+base36 encoded ID.
pub async fn fetch_movie(encoded_id: &str) -> Result<Movie, String> {
    let url = format!("{}/api/movies/{}", api_base(), encoded_id);
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    match resp.status().as_u16() {
        404 => Err("Movie not found".to_string()),
        400 => Err("Invalid movie ID".to_string()),
        _ => resp
            .json::<Movie>()
            .await
            .map_err(|e| format!("Parse error: {}", e)),
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
    admin_post(
        "/api/admin/enrich",
        serde_json::json!({ "limit": limit }),
        token,
    )
    .await
}

/// POST /api/admin/score — batch scoring in background.
pub async fn admin_score(token: &str) -> Result<serde_json::Value, String> {
    admin_post("/api/admin/score", serde_json::json!({}), token).await
}

/// POST /api/admin/providers-sync — streaming provider sync in background.
pub async fn admin_provider_sync(limit: i64, token: &str) -> Result<serde_json::Value, String> {
    admin_post(
        "/api/admin/providers-sync",
        serde_json::json!({ "limit": limit }),
        token,
    )
    .await
}

/// GET /api/admin/logs — recent run log entries.
pub async fn admin_logs(token: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/admin/logs", api_base());
    let mut req = reqwest::Client::new().get(&url);
    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }
    req.send()
        .await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

/// GET /api/admin/status — capability flags (no auth required).
pub async fn fetch_admin_status() -> Result<serde_json::Value, String> {
    let url = format!("{}/api/admin/status", api_base());
    reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

// ── Auth ──────────────────────────────────────────────────────────────────────

/// `POST /api/auth/magic` — request a magic-link email.
pub async fn send_magic_link(email: &str) -> Result<(), String> {
    let url = format!("{}/api/auth/magic", api_base());
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({ "email": email }))
        .send()
        .await
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
    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        resp.json::<AuthResponse>()
            .await
            .map_err(|e| format!("Parse error: {}", e))
    } else {
        Err(resp
            .text()
            .await
            .unwrap_or_else(|_| "Invalid or expired link".into()))
    }
}

// ── Watchlist ─────────────────────────────────────────────────────────────────

/// `GET /api/watchlist` — return the user's full watchlist.
pub async fn get_watchlist(token: &str) -> Result<Vec<WatchlistEntry>, String> {
    let url = format!("{}/api/watchlist", api_base());
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    if resp.status().is_success() {
        resp.json::<Vec<WatchlistEntry>>()
            .await
            .map_err(|e| format!("Parse error: {}", e))
    } else {
        Err(format!("Server error: {}", resp.status()))
    }
}

/// `GET /api/watchlist/movie/:movie_id` — check if a specific movie is on the watchlist.
/// Returns `None` if not on the list (404 from server).
pub async fn get_watchlist_entry(
    movie_id: i64,
    token: &str,
) -> Result<Option<WatchlistEntry>, String> {
    let url = format!("{}/api/watchlist/movie/{}", api_base(), movie_id);
    let resp = reqwest::Client::new()
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
    let url = format!("{}/api/watchlist", api_base());
    let resp = reqwest::Client::new()
        .post(&url)
        .header("Authorization", format!("Bearer {}", token))
        .json(&WatchlistUpsert {
            movie_id,
            state,
            user_rating,
        })
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
    let url = format!("{}/api/watchlist/movie/{}", api_base(), movie_id);
    let resp = reqwest::Client::new()
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

// ── Analytics ─────────────────────────────────────────────────────────────────

/// Payload for POST /api/event — all fields except event_type are optional.
#[derive(Serialize)]
pub struct TrackEventPayload {
    pub event_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movie_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub era: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_num: Option<i32>,
}

/// Fire an anonymous engagement event to POST /api/event.
/// Fire-and-forget — errors are silently swallowed; analytics must never affect the UI.
pub async fn track_event(payload: TrackEventPayload) {
    let url = format!("{}/api/event", api_base());
    let _ = reqwest::Client::new()
        .post(&url)
        .json(&payload)
        .send()
        .await;
}

/// Call `window.umami.track(event_name, props_json)` from WASM via js_sys::eval.
/// The `if (window.umami)` guard handles the race where the deferred script hasn't loaded yet.
/// `props_json` must be a valid JSON object string, e.g. `r#"{"section":"gems"}"#`.
pub fn track_umami(event_name: &str, props_json: &str) {
    let script = format!(
        "if(window.umami){{window.umami.track('{}',{});}}",
        event_name, props_json
    );
    let _ = js_sys::eval(&script);
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn admin_post(
    path: &str,
    body: serde_json::Value,
    token: &str,
) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", api_base(), path);
    let mut req = reqwest::Client::new().post(&url).json(&body);
    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }
    req.send()
        .await
        .map_err(|e| format!("Network error: {}", e))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}
