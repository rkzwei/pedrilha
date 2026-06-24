use anyhow::{Context, Result};
use gem_finder_db::models;
use gem_finder_shared::types::OmdbResponse;
use reqwest::Client;
use turso::Connection;

/// Service for enriching movie data via the OMDb API.
///
/// OMDb provides both IMDb ratings/votes and Rotten Tomatoes scores in a single
/// request by IMDb ID — this is the only reliable way to get RT data since TMDB
/// does not provide it.
pub struct OmdbEnrichmentService {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OmdbEnrichmentService {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://www.omdbapi.com".to_string(),
        }
    }

    /// Enrich all movies that are missing IMDb ratings or RT scores.
    ///
    /// Iterates over movies with a non-empty `imdb_id` but null `imdb_rating`
    /// or null `rt_critic_score`. Calls OMDb for each and updates the DB.
    ///
    /// Returns (enriched_count, total_candidates, errors).
    pub async fn enrich_movies(
        &self,
        conn: &Connection,
        limit: i64,
    ) -> Result<(usize, usize, Vec<String>)> {
        let candidates = models::get_movies_needing_enrichment(conn, limit).await?;
        let total = candidates.len();
        let mut enriched = 0usize;
        let mut errors: Vec<String> = Vec::new();

        tracing::info!("OMDb enrichment: {} movies to process", total);

        const CHUNK: usize = 50;

        for (i, (movie_id, title, imdb_id)) in candidates.iter().enumerate() {
            match self
                .enrich_single_movie(conn, *movie_id, title, imdb_id)
                .await
            {
                Ok(true) => enriched += 1,
                Ok(false) => {} // no new data (OMDb had no RT scores)
                Err(e) => {
                    let msg = format!("Failed to enrich '{}' (imdb: {}): {}", title, imdb_id, e);
                    tracing::warn!("{}", msg);
                    errors.push(msg);
                }
            }

            // Progress log every CHUNK movies
            let processed = i + 1;
            if processed % CHUNK == 0 || processed == total {
                tracing::info!(
                    "OMDb enrichment: {}/{} processed, {} enriched, {} errors so far",
                    processed, total, enriched, errors.len()
                );
                // Also write to run_logs so the admin panel shows live progress
                let _ = models::insert_run_log(
                    conn,
                    "info",
                    "enrich_progress",
                    &format!("{}/{} processed, {} enriched, {} errors",
                        processed, total, enriched, errors.len()),
                ).await;
            }
        }

        tracing::info!(
            "OMDb enrichment complete: {}/{} enriched, {} errors",
            enriched,
            total,
            errors.len()
        );

        Ok((enriched, total, errors))
    }

    /// Enrich a single movie by calling OMDb with its IMDb ID.
    /// Returns `Ok(true)` if new data was written, `Ok(false)` if OMDb responded
    /// successfully but all fields were already present (genuine no-op).
    /// Returns `Err` for both network/parse failures AND OMDb `Response: False`
    /// (OMDb has no record for this IMDb ID — the movie will not be enriched).
    async fn enrich_single_movie(
        &self,
        conn: &Connection,
        movie_id: i64,
        title: &str,
        imdb_id: &str,
    ) -> Result<bool> {
        let url = format!("{}/?apikey={}&i={}", self.base_url, self.api_key, imdb_id);

        let response: OmdbResponse = self
            .client
            .get(&url)
            .send()
            .await
            .context("OMDb request failed")?
            .json()
            .await
            .context("OMDb response parse failed")?;

        // Check for OMDb error (e.g. "Movie not found!", "Error getting data.", "Invalid API key!").
        // OMDb returns {"Response": "False", "Error": "reason"} on failure.
        let is_ok = response.response.as_deref() == Some("True");
        if !is_ok {
            let err_msg = response.error.as_deref().unwrap_or("unknown error");
            // Return Err so the caller counts this in the error tally.
            // The movie stays in the unenriched queue; the error count reflects reality.
            return Err(anyhow::anyhow!("OMDb no data: {}", err_msg));
        }

        // Extract IMDb rating
        let imdb_rating: Option<f64> = response
            .imdb_rating
            .as_deref()
            .and_then(|r| r.parse::<f64>().ok())
            .filter(|r| *r > 0.0 && *r <= 10.0);

        // Extract IMDb vote count (OMDb returns it as a string like "123,456")
        let imdb_vote_count: Option<i64> = response
            .imdb_votes
            .as_deref()
            .map(|v| v.replace(',', ""))
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|c| *c > 0);

        // Extract RT critic and audience scores from the ratings array
        let (rt_critic, rt_audience) = self.extract_rt_scores(&response);

        // Only update if we found at least one new piece of data
        if imdb_rating.is_none()
            && imdb_vote_count.is_none()
            && rt_critic.is_none()
            && rt_audience.is_none()
        {
            return Ok(false);
        }

        models::update_movie_enrichment(
            conn,
            movie_id,
            imdb_rating,
            imdb_vote_count,
            rt_critic,
            rt_audience,
        )
        .await?;

        tracing::debug!(
            "Enriched '{}' (id={}): imdb_rating={:?}, imdb_votes={:?}, rt_critic={:?}, rt_audience={:?}",
            title,
            movie_id,
            imdb_rating,
            imdb_vote_count,
            rt_critic,
            rt_audience,
        );

        Ok(true)
    }

    /// Extract RT critic and audience scores from OMDb's ratings array.
    ///
    /// OMDb returns ratings in this format:
    /// ```
    /// "Ratings": [
    ///   {"Source": "Internet Movie Database", "Value": "7.5/10"},
    ///   {"Source": "Rotten Tomatoes", "Value": "85%"},
    ///   {"Source": "Metacritic", "Value": "76/100"}
    /// ]
    /// ```
    /// We only extract the Rotten Tomatoes entry, and it's a single score
    /// (OMDb doesn't distinguish critic vs audience). For now we store the
    /// single RT score as `rt_critic_score` and leave `rt_audience_score` None.
    fn extract_rt_scores(&self, response: &OmdbResponse) -> (Option<i32>, Option<i32>) {
        let ratings = match response.ratings.as_ref() {
            Some(r) => r,
            None => return (None, None),
        };

        let mut rt_critic: Option<i32> = None;
        let rt_audience: Option<i32> = None;

        for rating in ratings {
            let source = rating.source.as_deref().unwrap_or("");
            let value = rating.value.as_deref().unwrap_or("");

            if source == "Rotten Tomatoes" {
                // Value is like "85%" — strip the % and parse
                if let Some(pct_str) = value.strip_suffix('%') {
                    if let Ok(pct) = pct_str.parse::<i32>() {
                        rt_critic = Some(pct);
                        // OMDb only provides a single RT score. We store it as critic
                        // score and leave audience as None (it's better than nothing).
                        // If we wanted both we'd need a separate RT API.
                    }
                }
            }
        }

        (rt_critic, rt_audience)
    }
}
