use anyhow::Result;
use chrono::Datelike;
use gem_finder_db::models;
use gem_finder_shared::{
    constants::{
        self, weights, IMDB_GEM_MAX, IMDB_GEM_MIN, IMDB_GEM_MIN_RT_ENDORSED, MIN_GEM_AGE_YEARS,
        MIN_IMDB_VOTES, RT_CREDIBILITY_FLOOR, RT_CREDIBILITY_HIGH, RT_ENDORSEMENT_THRESHOLD,
    },
    types::{GemScore, GemScoreComponents, Movie},
};
use turso::Connection;

/// Calculator for hidden gem scores.
pub struct GemScoreCalculator;

impl Default for GemScoreCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl GemScoreCalculator {
    pub fn new() -> Self {
        Self
    }

    /// Calculate the gem score for a single movie.
    /// Returns `None` if the movie doesn't meet the minimum criteria.
    pub fn calculate(&self, movie: &Movie, big_hit_dates: &[String]) -> Option<GemScore> {
        let movie_id = movie.id?;

        // Use IMDb rating (preferred) or fall back to TMDB rating
        let rating = movie.imdb_rating.or(movie.tmdb_rating)?;
        // Use the larger of IMDb and TMDB vote counts. IMDb has more users overall,
        // but for niche films the TMDB community may have voted more. The larger count
        // is the best proxy for "total audience that has discovered this film."
        let votes = movie
            .imdb_vote_count
            .unwrap_or(0)
            .max(movie.tmdb_vote_count.unwrap_or(0));

        // Minimum votes required to avoid noise
        if votes < MIN_IMDB_VOTES {
            return None;
        }

        // Tiered rating floor: films with strong critic endorsement (RT  RT_ENDORSEMENT_THRESHOLD)
        // can qualify even if their crowd rating is below the standard sweet spot floor.
        // This captures films like The Tunnel (2011): IMDb 5.8, RT 100% - critics endorsed it
        // but audiences missed it due to unconventional distribution. Without critic endorsement
        // a low crowd rating signals genuine poor quality, so the standard floor still applies.
        let effective_min = match movie.rt_critic_score {
            Some(rt) if rt >= RT_ENDORSEMENT_THRESHOLD => IMDB_GEM_MIN_RT_ENDORSED,
            _ => IMDB_GEM_MIN,
        };

        // Rating must be in the sweet spot range
        if !(effective_min..=IMDB_GEM_MAX).contains(&rating) {
            return None;
        }

        // Minimum age filter: exclude films released this calendar year.
        // A film from the current year hasn't had time to be overlooked.
        // Films from last year onwards are eligible; year_decay naturally
        // scores them low relative to older films, so they'll only rank high
        // if other signals (votes, RT) are genuinely strong.
        let current_year = chrono::Utc::now().year();
        if let Some(year) = movie.year {
            if current_year - year < MIN_GEM_AGE_YEARS {
                return None;
            }
        }

        let imdb_rating_score = self.calc_imdb_rating_score(rating, effective_min);
        let vote_ratio_score = self.calc_vote_ratio_score(rating, votes);
        let year_decay_score = self.calc_year_decay_score(movie.year);
        let obscured_by_big_hit_score =
            self.calc_obscured_score(movie.release_date.as_deref(), big_hit_dates);
        let rt_credibility_multiplier = self.calc_rt_credibility_multiplier(movie.rt_critic_score);
        let genre_boost = self.calc_genre_boost(movie.genre.as_deref());

        // RT acts as a credibility multiplier on vote_ratio, not an additive component.
        //
        // True hidden gem = critics endorsed it + audiences missed it (high RT, low votes).
        // Wildcard / divisive = critics panned it + audiences stayed away (low RT, low votes).
        //
        // Without this multiplier, both patterns produce identical vote_ratio scores "
        // the algorithm cannot distinguish genuine undiscovery from informed avoidance.
        // Films with rt < WILDCARD_RT_THRESHOLD are scored (not filtered) but classified
        // as wildcards post-scoring and excluded from the main gems listing.
        let vote_ratio_adjusted = vote_ratio_score * rt_credibility_multiplier;

        let weighted_sum = imdb_rating_score * weights::IMDB_RATING
            + vote_ratio_adjusted * weights::VOTE_RATIO
            + year_decay_score * weights::YEAR_DECAY
            + obscured_by_big_hit_score * weights::OBSCURED;

        let raw_score = weighted_sum * genre_boost;
        let normalized_score = raw_score.clamp(0.0, 1.0);

        Some(GemScore {
            movie_id,
            raw_score,
            normalized_score,
            components: GemScoreComponents {
                imdb_rating_score,
                vote_ratio_score,
                year_decay_score,
                obscured_by_big_hit_score,
                rt_credibility_multiplier,
                genre_boost,
            },
        })
    }

    /// Score the IMDb rating component.
    ///
    /// Standard range [IMDB_GEM_MIN=6.5, IMDB_GEM_MAX=7.9]: quadratic, peaks at 7.2.
    /// At edges (6.5 or 7.9) -> 0.0; at center (7.2) -> 1.0.
    ///
    /// RT-endorsed lower range [IMDB_GEM_MIN_RT_ENDORSED=5.5, IMDB_GEM_MIN=6.5):
    /// Linear from 0.0 (at 5.5) to 0.4 (at 6.5). Capped below the standard floors
    /// maximum to correctly rank RT-endorsed low-rated films below standard sweet-spot
    /// films, while still giving them a meaningful positive signal on this component.
    fn calc_imdb_rating_score(&self, rating: f64, effective_min: f64) -> f64 {
        if !(effective_min..=IMDB_GEM_MAX).contains(&rating) {
            return 0.0;
        }
        if rating < IMDB_GEM_MIN {
            // RT-endorsed lower tier: linear 0.0 -> 0.4 across [effective_min, IMDB_GEM_MIN)
            let range = IMDB_GEM_MIN - effective_min;
            ((rating - effective_min) / range * 0.4).clamp(0.0, 0.4)
        } else {
            // Standard sweet spot: quadratic peaking at center 7.2
            let center = (IMDB_GEM_MIN + IMDB_GEM_MAX) / 2.0; // 7.2
            let half_range = (IMDB_GEM_MAX - IMDB_GEM_MIN) / 2.0; // 0.7
            let deviation = (rating - center).abs() / half_range;
            (1.0 - deviation * deviation).max(0.0)
        }
    }

