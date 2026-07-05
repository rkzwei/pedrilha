use anyhow::{Context, Result};
use chrono::Utc;
use gem_finder_db::models;
use gem_finder_shared::{
    constants::tmdb_rate_limit,
    types::{TmdbProvider, TmdbRegionProviders, TmdbWatchProvidersResponse},
};
use reqwest::Client;
use turso::Connection;

/// Regions we support for the "What can I watch?" filter.
const REGIONS: [&str; 2] = ["US", "BR"];

/// Service for syncing streaming availability via the TMDB watch-providers API
/// (JustWatch data). Free with the existing `TMDB_API_KEY`.
///
/// Mirrors `OmdbEnrichmentService`: batch over candidate movies, per-item fetch,
/// chunked progress logging, wave-based rate limiting.
pub struct ProviderSyncService {
    client: Client,
    api_key: String,
    base_url: String,
}

impl ProviderSyncService {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.themoviedb.org/3".to_string(),
        }
    }

    /// Sync providers for scored movies whose data is missing or > 7 days stale.
    ///
    /// Returns `(synced_count, total_candidates, errors)`.
    pub async fn sync_providers(
        &self,
        conn: &Connection,
        limit: i64,
    ) -> Result<(usize, usize, Vec<String>)> {
        let candidates = models::get_movies_needing_provider_sync(conn, limit).await?;
        let total = candidates.len();
        let mut synced = 0usize;
        let mut errors: Vec<String> = Vec::new();

        tracing::info!("Provider sync: {} movies to process", total);

        const CHUNK: usize = 50;

        for (i, (movie_id, tmdb_id)) in candidates.iter().enumerate() {
            // Wave-based rate limiting: longer pause every WAVE_SIZE requests.
            if i > 0 && (i as i32) % tmdb_rate_limit::WAVE_SIZE == 0 {
                tokio::time::sleep(std::time::Duration::from_millis(
                    tmdb_rate_limit::WAVE_DELAY_MS,
                ))
                .await;
            } else if i > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(
                    tmdb_rate_limit::PAGE_DELAY_MS,
                ))
                .await;
            }

            match self.sync_single_movie(conn, *movie_id, *tmdb_id).await {
                Ok(()) => synced += 1,
                Err(e) => {
                    let msg = format!(
                        "Failed providers for movie {} (tmdb {}): {}",
                        movie_id, tmdb_id, e
                    );
                    tracing::warn!("{}", msg);
                    errors.push(msg);
                }
            }

            let processed = i + 1;
            if processed % CHUNK == 0 || processed == total {
                tracing::info!(
                    "Provider sync: {}/{} processed, {} synced, {} errors so far",
                    processed,
                    total,
                    synced,
                    errors.len()
                );
                let _ = models::insert_run_log(
                    conn,
                    "info",
                    "provider_sync_progress",
                    &format!(
                        "{}/{} processed, {} synced, {} errors",
                        processed,
                        total,
                        synced,
                        errors.len()
                    ),
                )
                .await;
            }
        }

        tracing::info!(
            "Provider sync complete: {}/{} synced, {} errors",
            synced,
            total,
            errors.len()
        );

        Ok((synced, total, errors))
    }

    /// Fetch and store providers for one movie across all supported regions.
    /// Always replaces each region's rows (empty set clears stale availability).
    async fn sync_single_movie(
        &self,
        conn: &Connection,
        movie_id: i64,
        tmdb_id: i64,
    ) -> Result<()> {
        let url = format!(
            "{}/movie/{}/watch/providers?api_key={}",
            self.base_url, tmdb_id, self.api_key
        );

        let http_resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("TMDB watch/providers request failed")?;
        let status = http_resp.status();
        if !status.is_success() {
            let body = http_resp.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "TMDB HTTP {}: {}",
                status,
                &body[..body.len().min(200)]
            ));
        }

        let response: TmdbWatchProvidersResponse = http_resp
            .json()
            .await
            .context("TMDB watch/providers parse failed")?;

        // Keep the first available region link for bookkeeping (detail links are
        // reconstructed per-region from tmdb_id when read).
        let mut stored_link: Option<String> = None;

        for region in REGIONS {
            let region_data = response.results.get(region);
            let rows = region_data.map(build_rows).unwrap_or_default();
            if stored_link.is_none() {
                if let Some(link) = region_data.and_then(|r| r.link.clone()) {
                    stored_link = Some(link);
                }
            }
            models::replace_movie_providers(conn, movie_id, region, &rows).await?;
        }

        let fetched_at = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        models::upsert_provider_sync(conn, movie_id, &fetched_at, stored_link.as_deref()).await?;

        Ok(())
    }
}

/// Flatten one region's tiered provider arrays into `(id, name, logo, access)` rows.
fn build_rows(region: &TmdbRegionProviders) -> Vec<(i32, String, Option<String>, String)> {
    let mut rows: Vec<(i32, String, Option<String>, String)> = Vec::new();
    let tiers: [(&str, &Vec<TmdbProvider>); 5] = [
        ("flatrate", &region.flatrate),
        ("free", &region.free),
        ("ads", &region.ads),
        ("rent", &region.rent),
        ("buy", &region.buy),
    ];
    for (access, providers) in tiers {
        for p in providers {
            rows.push((
                p.provider_id,
                p.provider_name.clone(),
                p.logo_path.clone(),
                access.to_string(),
            ));
        }
    }
    rows
}
