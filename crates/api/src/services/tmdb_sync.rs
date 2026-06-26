use anyhow::Result;
use gem_finder_db::models;
use gem_finder_shared::{
    constants::{tmdb_rate_limit, SEEDED_GEMS},
    types::{
        Movie, TmdbConfig, TmdbCredits, TmdbDiscoverResponse, TmdbFindResponse, TmdbKeywordsResponse, TmdbMovieDetail,
    },
};
use reqwest::Client;
use turso::Connection;

pub struct TmdbSyncService {
    client: Client,
    api_key: String,
    base_url: String,
    image_base_url: Option<String>,
}

impl TmdbSyncService {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.themoviedb.org/3".to_string(),
            image_base_url: None,
        }
    }

    pub async fn init_config(&mut self) -> Result<()> {
        let url = format!("{}/configuration?api_key={}", self.base_url, self.api_key);
        let config: TmdbConfig = self.client.get(&url).send().await?.json().await?;
        self.image_base_url = Some(format!("{}w500", config.images.secure_base_url));
        Ok(())
    }

    /// Sync hidden-gem candidates: movies rated 6.0–8.0 with at least 500 votes.
    ///
    /// `end_year`: upper date bound (inclusive). Defaults to `current_year - MIN_GEM_AGE_YEARS`
    /// when `None`. Pass an explicit value to sync a specific era window (e.g. 1985 for classics).
    ///
    /// Sort is `vote_count.desc` — most-voted films in the window appear first, so each page
    /// covers notable films distributed across ALL years in the era rather than the newest N.
    ///
    /// Exhausts all available TMDB pages using wave-based rate limiting:
    /// - `PAGE_DELAY_MS` between every discover page request
    /// - `WAVE_DELAY_MS` after every `WAVE_SIZE` pages (lets the sliding-window bucket recover)
    ///
    /// The natural ceiling is `response.total_pages` (up to TMDB's 500-page limit).
    /// On re-runs, cached movies are skipped via `get_movie_by_tmdb_id` so only the
    /// discover call is made — the delay still prevents 429s when many pages fire quickly.
    pub async fn sync_movies(
        &self,
        conn: &Connection,
        start_year: i32,
        end_year: Option<i32>,
    ) -> Result<()> {
        use chrono::Datelike;
        use gem_finder_shared::constants::MIN_GEM_AGE_YEARS;

        let mut page = 1;

        let default_cutoff = chrono::Utc::now().year() - MIN_GEM_AGE_YEARS;
        let cutoff_year = end_year.unwrap_or(default_cutoff).min(default_cutoff);

        tracing::info!(
            "Syncing gem candidates: {}-01-01 to {}-12-31, vote_avg 6.0-8.0, vote_count ≥ 500 (wave pagination)",
            start_year, cutoff_year
        );

        let mut synced = 0usize;
        let mut skipped = 0usize;

        loop {
            // Wave boundary: longer pause every WAVE_SIZE pages so the rate-limit bucket recovers.
            if page > 1 && (page - 1) % tmdb_rate_limit::WAVE_SIZE == 0 {
                tracing::info!(
                    "Wave boundary at page {} (window {}-{}) — pausing {}ms",
                    page,
                    start_year,
                    cutoff_year,
                    tmdb_rate_limit::WAVE_DELAY_MS
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    tmdb_rate_limit::WAVE_DELAY_MS,
                ))
                .await;
            } else if page > 1 {
                // Standard inter-page delay.
                tokio::time::sleep(std::time::Duration::from_millis(
                    tmdb_rate_limit::PAGE_DELAY_MS,
                ))
                .await;
            }

            let discover_url = format!(
                "{}/discover/movie?api_key={}&primary_release_date.gte={}-01-01\
                 &primary_release_date.lte={}-12-31\
                 &vote_average.gte=6.0&vote_average.lte=8.0&vote_count.gte=500\
                 &sort_by=vote_count.desc&page={}",
                self.base_url, self.api_key, start_year, cutoff_year, page
            );

            let http_resp = self.client.get(&discover_url).send().await?;
            let status = http_resp.status();
            if !status.is_success() {
                let body = http_resp.text().await.unwrap_or_default();
                tracing::error!(
                    "TMDB discover page {} HTTP {}: {}",
                    page,
                    status,
                    &body[..body.len().min(200)]
                );
                break;
            }

            let response: TmdbDiscoverResponse = http_resp.json().await?;

            let page_count = response.results.len();
            tracing::info!(
                "Discover page {}/{}: {} movies (window {}-{})",
                page,
                response.total_pages,
                page_count,
                start_year,
                cutoff_year
            );

            for tmdb_movie in response.results {
                // Skip if already in DB — avoids redundant API calls on re-runs.
                if let Ok(Some(_)) = models::get_movie_by_tmdb_id(conn, tmdb_movie.id).await {
                    skipped += 1;
                    continue;
                }
                match self.sync_single_movie(conn, tmdb_movie.id).await {
                    Ok(_) => synced += 1,
                    Err(e) => tracing::error!("Failed to sync movie {}: {}", tmdb_movie.id, e),
                }
            }

            if page >= response.total_pages || page >= tmdb_rate_limit::MAX_PAGES {
                break;
            }
            page += 1;
        }

        tracing::info!(
            "Gem candidate sync complete (window {}-{}): {} new, {} already in DB ({} pages fetched)",
            start_year, cutoff_year, synced, skipped, page
        );
        Ok(())
    }

    /// Sync blockbuster movies and record them in the big_hits table.
    ///
    /// These are NOT filtered by rating — we want blockbusters regardless of IMDb score.
    /// Sorted by vote_count descending: the most-voted movies on TMDB are inherently
    /// blockbusters (Avengers, Inception, The Dark Knight, etc.) without needing an
    /// explicit vote_count.gte filter, which was returning 0 results due to a TMDB
    /// API quirk (HTTP 200 with empty results array).
    ///
    /// The batch scoring pipeline reads from big_hits to derive the "obscured by big hit"
    /// signal; without this pass, that 0.20-weighted signal is always zero.
    pub async fn sync_blockbusters(&self, conn: &Connection) -> Result<usize> {
        let mut page = 1;
        // 5 pages × 20 results = up to 100 blockbusters — enough to cover major releases.
        const MAX_PAGES: i32 = 5;
        let mut inserted = 0usize;

        loop {
            // Sort by vote_count.desc — highest-voted movies are blockbusters by definition.
            // Previously used vote_count.gte=100000 which returned 0 results (TMDB quirk).
            let url = format!(
                "{}/discover/movie?api_key={}&sort_by=vote_count.desc&page={}",
                self.base_url, self.api_key, page
            );

            let http_resp = self.client.get(&url).send().await?;
            let status = http_resp.status();
            if !status.is_success() {
                let body = http_resp.text().await.unwrap_or_default();
                tracing::error!(
                    "TMDB blockbuster discover page {} HTTP {}: {}",
                    page,
                    status,
                    &body[..body.len().min(200)]
                );
                break;
            }

            let response: TmdbDiscoverResponse = http_resp.json().await?;

            tracing::info!(
                "TMDB blockbuster page {}/{}: {} movies",
                page,
                MAX_PAGES,
                response.results.len()
            );

            for tmdb_movie in &response.results {
                let year = tmdb_movie
                    .release_date
                    .as_deref()
                    .and_then(|d| d.split('-').next())
                    .and_then(|y| y.parse::<i32>().ok())
                    .unwrap_or(0);

                // For the obscured-by-big-hit signal we only need the release date.
                // Avoid 2 extra API calls (detail + credits) per blockbuster by stub-upserting
                // directly from the discover response. COALESCE in upsert_movie preserves any
                // richer data already stored for this film.
                let db_id = match models::get_movie_by_tmdb_id(conn, tmdb_movie.id).await {
                    Ok(Some(existing_id)) => existing_id,
                    _ => {
                        let stub = Movie {
                            id: None,
                            tmdb_id: tmdb_movie.id,
                            imdb_id: None,
                            title: tmdb_movie.title.clone(),
                            year: if year > 0 { Some(year) } else { None },
                            genre: None,
                            director: None,
                            overview: tmdb_movie.overview.clone(),
                            poster_url: None,
                            tmdb_rating: tmdb_movie.vote_average,
                            tmdb_vote_count: tmdb_movie.vote_count,
                            imdb_rating: None,
                            imdb_vote_count: None,
                            rt_critic_score: None,
                            rt_audience_score: None,
                            gem_score: None,
                            gem_rank: None,
                            release_date: tmdb_movie.release_date.clone(),
                            revenue: None,
                            collection_id: None,
                            keywords: None,
                            created_at: None,
                            updated_at: None,
                        };
                        match models::upsert_movie(conn, &stub).await {
                            Ok(id) => id,
                            Err(e) => {
                                tracing::warn!(
                                    "Could not upsert blockbuster stub {}: {}",
                                    tmdb_movie.id,
                                    e
                                );
                                continue;
                            }
                        }
                    }
                };

                let popularity = tmdb_movie.popularity.unwrap_or(0.0);
                match models::insert_big_hit(conn, db_id, year, popularity).await {
                    Ok(()) => inserted += 1,
                    Err(e) => tracing::warn!(
                        "Could not insert big_hit for tmdb_id {}: {}",
                        tmdb_movie.id,
                        e
                    ),
                }
            }

            if page >= response.total_pages || page >= MAX_PAGES {
                break;
            }
            page += 1;
        }

        tracing::info!(
            "Blockbuster sync complete: {} records written to big_hits",
            inserted
        );
        Ok(inserted)
    }

    /// Ensure all entries in `SEEDED_GEMS` are present in the database.
    ///
    /// Uses TMDB's `/find/{imdb_id}` endpoint to resolve each seeded gem's IMDb ID to a
    /// TMDB id, then upserts it like any other movie. These are the algorithm's ground-truth
    /// validation set — if they're not in the DB, acceptance-criteria checks are meaningless.
    ///
    /// Returns (seeded_count, total, Vec<(title, result)>), where result is one of
    /// "seeded", "not_found" (no TMDB match), "sync_failed", or "api_error".
    pub async fn seed_known_gems(
        &self,
        conn: &Connection,
    ) -> Result<(usize, usize, Vec<(String, String)>)> {
        let total = SEEDED_GEMS.len();
        let mut seeded = 0usize;
        let mut results: Vec<(String, String)> = Vec::new();

        for (title, year, imdb_id) in SEEDED_GEMS {
            let result = self.seed_single_gem(conn, title, *year, imdb_id).await;
            if result.as_str() == "seeded" {
                seeded += 1
            }
            results.push((title.to_string(), result));
        }

        tracing::info!("Seeded {}/{} known gems", seeded, total);
        Ok((seeded, total, results))
    }

    /// Attempt to seed a single gem.
    ///
    /// Strategy:
    /// 1. Primary: TMDB `/find/{imdb_id}?external_source=imdb_id` (exact IMDb ID lookup).
    ///    Retried up to MAX_RETRIES times on network/parse errors.
    /// 2. Fallback: TMDB `/search/movie?query={title}&year={year}` if the `/find` endpoint
    ///    returns an empty `movie_results` list. This catches cases where TMDB hasn't linked
    ///    the IMDb ID yet (common for older or international films).
    ///
    /// Returns a result string: "seeded", "not_found", "sync_failed", or "api_error".
    async fn seed_single_gem(
        &self,
        conn: &Connection,
        title: &str,
        year: i32,
        imdb_id: &str,
    ) -> String {
        const MAX_RETRIES: u32 = 3;

        // ── Phase 1: find by IMDb ID ────────────────────────────────────────
        // Returns true if we should proceed to the title-search fallback (empty results),
        // or false if we either succeeded or hit a hard error.
        let mut try_fallback = false;

        'find: for attempt in 1..=MAX_RETRIES {
            let url = format!(
                "{}/find/{}?api_key={}&external_source=imdb_id",
                self.base_url, imdb_id, self.api_key
            );

            let find_resp: TmdbFindResponse = match self.client.get(&url).send().await {
                Ok(r) => match r.json().await {
                    Ok(j) => j,
                    Err(e) => {
                        tracing::warn!(
                            "'{}' (imdb:{}) /find attempt {}/{}: parse error: {}",
                            title,
                            imdb_id,
                            attempt,
                            MAX_RETRIES,
                            e
                        );
                        if attempt < MAX_RETRIES {
                            tokio::time::sleep(std::time::Duration::from_millis(
                                500 * attempt as u64,
                            ))
                            .await;
                        } else {
                            tracing::error!(
                                "'{}' (imdb:{}): /find parse errors exhausted retries",
                                title,
                                imdb_id
                            );
                            return "api_error".to_string();
                        }
                        continue;
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "'{}' (imdb:{}) /find attempt {}/{}: request error: {}",
                        title,
                        imdb_id,
                        attempt,
                        MAX_RETRIES,
                        e
                    );
                    if attempt < MAX_RETRIES {
                        tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64))
                            .await;
                    } else {
                        tracing::error!(
                            "'{}' (imdb:{}): /find network errors exhausted retries",
                            title,
                            imdb_id
                        );
                        return "api_error".to_string();
                    }
                    continue;
                }
            };

            match find_resp.movie_results.first() {
                Some(tmdb_movie) => {
                    let found_tmdb_id = tmdb_movie.id;

                    // Skip the 2-call sync if the movie is already in the DB.
                    if let Ok(Some(_)) = models::get_movie_by_tmdb_id(conn, found_tmdb_id).await {
                        tracing::info!(
                            "'{}' (tmdb={}) already in DB — skipping API sync",
                            title,
                            found_tmdb_id
                        );
                        return "seeded".to_string();
                    }

                    match self.sync_single_movie(conn, found_tmdb_id).await {
                        Ok(_) => {
                            tracing::info!(
                                "Seeded '{}' via /find (imdb={}, tmdb={}) attempt {}",
                                title,
                                imdb_id,
                                found_tmdb_id,
                                attempt
                            );
                            return "seeded".to_string();
                        }
                        Err(e) => {
                            tracing::warn!(
                                "'{}' /find attempt {}/{}: sync failed (tmdb={}): {}",
                                title,
                                attempt,
                                MAX_RETRIES,
                                found_tmdb_id,
                                e
                            );
                            if attempt < MAX_RETRIES {
                                tokio::time::sleep(std::time::Duration::from_millis(
                                    500 * attempt as u64,
                                ))
                                .await;
                            } else {
                                tracing::error!("'{}' (imdb:{}): /find found movie but all sync attempts failed", title, imdb_id);
                                return "sync_failed".to_string();
                            }
                            continue;
                        }
                    }
                }
                None => {
                    // Empty movie_results is a definitive answer, not a transient error.
                    // Break and try the title-search fallback.
                    tracing::warn!(
                        "'{}' (imdb:{}): /find returned 0 movie_results. Trying title/year search fallback.",
                        title, imdb_id
                    );
                    try_fallback = true;
                    break 'find;
                }
            }
        }

        if !try_fallback {
            // Loop exited normally (all retries consumed) without setting try_fallback.
            // This means all attempts failed for a reason other than empty results.
            return "api_error".to_string();
        }

        // ── Phase 2: fallback to title+year search ──────────────────────────
        tracing::info!("'{}' ({}): trying /search/movie fallback", title, year);
        let encoded_title = title.replace(' ', "+");
        let search_url = format!(
            "{}/search/movie?api_key={}&query={}&year={}&include_adult=false",
            self.base_url, self.api_key, encoded_title, year
        );

        let search_resp: TmdbDiscoverResponse = match self.client.get(&search_url).send().await {
            Ok(r) => match r.json().await {
                Ok(j) => j,
                Err(e) => {
                    tracing::error!("'{}': /search/movie parse error: {}", title, e);
                    return "api_error".to_string();
                }
            },
            Err(e) => {
                tracing::error!("'{}': /search/movie request error: {}", title, e);
                return "api_error".to_string();
            }
        };

        tracing::info!(
            "'{}' ({}) search fallback returned {} result(s)",
            title,
            year,
            search_resp.results.len()
        );

        match search_resp.results.first() {
            Some(tmdb_movie) => {
                let tmdb_id = tmdb_movie.id;
                let found_title = &tmdb_movie.title;

                // Skip sync if already in DB.
                if let Ok(Some(_)) = models::get_movie_by_tmdb_id(conn, tmdb_id).await {
                    tracing::info!(
                        "'{}' (tmdb={}) already in DB — skipping search fallback sync",
                        title,
                        tmdb_id
                    );
                    return "seeded".to_string();
                }

                match self.sync_single_movie(conn, tmdb_id).await {
                    Ok(_) => {
                        tracing::info!(
                            "Seeded '{}' via /search fallback (matched '{}', tmdb={})",
                            title,
                            found_title,
                            tmdb_id
                        );
                        "seeded".to_string()
                    }
                    Err(e) => {
                        tracing::error!(
                            "'{}': /search fallback sync failed for tmdb_id {}: {}",
                            title,
                            tmdb_id,
                            e
                        );
                        "sync_failed".to_string()
                    }
                }
            }
            None => {
                tracing::error!(
                    "TMDB has no match for '{}' ({}, imdb: {}) via /find or /search. Check IMDb ID.",
                    title, year, imdb_id
                );
                "not_found".to_string()
            }
        }
    }

    /// Sync acclaimed candidates: highly-rated movies that may qualify for the acclaimed table.
    ///
    /// Targets films with TMDB vote_average ≥ 7.5 and vote_count ≥ 10,000.
    /// After this sync, `classify_acclaimed_films` applies the IMDb ≥ 8.0 / RT ≥ 80
    /// threshold to populate the acclaimed table itself.
    ///
    /// Exhausts all available pages using wave-based rate limiting (same as `sync_movies`).
    pub async fn sync_acclaimed_candidates(&self, conn: &Connection) -> Result<usize> {
        let mut page = 1;
        let mut synced = 0usize;
        let mut skipped = 0usize;

        tracing::info!(
            "Syncing acclaimed candidates: vote_avg ≥ 7.5, vote_count ≥ 10000 (wave pagination)"
        );

        loop {
            // Wave / page delays (same pattern as sync_movies).
            if page > 1 && (page - 1) % tmdb_rate_limit::WAVE_SIZE == 0 {
                tracing::info!(
                    "Wave boundary at acclaimed page {} — pausing {}ms",
                    page,
                    tmdb_rate_limit::WAVE_DELAY_MS
                );
                tokio::time::sleep(std::time::Duration::from_millis(
                    tmdb_rate_limit::WAVE_DELAY_MS,
                ))
                .await;
            } else if page > 1 {
                tokio::time::sleep(std::time::Duration::from_millis(
                    tmdb_rate_limit::PAGE_DELAY_MS,
                ))
                .await;
            }

            let url = format!(
                "{}/discover/movie?api_key={}&vote_average.gte=7.5&vote_count.gte=10000\
                 &sort_by=vote_average.desc&page={}",
                self.base_url, self.api_key, page
            );

            let http_resp = self.client.get(&url).send().await?;
            let status = http_resp.status();
            if !status.is_success() {
                let body = http_resp.text().await.unwrap_or_default();
                tracing::error!(
                    "TMDB acclaimed discover page {} HTTP {}: {}",
                    page,
                    status,
                    &body[..body.len().min(200)]
                );
                break;
            }

            let response: TmdbDiscoverResponse = http_resp.json().await?;
            tracing::info!(
                "Acclaimed candidates page {}/{}: {} movies",
                page,
                response.total_pages,
                response.results.len()
            );

            for tmdb_movie in response.results {
                if let Ok(Some(_)) = models::get_movie_by_tmdb_id(conn, tmdb_movie.id).await {
                    skipped += 1;
                    continue;
                }
                match self.sync_single_movie(conn, tmdb_movie.id).await {
                    Ok(_) => synced += 1,
                    Err(e) => tracing::error!(
                        "Failed to sync acclaimed candidate {}: {}",
                        tmdb_movie.id,
                        e
                    ),
                }
            }

            if page >= response.total_pages || page >= tmdb_rate_limit::MAX_PAGES {
                break;
            }
            page += 1;
        }

        tracing::info!(
            "Acclaimed candidate sync complete: {} new, {} already in DB ({} pages fetched)",
            synced,
            skipped,
            page
        );
        Ok(synced)
    }

    /// Fetch and upsert a single movie. Returns the database row id.
    async fn sync_single_movie(&self, conn: &Connection, tmdb_id: i64) -> Result<i64> {
        // Fetch movie detail (no append_to_response — TmdbMovieDetail has no credits field,
        // so appending credits would silently waste the payload and the quota).
        let detail_url = format!(
            "{}/movie/{}?api_key={}",
            self.base_url, tmdb_id, self.api_key
        );

        let detail: TmdbMovieDetail = self.client.get(&detail_url).send().await?.json().await?;

        // Fetch credits separately (single explicit call, no duplication).
        let credits_url = format!(
            "{}/movie/{}/credits?api_key={}",
            self.base_url, tmdb_id, self.api_key
        );
        let credits: TmdbCredits = self.client.get(&credits_url).send().await?.json().await?;
        // Fetch keywords for Musical and other tag-based filtering.
        let keywords_url = format!("{}/movie/{}/keywords?api_key={}", self.base_url, tmdb_id, self.api_key);
        let keywords_resp: TmdbKeywordsResponse = self.client.get(&keywords_url).send().await?.json().await?;
        let keywords_str = keywords_resp.keywords.iter().map(|k| k.name.clone()).collect::<Vec<_>>().join(", ");

        let director = credits
            .crew
            .iter()
            .find(|c| c.job == "Director")
            .map(|c| c.name.clone());

        let genres = detail
            .genres
            .iter()
            .map(|g| g.name.clone())
            .collect::<Vec<_>>()
            .join(", ");

        let poster_url = detail.poster_path.as_ref().and_then(|path| {
            self.image_base_url
                .as_ref()
                .map(|base| format!("{}{}", base, path))
        });

        let movie = Movie {
            id: None,
            tmdb_id: detail.id,
            imdb_id: detail.imdb_id,
            title: detail.title,
            year: detail
                .release_date
                .as_ref()
                .and_then(|d| d.split('-').next())
                .and_then(|y| y.parse().ok()),
            genre: Some(genres),
            director,
            overview: detail.overview,
            poster_url,
            tmdb_rating: detail.vote_average,
            tmdb_vote_count: detail.vote_count,
            // imdb_rating is intentionally None — TMDB does not provide IMDb ratings.
            // A separate enrichment step (e.g. OMDb API) is required to populate this.
            // The scoring algorithm falls back to tmdb_rating when imdb_rating is None.
            imdb_rating: None,
            imdb_vote_count: None,
            rt_critic_score: None,
            rt_audience_score: None,
            gem_score: None,
            gem_rank: None,
            release_date: detail.release_date,
            revenue: detail.revenue,
            collection_id: detail.belongs_to_collection.map(|c| c.id),
 keywords: if keywords_str.is_empty() { None } else { Some(keywords_str) },
            created_at: None,
            updated_at: None,
        };

        let db_id = models::upsert_movie(conn, &movie).await?;
        Ok(db_id)
    }
}