    /// Score from high rating relative to low vote count (undiscovered factor).
    /// Uses rating / log10(votes): high rating + few votes = hidden gem signal.
    /// Normalization baseline: 7.5 rating at 10,000 votes  ratio  1.875.
    fn calc_vote_ratio_score(&self, rating: f64, votes: i64) -> f64 {
        if votes <= 0 {
            return 0.0;
        }
        let log_votes = (votes as f64).log10().max(1.0);
        let ratio = rating / log_votes;
        // At ~500 votes (log10=2.7) with 7.9 rating  ratio2.93 (high score)
        // At ~1M votes (log10=6) with 6.5 rating  ratio1.08 (low score)
        // Cap at 3.0 to normalize to [0, 1]
        (ratio / 3.0).clamp(0.0, 1.0)
    }

    /// Score from year decay " older movies with sustained ratings are forgotten classics.
    ///
    /// Uses `(age / 30).clamp(0, 1)` instead of `age / (current_year - DATA_START_YEAR)`.
    /// The old formula spread scores across a 66-year range, giving 3-year-old films a
    /// nearly identical score (0.045) to 10-year-old films (0.15) " both effectively zero.
    /// The new formula plateaus at 1.0 for films 30+ years old, and gives meaningful
    /// differentiation across the 3-30 year range where most hidden gems live:
    ///   - 3 years   0.10  (recent release, low "forgotten" signal)
    ///   - 6 years   0.20  (e.g. Dinner in America 2020)
    ///   - 15 years  0.50
    ///   - 18 years  0.60  (e.g. The Hurt Locker 2008)
    ///   - 30+ years  1.0  (e.g. Sorcerer 1977)
    fn calc_year_decay_score(&self, year: Option<i32>) -> f64 {
        let current_year = chrono::Utc::now().year();
        let start_year = constants::DATA_START_YEAR;

        match year {
            Some(y) if y >= start_year && y <= current_year => {
                let age = (current_year - y) as f64;
                (age / 30.0).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    /// Score from being obscured by a nearby blockbuster.
    /// Returns 1.0 if a big hit was released within OBSCURED_WINDOW_WEEKS, else 0.0.
    fn calc_obscured_score(&self, release_date: Option<&str>, big_hit_dates: &[String]) -> f64 {
        let release_date = match release_date {
            Some(d) if !d.is_empty() => d,
            _ => return 0.0,
        };

        let movie_days = match date_to_approx_days(release_date) {
            Some(d) => d,
            None => return 0.0,
        };

        let window_days = constants::OBSCURED_WINDOW_WEEKS * 7;

        for big_hit_date in big_hit_dates {
            if let Some(hit_days) = date_to_approx_days(big_hit_date) {
                if (movie_days - hit_days).abs() <= window_days {
                    return 1.0;
                }
            }
        }
        0.0
    }

    /// RT credibility multiplier for the vote_ratio component.
    ///
    /// A high vote_ratio (high rating + low vote count) is only a reliable hidden-gem
    /// signal when critics also endorsed the film. Without critic endorsement, low vote
    /// counts reflect audiences actively avoiding a panned film ("informed avoidance"),
    /// not genuine undiscovery.
    ///
    /// Mapping:
    ///   - rt >= 70%   1.0  (critics endorsed it " full trust in vote_ratio signal)
    ///   - rt in [40, 70)  linear 0.0  1.0  (partial trust)
    ///   - rt < 40%    0.0  (critics panned it " vote_ratio is misleading)
    ///   - rt = None   0.8  (benefit of doubt; old films often lack RT data)
    ///
    /// Films with rt < WILDCARD_RT_THRESHOLD (50%) are post-scored into the wildcards
    /// table " they still receive a gem_score for ranking purposes but are listed
    /// separately from hidden gems.
    pub(crate) fn calc_rt_credibility_multiplier(&self, rt_critic: Option<i32>) -> f64 {
        match rt_critic {
            None => 0.8, // Benefit of doubt " old/obscure films often lack RT data
            Some(rt) if rt >= RT_CREDIBILITY_HIGH => 1.0,
            Some(rt) if rt >= RT_CREDIBILITY_FLOOR => {
                (rt as f64 - RT_CREDIBILITY_FLOOR as f64)
                    / (RT_CREDIBILITY_HIGH as f64 - RT_CREDIBILITY_FLOOR as f64)
            }
            Some(_) => 0.0, // RT < 40% " informed avoidance, not undiscovery
        }
    }

    /// Get the genre boost multiplier for a movie.
    ///
    /// Multiplies together the boosts for every matching genre. Using a product
    /// (rather than taking the max) means a "Drama, Romance" film correctly gets
    /// 1.1 - 0.85 = 0.935 " the Romance penalty applies even though Drama is present.
    /// With max(), Drama would always win (1.1) and the Romance penalty would be ignored.
    ///
    /// Unmatched genres contribute 1.0 (neutral), so films with genres not in the
    /// table are not penalised.
    fn calc_genre_boost(&self, genre: Option<&str>) -> f64 {
        let genre = match genre {
            Some(g) => g,
            None => return 1.0,
        };

        // Use the static slice " no HashMap allocation per movie.
        constants::genre_boosts::ALL
            .iter()
            .filter(|(name, _)| genre.contains(name))
            .map(|(_, boost)| *boost)
            .fold(1.0_f64, |acc, b| acc * b)
    }
}

/// Convert a YYYY-MM-DD date string to an approximate day count (for proximity checks).
fn date_to_approx_days(date: &str) -> Option<i64> {
    let parts: Vec<&str> = date.splitn(3, '-').collect();
    let year: i64 = parts.first().and_then(|s| s.parse().ok())?;
    let month: i64 = parts.get(1).and_then(|m| m.parse().ok()).unwrap_or(1);
    let day: i64 = parts.get(2).and_then(|d| d.parse().ok()).unwrap_or(1);
    Some(year * 365 + month * 30 + day)
}

/// Run the full batch scoring pipeline over all movies in the database.
/// Returns the count of movies that received a gem score.
pub async fn run_batch_scoring(conn: &Connection) -> Result<usize> {
    // 1. Load all movies
    let movies = models::get_all_movies_for_scoring(conn).await?;
    let total = movies.len();

    if total == 0 {
        tracing::warn!("No movies found for scoring");
        return Ok(0);
    }

    tracing::info!("Scoring {} movies", total);

    // 1b. Clear all existing gem scores before re-scoring.
    //
    // Without this, films that no longer pass the scoring filters (e.g., their RT data
    // arrived since last run and they now fail the sweet-spot check, or a vote count
    // changed) keep their stale gem_score and rank indefinitely.
    //
    // Clearing first means the scored set is always exactly the films that qualify
    // under the current algorithm and data " no survivors from previous runs.
    let _ = models::clear_all_gem_scores(conn).await;

    // 2. Load blockbuster release dates from the big_hits table.
    //    These are populated by TmdbSyncService::sync_blockbusters(), which fetches
    //    top-popularity movies with no rating filter " blockbusters like Star Wars and
    //    Avengers are above the 6.0"8.0 gem range and would never appear in the gem
    //    candidate pool, so deriving big_hit_dates from that pool is always wrong.
    let big_hit_dates = models::get_big_hit_dates(conn).await.unwrap_or_else(|e| {
        tracing::warn!(
            "Could not load big_hit_dates from DB: {}; obscured signal will be zero",
            e
        );
        Vec::new()
    });

    tracing::info!(
        "Loaded {} blockbuster dates for obscured-by-big-hit calculation",
        big_hit_dates.len()
    );

    // 3. Calculate gem scores for all eligible movies
    let calculator = GemScoreCalculator::new();
    let mut scored: Vec<(i64, f64)> = Vec::new();

    for movie in &movies {
        if let Some(gem_score) = calculator.calculate(movie, &big_hit_dates) {
            scored.push((gem_score.movie_id, gem_score.normalized_score));
        }
    }

    let scored_count = scored.len();

    // 4. Sort by score descending and assign ranks
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // 5. Normalize: divide all scores by the population max so the top gem = 1.0.
    //    Raw scores rarely reach theoretical max (all components peak simultaneously,
    //    which requires e.g. year_decay=1.0 AND vote_ratio=1.0 AND imdb=7.2 exactly AND
    //    big-hit overlap). Without normalization, Sorcerer scores ~66% instead of 100%.
    //    After normalization, the best gem in the current pool always scores 100% and
    //    all other gems are expressed relative to it.
    if let Some(&(_, max_score)) = scored.first() {
        if max_score > 0.0 && max_score < 1.0 {
            tracing::info!(
                "Normalizing {} scores by population max {:.4} (top gem  100%)",
                scored_count,
                max_score
            );
            for (_, score) in &mut scored {
                *score = (*score / max_score).clamp(0.0, 1.0);
            }
        }
    }

    // 6. Persist scores and ranks back to the database
    for (rank_idx, (movie_id, score)) in scored.iter().enumerate() {
        let rank = (rank_idx + 1) as i64;
        if let Err(e) = models::update_movie_gem_score(conn, *movie_id, *score, rank).await {
            tracing::error!("Failed to update gem score for movie {}: {}", movie_id, e);
        }
    }

    tracing::info!(
        "Batch scoring complete: {}/{} movies scored (normalized to population max)",
        scored_count,
        total
    );

    Ok(scored_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gem_finder_shared::types::Movie;

    /// Build a test movie. `tmdb_rating` and `imdb_rating` are kept separate:
    /// pass `imdb_rating: None` to simulate TMDB-only data (the common case after ingestion).
    ///
    /// Leaves `rt_critic_score: None` " the scoring filter only fires when RT data is
    /// present and below 65, so tests without RT data exercise the "no gate" path.
    fn make_movie(
        id: i64,
        tmdb_rating: f64,
        imdb_rating: Option<f64>,
        votes: i64,
        year: i32,
        genre: &str,
        release_date: &str,
    ) -> Movie {
        Movie {
            id: Some(id),
            tmdb_id: id,
            imdb_id: None,
            title: format!("Test Movie {}", id),
            year: Some(year),
            genre: Some(genre.to_string()),
            director: None,
            overview: None,
            poster_url: None,
            tmdb_rating: Some(tmdb_rating),
            tmdb_vote_count: Some(votes),
            imdb_rating,
            imdb_vote_count: imdb_rating.map(|_| votes), // only set if IMDb data exists
            rt_critic_score: None,
            rt_audience_score: None,
            gem_score: None,
            gem_rank: None,
            release_date: Some(release_date.to_string()),
            revenue: None,
            collection_id: None,
            keywords: None,
            created_at: None,
            updated_at: None,
        }
    }

    // "" Diagnostic: print score breakdown table ----------------------------"""""
    //
    // Run with: cargo test test_score_breakdown -- --nocapture

    #[test]
    fn test_score_breakdown_table() {
        let calc = GemScoreCalculator::new();

        // Representative movies: sorcerer-profile, hurt-locker-profile, blockbuster,
        // low-vote gem, recent underdog, and a movie with RT disparity.
        let cases: Vec<(&str, Movie, Vec<String>)> = vec![
            (
                "Sorcerer-profile (1977 thriller, low votes, near Star Wars)",
                {
                    let mut m = make_movie(
                        1,
                        7.5,
                        Some(7.6),
                        25_000,
                        1977,
                        "Drama, Thriller",
                        "1977-06-24",
                    );
                    m.rt_critic_score = Some(92);
                    m.rt_audience_score = Some(88);
                    m
                },
                vec!["1977-05-25".to_string()], // Star Wars opened 1 month prior
            ),
            (
                "Hurt-Locker-profile (2009, low votes, war/drama)",
                {
                    let mut m = make_movie(
                        2,
                        7.5,
                        Some(7.5),
                        200_000,
                        2009,
                        "Drama, Thriller, War",
                        "2009-06-26",
                    );
                    m.rt_critic_score = Some(97);
                    m.rt_audience_score = Some(83);
                    m
                },
                vec![],
            ),
            (
                "Dinner-in-America-profile (2020, very low votes, indie)",
                {
                    let mut m = make_movie(
                        3,
                        7.6,
                        Some(7.5),
                        5_000,
                        2020,
                        "Comedy, Drama",
                        "2020-01-20",
                    );
                    m.rt_critic_score = Some(82);
                    m.rt_audience_score = Some(95);
                    m
                },
                vec![],
            ),
            (
                "Blockbuster-adjacent (high votes - should score low)",
                make_movie(4, 7.5, Some(7.5), 1_500_000, 2015, "Action", "2015-04-24"),
                vec![],
            ),
            (
                "Below sweet spot (rating 5.8 - filtered out)",
                make_movie(5, 5.8, None, 50_000, 2010, "Action", "2010-01-01"),
                vec![],
            ),
            (
                "RT critic=45% - wildcard (dampened vote_ratio, still scores)",
                {
                    let mut m = make_movie(6, 7.2, None, 8_000, 2015, "Drama", "2015-03-01");
                    m.rt_critic_score = Some(45);
                    m
                },
                vec![],
            ),
            (
                "No RT data - multiplier=0.8 (benefit of doubt for old films)",
                make_movie(7, 7.2, None, 8_000, 2015, "Drama", "2015-03-01"),
                vec![],
            ),
        ];

        eprintln!("\n{:-<96}", "");
        eprintln!(
            "{:<50} {:>6} {:>6} {:>6} {:>6} {:>7} {:>6} {:>7}",
            "Movie", "imdb", "vote_r", "yr_dec", "obscrd", "rt_mult", "boost", "TOTAL%"
        );
        eprintln!("{:-<96}", "");

        for (label, movie, big_hits) in &cases {
            match calc.calculate(movie, big_hits) {
                Some(s) => {
                    let c = &s.components;
                    eprintln!(
                        "{:<50} {:>6.3} {:>6.3} {:>6.3} {:>6.3} {:>7.3} {:>6.3} {:>6.1}%",
                        &label[..label.len().min(49)],
                        c.imdb_rating_score,
                        c.vote_ratio_score,
                        c.year_decay_score,
                        c.obscured_by_big_hit_score,
                        c.rt_credibility_multiplier,
                        c.genre_boost,
                        s.normalized_score * 100.0
                    );
                }
                None => {
                    eprintln!(
                        "{:<50}  -- filtered out (below sweet spot or min votes) --",
                        label
                    );
                }
            }
        }
        eprintln!("{:-<96}\n", "");

        // Assertions " Sorcerer-profile should score highest, blockbuster lower than it.
        let sorcerer = calc.calculate(&cases[0].1, &cases[0].2).unwrap();
        let blockbuster = calc.calculate(&cases[3].1, &cases[3].2).unwrap();
        assert!(
            sorcerer.normalized_score > blockbuster.normalized_score,
            "Sorcerer-profile ({:.1}%) should beat blockbuster-adjacent ({:.1}%)",
            sorcerer.normalized_score * 100.0,
            blockbuster.normalized_score * 100.0
        );
    }

    // // Unit: rating sweet spot ------------------------------------------""""""""""

    #[test]
    fn test_imdb_sweet_spot_peaks_at_center() {
        let calc = GemScoreCalculator::new();
        // Standard range - quadratic peaking at 7.2
        let center = calc.calc_imdb_rating_score(7.2, IMDB_GEM_MIN);
        let edge_low = calc.calc_imdb_rating_score(6.5, IMDB_GEM_MIN);
        let edge_high = calc.calc_imdb_rating_score(7.9, IMDB_GEM_MIN);
        let outside_high = calc.calc_imdb_rating_score(8.5, IMDB_GEM_MIN);
        let outside_low_std = calc.calc_imdb_rating_score(5.9, IMDB_GEM_MIN); // below standard floor

        // RT-endorsed lower tier - linear 0.0->0.4 across [5.5, 6.5)
        let rt_endorsed_low = calc.calc_imdb_rating_score(5.5, IMDB_GEM_MIN_RT_ENDORSED); // bottom = 0
        let rt_endorsed_mid = calc.calc_imdb_rating_score(5.9, IMDB_GEM_MIN_RT_ENDORSED); // mid
        let rt_endorsed_top = calc.calc_imdb_rating_score(6.4, IMDB_GEM_MIN_RT_ENDORSED); // near top of tier

        eprintln!("std: center(7.2)={:.3}, edge_low(6.5)={:.3}, edge_high(7.9)={:.3}, out_high(8.5)={:.3}, out_low(5.9)={:.3}",
            center, edge_low, edge_high, outside_high, outside_low_std);
        eprintln!(
            "rt_endorsed: floor(5.5)={:.3}, mid(5.9)={:.3}, near_top(6.4)={:.3}",
            rt_endorsed_low, rt_endorsed_mid, rt_endorsed_top
        );

        assert!(center > edge_low, "Center should beat low edge");
        assert!(center > edge_high, "Center should beat high edge");
        assert_eq!(outside_high, 0.0, "Above range should be 0");
        assert_eq!(
            outside_low_std, 0.0,
            "Below standard floor with no RT endorsement = 0"
        );
        assert_eq!(rt_endorsed_low, 0.0, "At endorsed floor (5.5) score is 0");
        assert!(
            rt_endorsed_mid > 0.0 && rt_endorsed_mid < 0.4,
            "Mid endorsed tier partial score"
        );
        assert!(rt_endorsed_top < 0.4, "Near top of endorsed tier below cap");
        assert!(
            rt_endorsed_top > rt_endorsed_mid,
            "Higher rating = higher score in endorsed tier"
        );
        assert!(
            center > rt_endorsed_top,
            "Standard sweet spot beats endorsed tier at same component"
        );
    }

    // // Unit: year decay --------------------------------------------------------""""

    #[test]
    fn test_year_decay_older_scores_higher() {
        let calc = GemScoreCalculator::new();
        // Same rating and votes; only year differs.
        let old = make_movie(1, 7.2, None, 8_000, 1965, "Drama", "1965-03-01");
        let recent = make_movie(2, 7.2, None, 8_000, 2020, "Drama", "2020-03-01");

        let old_score = calc.calculate(&old, &[]).unwrap();
        let recent_score = calc.calculate(&recent, &[]).unwrap();

        eprintln!(
            "year_decay: 1965={:.1}%, 2020={:.1}%",
            old_score.normalized_score * 100.0,
            recent_score.normalized_score * 100.0
        );

        assert!(
            old_score.normalized_score > recent_score.normalized_score,
            "Older movie should score higher due to year decay: old={:.1}%, recent={:.1}%",
            old_score.normalized_score * 100.0,
            recent_score.normalized_score * 100.0
        );
    }

    // // Unit: obscured signal ------------------------------------------"""""""""""

    #[test]
    fn test_obscured_by_big_hit_boosts_score() {
        let calc = GemScoreCalculator::new();
        let movie = make_movie(1, 7.2, None, 8_000, 1977, "Drama", "1977-06-24");

        // Big hit released 1 day after " well within the 4-week window.
        let within_window = vec!["1977-06-25".to_string()];
        // Big hit released 1 year before " outside the window.
        let outside_window = vec!["1976-06-25".to_string()];

        let obscured = calc.calculate(&movie, &within_window).unwrap();
        let not_obscured = calc.calculate(&movie, &outside_window).unwrap();

        eprintln!("obscured_signal: within_window={:.1}% (signal={:.2}), outside_window={:.1}% (signal={:.2})",
            obscured.normalized_score * 100.0, obscured.components.obscured_by_big_hit_score,
            not_obscured.normalized_score * 100.0, not_obscured.components.obscured_by_big_hit_score);

        assert!(
            obscured.normalized_score > not_obscured.normalized_score,
            "Movie near a blockbuster should score higher: obscured={:.1}%, not_obscured={:.1}%",
            obscured.normalized_score * 100.0,
            not_obscured.normalized_score * 100.0
        );
        assert_eq!(obscured.components.obscured_by_big_hit_score, 1.0);
        assert_eq!(not_obscured.components.obscured_by_big_hit_score, 0.0);
    }

    // // Unit: movies outside sweet spot are filtered ----------------------------"

    #[test]
    fn test_outside_sweet_spot_returns_none() {
        let calc = GemScoreCalculator::new();

        // Above sweet spot ceiling - filtered regardless of RT
        let blockbuster = make_movie(
            1,
            8.4,
            Some(8.4),
            1_200_000,
            2012,
            "Action, Science Fiction",
            "2012-05-04",
        );
        assert!(
            calc.calculate(&blockbuster, &[]).is_none(),
            "Rating 8.4 is above the sweet spot ceiling - should return None"
        );

        // Below floor with NO RT data - filtered (standard floor 6.5 applies)
        let below_floor_no_rt = make_movie(2, 5.5, None, 50_000, 2010, "Action", "2010-01-01");
        assert!(
            calc.calculate(&below_floor_no_rt, &[]).is_none(),
            "Rating 5.5 with no RT data - standard floor 6.5 applies, should return None"
        );

        // Below floor with LOW RT (<75%) - filtered (not endorsed, standard floor applies)
        let mut below_floor_low_rt = make_movie(3, 5.8, None, 10_000, 2010, "Drama", "2010-01-01");
        below_floor_low_rt.rt_critic_score = Some(60);
        assert!(
            calc.calculate(&below_floor_low_rt, &[]).is_none(),
            "Rating 5.8 with RT 60% - below endorsement threshold, standard floor applies"
        );

        // Below floor with HIGH RT (75%) - PASSES (RT-endorsed lower floor 5.5 applies)
        let mut below_floor_high_rt = make_movie(4, 5.8, None, 10_000, 2010, "Drama", "2010-01-01");
        below_floor_high_rt.rt_critic_score = Some(100);
        assert!(
            calc.calculate(&below_floor_high_rt, &[]).is_some(),
            "Rating 5.8 with RT 100% - RT-endorsed floor 5.5 applies, should score"
        );

        // Below the endorsed floor entirely - filtered even with high RT
        let mut below_endorsed_floor =
            make_movie(5, 5.3, None, 10_000, 2010, "Drama", "2010-01-01");
        below_endorsed_floor.rt_critic_score = Some(100);
        assert!(
            calc.calculate(&below_endorsed_floor, &[]).is_none(),
            "Rating 5.3 is below even the RT-endorsed floor (5.5) - should return None"
        );

        eprintln!("sweet_spot filter: 8.4->None ok, 5.5(no RT)->None ok, 5.8(RT60%)->None ok, 5.8(RT100%)->Some ok, 5.3(RT100%)->None ok");
    }

    #[test]
    fn test_rt_endorsed_low_rating_scores() {
        // The Tunnel (2011) profile: IMDb 5.8, RT 100%, ~10k votes, age 15 years.
        // Under the standard floor (6.5) this film is invisible. With RT endorsement
        // (RT  75% unlocks floor 5.5) it should receive a non-zero gem score.
        let calc = GemScoreCalculator::new();

        let mut tunnel_profile = make_movie(99, 5.8, None, 10_000, 2011, "Horror", "2011-04-22");
        tunnel_profile.rt_critic_score = Some(100);

        let score = calc.calculate(&tunnel_profile, &[]);
        assert!(
            score.is_some(),
            "The Tunnel profile (IMDb 5.8, RT 100%) should score"
        );

        let score = score.unwrap();
        assert!(score.normalized_score > 0.0, "Score should be positive");
        assert_eq!(
            score.components.rt_credibility_multiplier, 1.0,
            "RT 100% -> full multiplier"
        );

        // Same film WITHOUT RT data - standard floor applies, should be filtered
        let tunnel_no_rt = make_movie(100, 5.8, None, 10_000, 2011, "Horror", "2011-04-22");
        assert!(
            calc.calculate(&tunnel_no_rt, &[]).is_none(),
            "Same film without RT data should be filtered (standard floor 6.5 applies)"
        );

        // Same film with RT 60% (below endorsement threshold) - filtered
        let mut tunnel_low_rt = make_movie(101, 5.8, None, 10_000, 2011, "Horror", "2011-04-22");
        tunnel_low_rt.rt_critic_score = Some(60);
        assert!(
            calc.calculate(&tunnel_low_rt, &[]).is_none(),
            "RT 60% is below endorsement threshold (75%) - standard floor applies"
        );

        eprintln!(
            "RT-endorsed: tunnel(RT100%)={:.1}% ok, tunnel(no RT)=None ok, tunnel(RT60%)=None ok",
            score.normalized_score * 100.0
        );
    }

    #[test]
    fn test_below_min_votes_returns_none() {
        let calc = GemScoreCalculator::new();
        let low_votes = make_movie(1, 7.2, None, 100, 2000, "Drama", "2000-01-01");
        assert!(
            calc.calculate(&low_votes, &[]).is_none(),
            "Fewer than MIN_IMDB_VOTES should return None"
        );
        eprintln!("min_votes filter: 100 votes -> None ok");
    }

    // -- Unit: vote ratio -- high rating + low votes scores higher --------------
    //
    // NOTE: Seeded-gem ranks are calibration diagnostics, never acceptance
    // criteria (ETHOS.md: "Sorcerer's role: calibration only"). Population-
    // level calibration runs in db_tests::test_known_gems_calibration_report
    // against a real database; seed it with `cargo run -- seed-test-data`.

    #[test]
    fn test_vote_ratio_low_votes_score_higher() {
        let calc = GemScoreCalculator::new();
        // Same rating, different vote counts " fewer votes = more undiscovered.
        let undiscovered = make_movie(1, 7.5, None, 1_000, 2005, "Drama", "2005-01-01");
        let well_known = make_movie(2, 7.5, None, 500_000, 2005, "Drama", "2005-01-01");

        let u = calc.calculate(&undiscovered, &[]).unwrap();
        let w = calc.calculate(&well_known, &[]).unwrap();

        eprintln!(
            "vote_ratio: undiscovered(1k votes)={:.3}, well_known(500k votes)={:.3}",
            u.components.vote_ratio_score, w.components.vote_ratio_score
        );

        assert!(
            u.components.vote_ratio_score > w.components.vote_ratio_score,
            "Low-vote movie should have higher vote_ratio_score: {:.3} vs {:.3}",
            u.components.vote_ratio_score,
            w.components.vote_ratio_score
        );
    }

    // // Unit: RT credibility multiplier ------------------------------------------"

    #[test]
    fn test_rt_credibility_multiplier_higher_rt_gives_higher_multiplier() {
        // Higher RT = higher credibility multiplier on vote_ratio.
        // RT=98% (critics loved it)  multiplier=1.0
        // RT=55% (critics lukewarm)  multiplier=0.5
        // RT=30% (critics panned it)  multiplier=0.0
        let calc = GemScoreCalculator::new();

        let mult_98 = calc.calc_rt_credibility_multiplier(Some(98));
        let mult_55 = calc.calc_rt_credibility_multiplier(Some(55));
        let mult_30 = calc.calc_rt_credibility_multiplier(Some(30));
        let mult_none = calc.calc_rt_credibility_multiplier(None);

        eprintln!(
            "rt_multiplier: 98%={:.3}, 55%={:.3}, 30%={:.3}, None={:.3}",
            mult_98, mult_55, mult_30, mult_none
        );

        assert_eq!(mult_98, 1.0, "RT 98% should give multiplier 1.0");
        assert!(
            mult_55 > 0.0 && mult_55 < 1.0,
            "RT 55% should give partial multiplier"
        );
        assert_eq!(
            mult_30, 0.0,
            "RT 30% should give multiplier 0.0 (below credibility floor)"
        );
        assert!(
            mult_none > 0.5 && mult_none < 1.0,
            "No RT data should give benefit-of-doubt multiplier (0.8)"
        );
        assert!(
            mult_98 > mult_55 && mult_55 > mult_30,
            "Multiplier must increase with RT"
        );
    }

    #[test]
    fn test_low_rt_dampens_vote_ratio_contribution() {
        // Same film, same votes " but high RT vs low RT.
        // High RT: critics endorsed + audiences missed = true hidden gem signal.
        // Low RT: critics panned + audiences stayed away = informed avoidance.
        // The low-RT film should score lower overall because vote_ratio is dampened.
        let calc = GemScoreCalculator::new();

        let mut high_rt = make_movie(1, 7.2, None, 5_000, 2010, "Drama", "2010-05-01");
        high_rt.rt_critic_score = Some(90);

        let mut low_rt = make_movie(2, 7.2, None, 5_000, 2010, "Drama", "2010-05-01");
        low_rt.rt_critic_score = Some(35); // below credibility floor  multiplier = 0.0

        let s_high = calc.calculate(&high_rt, &[]).unwrap();
        let s_low = calc.calculate(&low_rt, &[]).unwrap();

        eprintln!(
            "rt_dampening: high_rt(90%, mult={:.3})={:.1}%, low_rt(35%, mult={:.3})={:.1}%",
            s_high.components.rt_credibility_multiplier,
            s_high.normalized_score * 100.0,
            s_low.components.rt_credibility_multiplier,
            s_low.normalized_score * 100.0,
        );

        assert_eq!(
            s_low.components.rt_credibility_multiplier, 0.0,
            "RT 35% should produce 0.0 credibility multiplier"
        );
        assert!(
            s_high.normalized_score > s_low.normalized_score,
            "High-RT film should outscore low-RT film with same votes: {:.1}% vs {:.1}%",
            s_high.normalized_score * 100.0,
            s_low.normalized_score * 100.0
        );
    }

    #[test]
    fn test_no_rt_data_uses_benefit_of_doubt() {
        let calc = GemScoreCalculator::new();
        let movie = make_movie(1, 7.2, None, 10_000, 2010, "Drama", "2010-05-01");
        let score = calc.calculate(&movie, &[]).unwrap();
        eprintln!(
            "no_rt_multiplier: {:.3} (expected ~0.8)",
            score.components.rt_credibility_multiplier
        );
        assert!(
            (score.components.rt_credibility_multiplier - 0.8).abs() < 0.001,
            "No RT data should give 0.8 benefit-of-doubt multiplier, got {:.3}",
            score.components.rt_credibility_multiplier
        );
    }
}

// ------------------------------------------------------------------------------------""""
// Integration tests " operate against the real local database (gem_finder.db).
//
// These tests are READ-ONLY: they never call update_movie_gem_score or any
// other write function. They fetch the real population, run the calculator
// in memory, and assert ranking properties.
//
// Run:  cargo test -p gem-finder-api db_tests -- --nocapture --test-threads=1
//
//   --nocapture      : required so the score tables print to terminal
//   --test-threads=1 : required; Turso local SQLite does not support concurrent
//                      connections from the same process " parallel tests race
//
// Seed data first (from workspace root):
//   cargo run -p gem-finder-api -- seed-test-data
//
// Skip condition: if the local DB has fewer than 3 scored movies the tests
// print a diagnostic and return without failing.
// ------------------------------------------------------------------------------------""""
#[cfg(test)]
mod db_tests {
    use super::*;
    use gem_finder_db::{models, Database};

    /// Known seeded gems: (display_name, imdb_id).
    const KNOWN_GEMS: &[(&str, &str)] = &[
        ("Sorcerer (1977)", "tt0076740"),
        ("Dinner in America (2020)", "tt9058654"),
        ("The Hurt Locker (2009)", "tt0887912"),
    ];

    /// Scoring result for a single real movie (owned, no lifetime issues).
    struct ScoredMovie {
        title: String,
        year: i32,
        imdb_id: Option<String>,
        votes: i64,
        score: f64,
        components: GemScoreComponents,
    }

    /// Open the real local DB and return a connection.
    ///
    /// Uses CARGO_MANIFEST_DIR (set at compile-time) to construct an absolute
    /// path to the workspace root, then opens gem_finder.db there.
    /// This matches the path used by `cargo run -p gem-finder-api -- seed-test-data`,
    /// which also writes gem_finder.db relative to where it is invoked (workspace root).
    ///
    /// Returns None if the DB file cannot be opened (don't fail the test).
    async fn open_db() -> Option<turso::Connection> {
        // CARGO_MANIFEST_DIR = .../gem-finder/crates/api
        // workspace root     = .../gem-finder  (two levels up)
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent()?.parent()?;
        let db_path = workspace_root.join("gem_finder.db");

        if !db_path.exists() {
            eprintln!("gem_finder.db not found at {} - run: cargo run -p gem-finder-api -- seed-test-data",
                db_path.display());
            return None;
        }

        let db_path_str = db_path.to_str()?;
        let db = Database::new_local(db_path_str).await.ok()?;
        db.connect().await.ok()
    }

    /// Fetch all movies + big_hit_dates, run the calculator, return sorted results.
    async fn score_real_population() -> Option<Vec<ScoredMovie>> {
        let conn = open_db().await?;

        let movies = models::get_all_movies_for_scoring(&conn).await.ok()?;
        let big_hit_dates = models::get_big_hit_dates(&conn).await.unwrap_or_default();
        let calc = GemScoreCalculator::new();

        let mut scored: Vec<ScoredMovie> = movies
            .iter()
            .filter_map(|m| {
                calc.calculate(m, &big_hit_dates).map(|gs| ScoredMovie {
                    title: m.title.clone(),
                    year: m.year.unwrap_or(0),
                    imdb_id: m.imdb_id.clone(),
                    // Show the actual vote count used in scoring (max of imdb/tmdb),
                    // not just the TMDB count " they can differ substantially.
                    votes: m
                        .imdb_vote_count
                        .unwrap_or(0)
                        .max(m.tmdb_vote_count.unwrap_or(0)),
                    score: gs.normalized_score,
                    components: gs.components,
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Some(scored)
    }

    /// Print a ranked score table (top 25 + any known gems outside top 25).
    #[tokio::test]
    async fn test_real_db_score_table() {
        let scored = match score_real_population().await {
            Some(s) if !s.is_empty() => s,
            Some(_) | None => {
                eprintln!("DB has no scored movies - run: cargo run -- seed-test-data");
                return;
            }
        };

        // total_in_db is derived from the same query as `scored` - no second connection needed.
        let total_in_db = {
            let conn = match open_db().await {
                Some(c) => c,
                None => {
                    eprintln!("Could not re-open DB for count");
                    return;
                }
            };
            models::get_all_movies_for_scoring(&conn)
                .await
                .map(|v| v.len())
                .unwrap_or(scored.len())
        };

        eprintln!("--- Real DB Score Table ---");
        eprintln!(
            "  {} total movies in DB, {} passed scoring filter",
            total_in_db,
            scored.len()
        );
        eprintln!(
            "{:<4} {:<37} {:>4} {:>7} {:>10}",
            "Rank", "Title", "Year", "Score%", "Votes"
        );

        let known_imdb_ids: std::collections::HashSet<&str> =
            KNOWN_GEMS.iter().map(|(_, id)| *id).collect();

        let display_limit = 25usize;
        let mut printed_gems = std::collections::HashSet::new();

        for (rank, m) in scored.iter().enumerate() {
            let is_gem = m
                .imdb_id
                .as_deref()
                .is_some_and(|id| known_imdb_ids.contains(id));
            if is_gem {
                printed_gems.insert(m.imdb_id.clone());
            }
            if rank < display_limit || is_gem {
                let marker = if is_gem { "" } else { "  " };
                eprintln!(
                    "{:>4} {}{:<37} {:>4} {:>6.1}% {:>10}",
                    rank + 1,
                    marker,
                    &m.title[..m.title.len().min(37)],
                    m.year,
                    m.score * 100.0,
                    m.votes,
                );
            }
        }
        eprintln!("");

        // Component breakdown for known gems
        eprintln!("--- Known Gem Component Breakdown ---");
        eprintln!(
            "{:<35} {:>6} {:>6} {:>6} {:>6} {:>7} {:>6} {:>7}",
            "Movie", "imdb", "vote_r", "yr_dec", "obscrd", "rt_mult", "boost", "TOTAL%"
        );
        eprintln!("{:-<91}", "");
        for m in &scored {
            let is_gem = m
                .imdb_id
                .as_deref()
                .is_some_and(|id| known_imdb_ids.contains(id));
            if is_gem {
                let c = &m.components;
                eprintln!(
                    "{:<35} {:>6.3} {:>6.3} {:>6.3} {:>6.3} {:>7.3} {:>6.3} {:>6.1}%",
                    &m.title[..m.title.len().min(34)],
                    c.imdb_rating_score,
                    c.vote_ratio_score,
                    c.year_decay_score,
                    c.obscured_by_big_hit_score,
                    c.rt_credibility_multiplier,
                    c.genre_boost,
                    m.score * 100.0,
                );
            }
        }
        eprintln!("{}", "-".repeat(91));
    }

    /// Calibration report for the seeded gems (ETHOS.md — "Sorcerer's role:
    /// calibration only").
    ///
    /// Seeded-gem rank is NEVER a pass/fail condition: an algorithm told
    /// where Sorcerer must land proves nothing. This test prints 🚩 RED FLAG
    /// diagnostics instead of asserting, when:
    /// - a seeded gem scores 0.0 (algorithm produced no signal), or
    /// - no seeded gem ranks in the top 25% of the scored population.
    /// A red flag means: investigate the algorithm or the data.
    #[tokio::test]
    async fn test_known_gems_calibration_report() {
        let scored = match score_real_population().await {
            Some(s) => s,
            None => {
                eprintln!("DB unavailable - skipping");
                return;
            }
        };

        let total = scored.len();

        // With fewer than 10 movies the tier cutoffs are meaningless.
        if total < 10 {
            eprintln!(
                "  Only {} scored movies - calibration needs 10+ for meaningful ranks. Run seed-test-data to populate the DB.",
                total
            );
            for m in &scored {
                if m.score <= 0.0 {
                    eprintln!(
                        "🚩 RED FLAG: {} has gem score 0.0 - no signal. Investigate (ETHOS.md: calibration).",
                        m.title
                    );
                }
            }
            return;
        }

        let top_25_cutoff = ((total as f64 * 0.25).ceil() as usize).max(1);

        eprintln!(
            "Population: {} scored movies, top-25% = rank {}",
            total, top_25_cutoff
        );

        let mut best_known_rank: Option<usize> = None;

        for (label, imdb_id) in KNOWN_GEMS {
            let result = scored
                .iter()
                .enumerate()
                .find(|(_, m)| m.imdb_id.as_deref() == Some(imdb_id));

            match result {
                None => {
                    eprintln!("  {} (imdb:{}) NOT found in scored population - not in DB or below filter threshold", label, imdb_id);
                }
                Some((rank_idx, m)) => {
                    let rank = rank_idx + 1;
                    eprintln!(
                        " {}  rank {}/{} ({:.1}%)",
                        label,
                        rank,
                        total,
                        m.score * 100.0
                    );

                    if m.score <= 0.0 {
                        eprintln!(
                            "🚩 RED FLAG: {} has gem score 0.0 - no signal. Investigate (ETHOS.md: calibration).",
                            label
                        );
                    }

                    if best_known_rank.is_none_or(|best| rank < best) {
                        best_known_rank = Some(rank);
                    }
                }
            }
        }

        match best_known_rank {
            Some(best) if best <= top_25_cutoff => {
                eprintln!(
                    "Calibration OK: best known gem rank {} (top {:.1}%)",
                    best,
                    (best as f64 / total as f64) * 100.0
                );
            }
            Some(best) => {
                eprintln!(
                    "🚩 RED FLAG: best known gem ranked {}/{} - outside top 25% (rank {}). The algorithm should *find* the seeded gems. Investigate (ETHOS.md: calibration).",
                    best, total, top_25_cutoff
                );
            }
            None => {
                eprintln!(
                    "🚩 RED FLAG: no seeded gems found in scored population. Run seed-test-data or check SEEDED_GEMS ingestion."
                );
            }
        }
    }
}
