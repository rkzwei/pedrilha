use anyhow::Result;
use gem_finder_shared::types::{
    FriendInfo, Movie, MovieProvider, MovieSummary, ProviderInfo, ReceivedRec, RecPublic,
    RegionProviders, RunLogEntry, SentRec, User, WatchState, WatchlistEntry,
};
use std::collections::HashMap;
use turso::transaction::TransactionBehavior;
use turso::{params, Connection, Value};

/// Helper to extract an Option<String> from a Value.
fn value_to_opt_string(v: Value) -> Option<String> {
    v.as_text().map(|s| s.to_owned())
}

/// Helper to extract an Option<f64> from a Value.
fn value_to_opt_f64(v: Value) -> Option<f64> {
    v.as_real()
        .copied()
        .or_else(|| v.as_integer().map(|i| *i as f64))
}

/// Helper to extract an Option<i64> from a Value.
fn value_to_opt_i64(v: Value) -> Option<i64> {
    v.as_integer().copied()
}

/// Helper to extract an Option<i32> from a Value.
fn value_to_opt_i32(v: Value) -> Option<i32> {
    v.as_integer().map(|i| *i as i32)
}

/// Insert or update a movie in the database.
pub async fn upsert_movie(conn: &Connection, movie: &Movie) -> Result<i64> {
    let mut stmt = conn
        .prepare(
            "INSERT INTO movies (tmdb_id, imdb_id, title, year, genre, director, overview,
             poster_url, tmdb_rating, tmdb_vote_count, imdb_rating, imdb_vote_count,
             rt_critic_score, rt_audience_score, gem_score, gem_rank, release_date,
             revenue, collection_id, keywords)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
             ON CONFLICT(tmdb_id) DO UPDATE SET
             imdb_id = COALESCE(excluded.imdb_id, movies.imdb_id),
             title = excluded.title,
             year = COALESCE(excluded.year, movies.year),
             genre = COALESCE(excluded.genre, movies.genre),
             director = COALESCE(excluded.director, movies.director),
             overview = COALESCE(excluded.overview, movies.overview),
             poster_url = COALESCE(excluded.poster_url, movies.poster_url),
             tmdb_rating = COALESCE(excluded.tmdb_rating, movies.tmdb_rating),
             tmdb_vote_count = COALESCE(excluded.tmdb_vote_count, movies.tmdb_vote_count),
             imdb_rating = COALESCE(excluded.imdb_rating, movies.imdb_rating),
             imdb_vote_count = COALESCE(excluded.imdb_vote_count, movies.imdb_vote_count),
             gem_score = COALESCE(excluded.gem_score, movies.gem_score),
             gem_rank = COALESCE(excluded.gem_rank, movies.gem_rank),
             revenue = COALESCE(excluded.revenue, movies.revenue),
             collection_id = COALESCE(excluded.collection_id, movies.collection_id),
 keywords = COALESCE(excluded.keywords, movies.keywords),
             updated_at = datetime('now')
             RETURNING id",
        )
        .await?;

    let mut rows = stmt
        .query(params![
            movie.tmdb_id,
            movie.imdb_id.as_deref(),
            movie.title.as_str(),
            movie.year,
            movie.genre.as_deref(),
            movie.director.as_deref(),
            movie.overview.as_deref(),
            movie.poster_url.as_deref(),
            movie.tmdb_rating,
            movie.tmdb_vote_count,
            movie.imdb_rating,
            movie.imdb_vote_count,
            movie.rt_critic_score,
            movie.rt_audience_score,
            movie.gem_score,
            movie.gem_rank,
            movie.release_date.as_deref(),
            movie.revenue,
            movie.collection_id,
            movie.keywords.as_deref(),
        ])
        .await?;

    let row = rows
        .next()
        .await?
        .ok_or_else(|| anyhow::anyhow!("Failed to upsert movie"))?;

    Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0))
}

/// Get a paginated list of movies sorted by gem score.
pub async fn get_top_gems(
    conn: &Connection,
    page: i32,
    per_page: i32,
    min_year: Option<i32>,
    genre: Option<&str>,
) -> Result<Vec<MovieSummary>> {
    let offset = ((page - 1) * per_page) as i64;
    let limit = per_page as i64;

    // Build query with optional filters. year and limit/offset are integers — safe to inline.
    // genre is user-supplied text and is bound as a parameter to avoid injection.
    //
    // Exclude wildcards from the gems listing: films with rt_critic_score < 50 are divisive
    // (low votes may reflect critical rejection, not undiscovery) and are listed separately
    // at /wildcards. Films with no RT data (rt_critic_score IS NULL) are included — benefit
    // of doubt, especially for old films that predate Rotten Tomatoes.
    let mut sql = String::from(
        "SELECT id, title, year, genre, director, poster_url, imdb_rating, rt_critic_score, gem_score, gem_rank, keywords
         FROM movies WHERE gem_score IS NOT NULL
           AND (rt_critic_score IS NULL OR rt_critic_score >= 50)",
    );

    if let Some(y) = min_year {
        sql.push_str(&format!(" AND year >= {}", y));
    }
    let genre_param: Option<String> = genre.map(|g| format!("%{}%", g));
    if genre_param.is_some() {
        sql.push_str(" AND genre LIKE ?1");
    }

    sql.push_str(" ORDER BY gem_score DESC, gem_rank ASC");
    sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

    let mut stmt = conn.prepare(&sql).await?;
    let mut rows = if let Some(ref g) = genre_param {
        stmt.query(turso::params![g.as_str()]).await?
    } else {
        stmt.query(turso::params![]).await?
    };

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(MovieSummary {
            id: value_to_opt_i64(row.get_value(0)?).unwrap_or(0),
            title: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
            year: value_to_opt_i32(row.get_value(2)?),
            genre: value_to_opt_string(row.get_value(3)?),
            director: value_to_opt_string(row.get_value(4)?),
            poster_url: value_to_opt_string(row.get_value(5)?),
            imdb_rating: value_to_opt_f64(row.get_value(6)?),
            rt_critic_score: value_to_opt_i32(row.get_value(7)?),
            gem_score: value_to_opt_f64(row.get_value(8)?),
            gem_rank: value_to_opt_i64(row.get_value(9)?),
            keywords: value_to_opt_string(row.get_value(10)?),
            watch_badge: None,
        });
    }
    Ok(results)
}

/// Get a single movie by its ID.
pub async fn get_movie_by_id(conn: &Connection, id: i64) -> Result<Option<Movie>> {
    let mut stmt = conn.prepare("SELECT * FROM movies WHERE id = ?1").await?;
    let mut rows = stmt.query(params![id]).await?;

    if let Some(row) = rows.next().await? {
        Ok(Some(Movie {
            id: value_to_opt_i64(row.get_value(0)?),
            tmdb_id: value_to_opt_i64(row.get_value(1)?).unwrap_or(0),
            imdb_id: value_to_opt_string(row.get_value(2)?),
            title: value_to_opt_string(row.get_value(3)?).unwrap_or_default(),
            year: value_to_opt_i32(row.get_value(4)?),
            genre: value_to_opt_string(row.get_value(5)?),
            director: value_to_opt_string(row.get_value(6)?),
            overview: value_to_opt_string(row.get_value(7)?),
            poster_url: value_to_opt_string(row.get_value(8)?),
            tmdb_rating: value_to_opt_f64(row.get_value(9)?),
            tmdb_vote_count: value_to_opt_i64(row.get_value(10)?),
            imdb_rating: value_to_opt_f64(row.get_value(11)?),
            imdb_vote_count: value_to_opt_i64(row.get_value(12)?),
            rt_critic_score: value_to_opt_i32(row.get_value(13)?),
            rt_audience_score: value_to_opt_i32(row.get_value(14)?),
            gem_score: value_to_opt_f64(row.get_value(15)?),
            gem_rank: value_to_opt_i64(row.get_value(16)?),
            release_date: value_to_opt_string(row.get_value(17)?),
            created_at: value_to_opt_string(row.get_value(18)?),
            updated_at: value_to_opt_string(row.get_value(19)?),
            revenue: value_to_opt_i64(row.get_value(20)?),
            collection_id: value_to_opt_i64(row.get_value(21)?),
            keywords: value_to_opt_string(row.get_value(22)?),
            watch_providers: None,
        }))
    } else {
        Ok(None)
    }
}

/// Get all movies for the batch scoring pipeline.
/// Excludes movies that appear in `big_hits` — those are classified blockbusters and
/// must never be scored as hidden gem candidates, even if their TMDB vote counts are low.
pub async fn get_all_movies_for_scoring(conn: &Connection) -> Result<Vec<Movie>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, tmdb_id, imdb_id, title, year, genre, director, overview, poster_url,
             tmdb_rating, tmdb_vote_count, imdb_rating, imdb_vote_count,
             rt_critic_score, rt_audience_score, gem_score, gem_rank, release_date,
             created_at, updated_at, revenue, collection_id, keywords FROM movies
             WHERE id NOT IN (SELECT movie_id FROM big_hits WHERE movie_id IS NOT NULL)",
        )
        .await?;
    let mut rows = stmt.query(turso::params![]).await?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(Movie {
            id: value_to_opt_i64(row.get_value(0)?),
            tmdb_id: value_to_opt_i64(row.get_value(1)?).unwrap_or(0),
            imdb_id: value_to_opt_string(row.get_value(2)?),
            title: value_to_opt_string(row.get_value(3)?).unwrap_or_default(),
            year: value_to_opt_i32(row.get_value(4)?),
            genre: value_to_opt_string(row.get_value(5)?),
            director: value_to_opt_string(row.get_value(6)?),
            overview: value_to_opt_string(row.get_value(7)?),
            poster_url: value_to_opt_string(row.get_value(8)?),
            tmdb_rating: value_to_opt_f64(row.get_value(9)?),
            tmdb_vote_count: value_to_opt_i64(row.get_value(10)?),
            imdb_rating: value_to_opt_f64(row.get_value(11)?),
            imdb_vote_count: value_to_opt_i64(row.get_value(12)?),
            rt_critic_score: value_to_opt_i32(row.get_value(13)?),
            rt_audience_score: value_to_opt_i32(row.get_value(14)?),
            gem_score: value_to_opt_f64(row.get_value(15)?),
            gem_rank: value_to_opt_i64(row.get_value(16)?),
            release_date: value_to_opt_string(row.get_value(17)?),
            created_at: value_to_opt_string(row.get_value(18)?),
            updated_at: value_to_opt_string(row.get_value(19)?),
            revenue: value_to_opt_i64(row.get_value(20)?),
            collection_id: value_to_opt_i64(row.get_value(21)?),
            keywords: value_to_opt_string(row.get_value(22)?),
            watch_providers: None,
        });
    }
    Ok(results)
}

/// Insert an anonymous engagement event into the events table.
#[allow(clippy::too_many_arguments)]
pub async fn insert_event(
    conn: &Connection,
    event_type: &str,
    movie_id: Option<i64>,
    genre: Option<&str>,
    era: Option<i32>,
    section: Option<&str>,
    page_num: Option<i32>,
    session_hash: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO events (event_type, movie_id, genre, era, section, page_num, session_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        turso::params![
            event_type,
            movie_id,
            genre,
            era,
            section,
            page_num,
            session_hash,
        ],
    )
    .await?;
    Ok(())
}

/// Clear gem_score and gem_rank on every movie.
///
/// Called at the start of each batch scoring run so that films which no longer
/// pass the scoring filters (e.g. RT data arrived and they're now outside the
/// sweet spot, or vote counts changed) don't keep stale scores from a prior run.
pub async fn clear_all_gem_scores(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE movies SET gem_score = NULL, gem_rank = NULL, updated_at = datetime('now')
         WHERE gem_score IS NOT NULL OR gem_rank IS NOT NULL",
        turso::params![],
    )
    .await?;
    Ok(())
}

/// Update a movie's gem score and rank in the database.
pub async fn update_movie_gem_score(
    conn: &Connection,
    movie_id: i64,
    score: f64,
    rank: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE movies SET gem_score = ?1, gem_rank = ?2, updated_at = datetime('now') WHERE id = ?3",
        params![score, rank, movie_id],
    )
    .await?;
    Ok(())
}

/// Get the total count of movies with gem scores.
pub async fn get_gems_count(
    conn: &Connection,
    min_year: Option<i32>,
    genre: Option<&str>,
) -> Result<i64> {
    let mut sql = String::from(
        "SELECT COUNT(*) FROM movies WHERE gem_score IS NOT NULL
           AND (rt_critic_score IS NULL OR rt_critic_score >= 50)",
    );
    if let Some(y) = min_year {
        sql.push_str(&format!(" AND year >= {}", y));
    }
    let genre_param: Option<String> = genre.map(|g| format!("%{}%", g));
    if genre_param.is_some() {
        sql.push_str(" AND genre LIKE ?1");
    }

    let mut stmt = conn.prepare(&sql).await?;
    let mut rows = if let Some(ref g) = genre_param {
        stmt.query(turso::params![g.as_str()]).await?
    } else {
        stmt.query(turso::params![]).await?
    };

    if let Some(row) = rows.next().await? {
        Ok(value_to_opt_i64(row.get_value(0)?).unwrap_or(0))
    } else {
        Ok(0)
    }
}

/// Look up a movie by its TMDB id. Used after upserting to retrieve the DB-assigned id.
pub async fn get_movie_by_tmdb_id(conn: &Connection, tmdb_id: i64) -> Result<Option<i64>> {
    let mut stmt = conn
        .prepare("SELECT id FROM movies WHERE tmdb_id = ?1")
        .await?;
    let mut rows = stmt.query(params![tmdb_id]).await?;
    if let Some(row) = rows.next().await? {
        Ok(value_to_opt_i64(row.get_value(0)?))
    } else {
        Ok(None)
    }
}

/// Record a movie as a known blockbuster in the big_hits table.
/// `movie_id` must already exist in the movies table (FK constraint).
/// Uses INSERT OR IGNORE so re-running a sync is safe.
pub async fn insert_big_hit(
    conn: &Connection,
    movie_id: i64,
    year: i32,
    popularity_score: f64,
) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO big_hits (movie_id, year, popularity_score)
         VALUES (?1, ?2, ?3)",
        params![movie_id, year, popularity_score],
    )
    .await?;
    Ok(())
}

/// Get release dates for all recorded blockbusters (big_hits JOIN movies).
/// Used by the batch scoring pipeline to populate the obscured-by-big-hit signal.
pub async fn get_big_hit_dates(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT m.release_date
             FROM big_hits bh
             JOIN movies m ON m.id = bh.movie_id
             WHERE m.release_date IS NOT NULL AND m.release_date != ''",
        )
        .await?;
    let mut rows = stmt.query(turso::params![]).await?;
    let mut dates = Vec::new();
    while let Some(row) = rows.next().await? {
        if let Some(d) = value_to_opt_string(row.get_value(0)?) {
            dates.push(d);
        }
    }
    Ok(dates)
}

// ──────────────────────────────────────────────
// Run Log Functions
// ──────────────────────────────────────────────

/// Insert a run log entry.
pub async fn insert_run_log(
    conn: &Connection,
    level: &str,
    event_type: &str,
    message: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO run_logs (level, event_type, message) VALUES (?1, ?2, ?3)",
        turso::params![level, event_type, message],
    )
    .await?;
    Ok(())
}

/// Get the most recent run log entries (default: last 50).
pub async fn get_recent_run_logs(conn: &Connection, limit: i64) -> Result<Vec<RunLogEntry>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, level, event_type, message, created_at
             FROM run_logs
             ORDER BY id DESC
             LIMIT ?1",
        )
        .await?;
    let mut rows = stmt.query(turso::params![limit]).await?;
    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(RunLogEntry {
            id: value_to_opt_i64(row.get_value(0)?),
            level: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
            event_type: value_to_opt_string(row.get_value(2)?).unwrap_or_default(),
            message: value_to_opt_string(row.get_value(3)?).unwrap_or_default(),
            created_at: value_to_opt_string(row.get_value(4)?),
        });
    }
    Ok(results)
}

/// Get movies that are missing enrichment data (IMDb rating or RT scores).
pub async fn get_movies_needing_enrichment(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<(i64, String, String)>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, imdb_id
             FROM movies
             WHERE imdb_id IS NOT NULL
               AND imdb_id != ''
               AND (imdb_rating IS NULL OR rt_critic_score IS NULL)
             LIMIT ?1",
        )
        .await?;
    let mut rows = stmt.query(turso::params![limit]).await?;
    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        let movie_id = value_to_opt_i64(row.get_value(0)?).unwrap_or(0);
        let title = value_to_opt_string(row.get_value(1)?).unwrap_or_default();
        let imdb_id = value_to_opt_string(row.get_value(2)?).unwrap_or_default();
        results.push((movie_id, title, imdb_id));
    }
    Ok(results)
}

// ──────────────────────────────────────────────
// Acclaimed Films Functions
// ──────────────────────────────────────────────

/// Populate the `acclaimed` table from movies that meet the quality threshold:
/// IMDb ≥ 8.0 AND RT critic ≥ 80.
///
/// INSERT OR IGNORE is idempotent — safe to call repeatedly.
/// Returns the number of rows newly inserted.
pub async fn classify_acclaimed_films(conn: &Connection) -> Result<i64> {
    use gem_finder_shared::constants::{ACCLAIMED_MIN_IMDB, ACCLAIMED_MIN_RT};

    // Franchise/MCU keyword exclusions.
    // Spider-Verse is kept via the carve-out (NOT LIKE '%spider-verse%').
    // batman, dark knight, logan, wolverine, pixar are intentionally not excluded here.
    // COALESCE ensures films with NULL keywords are never wrongly excluded.
    let kw = "
      AND COALESCE(keywords, '') NOT LIKE '%marvel cinematic universe%'
      AND COALESCE(keywords, '') NOT LIKE '%mcu%'
      AND COALESCE(keywords, '') NOT LIKE '%avengers%'
      AND NOT (COALESCE(keywords, '') LIKE '%spider-man%'
               AND COALESCE(keywords, '') NOT LIKE '%spider-verse%')
      AND COALESCE(keywords, '') NOT LIKE '%deadpool%'
      AND COALESCE(keywords, '') NOT LIKE '%walt disney animation%'
      AND COALESCE(keywords, '') NOT LIKE '%dreamworks animation%'";

    // Remove stale entries: scores dropped below threshold, or keywords now match
    // the franchise exclusion list.
    conn.execute(
        &format!(
            "DELETE FROM acclaimed
             WHERE movie_id NOT IN (
                 SELECT id FROM movies
                 WHERE imdb_rating >= {}
                   AND rt_critic_score >= {}
                   {}
             )",
            ACCLAIMED_MIN_IMDB, ACCLAIMED_MIN_RT, kw
        ),
        turso::params![],
    )
    .await?;

    conn.execute(
        &format!(
            "INSERT OR IGNORE INTO acclaimed (movie_id)
             SELECT id FROM movies
             WHERE imdb_rating >= {}
               AND rt_critic_score >= {}
               {}",
            ACCLAIMED_MIN_IMDB, ACCLAIMED_MIN_RT, kw
        ),
        turso::params![],
    )
    .await?;

    // Return count of all acclaimed entries (idempotent — not just new ones).
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM acclaimed").await?;
    let mut rows = stmt.query(turso::params![]).await?;
    if let Some(row) = rows.next().await? {
        Ok(value_to_opt_i64(row.get_value(0)?).unwrap_or(0))
    } else {
        Ok(0)
    }
}

/// Get a paginated list of acclaimed films ordered by IMDb rating desc.
pub async fn get_acclaimed_films(
    conn: &Connection,
    page: i32,
    per_page: i32,
) -> Result<Vec<MovieSummary>> {
    let offset = ((page - 1) * per_page) as i64;
    let limit = per_page as i64;

    let mut stmt = conn
        .prepare(&format!(
            "SELECT m.id, m.title, m.year, m.genre, m.director,
                    m.poster_url, m.imdb_rating, m.rt_critic_score,
                    m.gem_score, m.gem_rank, m.keywords
             FROM acclaimed a
             JOIN movies m ON m.id = a.movie_id
             ORDER BY m.imdb_rating DESC, m.rt_critic_score DESC
             LIMIT {} OFFSET {}",
            limit, offset
        ))
        .await?;
    let mut rows = stmt.query(turso::params![]).await?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(MovieSummary {
            id: value_to_opt_i64(row.get_value(0)?).unwrap_or(0),
            title: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
            year: value_to_opt_i32(row.get_value(2)?),
            genre: value_to_opt_string(row.get_value(3)?),
            director: value_to_opt_string(row.get_value(4)?),
            poster_url: value_to_opt_string(row.get_value(5)?),
            imdb_rating: value_to_opt_f64(row.get_value(6)?),
            rt_critic_score: value_to_opt_i32(row.get_value(7)?),
            gem_score: value_to_opt_f64(row.get_value(8)?),
            gem_rank: value_to_opt_i64(row.get_value(9)?),
            keywords: value_to_opt_string(row.get_value(10)?),
            watch_badge: None,
        });
    }
    Ok(results)
}

/// Total number of acclaimed films in the table.
pub async fn get_acclaimed_count(conn: &Connection) -> Result<i64> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM acclaimed").await?;
    let mut rows = stmt.query(turso::params![]).await?;
    if let Some(row) = rows.next().await? {
        Ok(value_to_opt_i64(row.get_value(0)?).unwrap_or(0))
    } else {
        Ok(0)
    }
}

/// Insert films into the wildcards table post-scoring.
/// Wildcards: gem_score IS NOT NULL and rt_critic_score < 50%.
/// Deletes stale entries (films re-enriched to RT ≥ 50%) before re-inserting.
pub async fn classify_wildcards(conn: &Connection) -> Result<i64> {
    // Remove stale entries: films whose RT score was updated to ≥ 50%
    // or whose gem_score was removed.
    conn.execute(
        "DELETE FROM wildcards
         WHERE movie_id NOT IN (
             SELECT id FROM movies
             WHERE gem_score IS NOT NULL
               AND rt_critic_score IS NOT NULL
               AND rt_critic_score < 50
         )",
        turso::params![],
    )
    .await?;
    conn.execute(
        "INSERT OR IGNORE INTO wildcards (movie_id)
         SELECT id FROM movies
         WHERE gem_score IS NOT NULL
           AND rt_critic_score IS NOT NULL
           AND rt_critic_score < 50",
        turso::params![],
    )
    .await?;

    let mut stmt = conn.prepare("SELECT COUNT(*) FROM wildcards").await?;
    let mut rows = stmt.query(turso::params![]).await?;
    if let Some(row) = rows.next().await? {
        Ok(value_to_opt_i64(row.get_value(0)?).unwrap_or(0))
    } else {
        Ok(0)
    }
}

/// Get a paginated list of wildcard films ordered by gem_score DESC.
pub async fn get_wildcards(
    conn: &Connection,
    page: i32,
    per_page: i32,
) -> Result<Vec<MovieSummary>> {
    let offset = ((page - 1) * per_page) as i64;
    let limit = per_page as i64;

    let mut stmt = conn
        .prepare(&format!(
            "SELECT m.id, m.title, m.year, m.genre, m.director,
                    m.poster_url, m.imdb_rating, m.rt_critic_score,
                    m.gem_score, m.gem_rank, m.keywords
             FROM wildcards w
             JOIN movies m ON m.id = w.movie_id
             ORDER BY m.gem_score DESC
             LIMIT {} OFFSET {}",
            limit, offset
        ))
        .await?;
    let mut rows = stmt.query(turso::params![]).await?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(MovieSummary {
            id: value_to_opt_i64(row.get_value(0)?).unwrap_or(0),
            title: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
            year: value_to_opt_i32(row.get_value(2)?),
            genre: value_to_opt_string(row.get_value(3)?),
            director: value_to_opt_string(row.get_value(4)?),
            poster_url: value_to_opt_string(row.get_value(5)?),
            imdb_rating: value_to_opt_f64(row.get_value(6)?),
            rt_critic_score: value_to_opt_i32(row.get_value(7)?),
            gem_score: value_to_opt_f64(row.get_value(8)?),
            gem_rank: value_to_opt_i64(row.get_value(9)?),
            keywords: value_to_opt_string(row.get_value(10)?),
            watch_badge: None,
        });
    }
    Ok(results)
}

/// Total number of films in the wildcards table.
pub async fn get_wildcards_count(conn: &Connection) -> Result<i64> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM wildcards").await?;
    let mut rows = stmt.query(turso::params![]).await?;
    if let Some(row) = rows.next().await? {
        Ok(value_to_opt_i64(row.get_value(0)?).unwrap_or(0))
    } else {
        Ok(0)
    }
}

// ── Cache-feed queries (full sorted lists, no pagination) ─────────────────────
//
// These power the in-memory TTL cache in the API layer. Each function returns
// the complete sorted list; the API handler applies user-supplied filters and
// pagination on the in-memory Vec, so the DB is only hit on cache misses.

/// All scored hidden gems ordered by gem_score DESC — excludes wildcards (RT < 50).
/// Films with no RT data (NULL) are kept — benefit of the doubt for older films.
pub async fn get_all_gems_for_cache(conn: &Connection) -> Result<Vec<MovieSummary>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, year, genre, director, poster_url,
                    imdb_rating, rt_critic_score, gem_score, gem_rank, keywords
             FROM movies
             WHERE gem_score IS NOT NULL
               AND (rt_critic_score IS NULL OR rt_critic_score >= 50)
             ORDER BY gem_score DESC, gem_rank ASC",
        )
        .await?;
    let mut rows = stmt.query(turso::params![]).await?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(MovieSummary {
            id: value_to_opt_i64(row.get_value(0)?).unwrap_or(0),
            title: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
            year: value_to_opt_i32(row.get_value(2)?),
            genre: value_to_opt_string(row.get_value(3)?),
            director: value_to_opt_string(row.get_value(4)?),
            poster_url: value_to_opt_string(row.get_value(5)?),
            imdb_rating: value_to_opt_f64(row.get_value(6)?),
            rt_critic_score: value_to_opt_i32(row.get_value(7)?),
            gem_score: value_to_opt_f64(row.get_value(8)?),
            gem_rank: value_to_opt_i64(row.get_value(9)?),
            keywords: value_to_opt_string(row.get_value(10)?),
            watch_badge: None,
        });
    }
    Ok(results)
}

/// All acclaimed films ordered by imdb_rating DESC, rt_critic_score DESC.
pub async fn get_all_acclaimed_for_cache(conn: &Connection) -> Result<Vec<MovieSummary>> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.title, m.year, m.genre, m.director,
                    m.poster_url, m.imdb_rating, m.rt_critic_score,
                    m.gem_score, m.gem_rank, m.keywords
             FROM acclaimed a
             JOIN movies m ON m.id = a.movie_id
             ORDER BY m.imdb_rating DESC, m.rt_critic_score DESC",
        )
        .await?;
    let mut rows = stmt.query(turso::params![]).await?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(MovieSummary {
            id: value_to_opt_i64(row.get_value(0)?).unwrap_or(0),
            title: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
            year: value_to_opt_i32(row.get_value(2)?),
            genre: value_to_opt_string(row.get_value(3)?),
            director: value_to_opt_string(row.get_value(4)?),
            poster_url: value_to_opt_string(row.get_value(5)?),
            imdb_rating: value_to_opt_f64(row.get_value(6)?),
            rt_critic_score: value_to_opt_i32(row.get_value(7)?),
            gem_score: value_to_opt_f64(row.get_value(8)?),
            gem_rank: value_to_opt_i64(row.get_value(9)?),
            keywords: value_to_opt_string(row.get_value(10)?),
            watch_badge: None,
        });
    }
    Ok(results)
}

/// All wildcard films ordered by gem_score DESC.
pub async fn get_all_wildcards_for_cache(conn: &Connection) -> Result<Vec<MovieSummary>> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.title, m.year, m.genre, m.director,
                    m.poster_url, m.imdb_rating, m.rt_critic_score,
                    m.gem_score, m.gem_rank, m.keywords
             FROM wildcards w
             JOIN movies m ON m.id = w.movie_id
             ORDER BY m.gem_score DESC",
        )
        .await?;
    let mut rows = stmt.query(turso::params![]).await?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(MovieSummary {
            id: value_to_opt_i64(row.get_value(0)?).unwrap_or(0),
            title: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
            year: value_to_opt_i32(row.get_value(2)?),
            genre: value_to_opt_string(row.get_value(3)?),
            director: value_to_opt_string(row.get_value(4)?),
            poster_url: value_to_opt_string(row.get_value(5)?),
            imdb_rating: value_to_opt_f64(row.get_value(6)?),
            rt_critic_score: value_to_opt_i32(row.get_value(7)?),
            gem_score: value_to_opt_f64(row.get_value(8)?),
            gem_rank: value_to_opt_i64(row.get_value(9)?),
            keywords: value_to_opt_string(row.get_value(10)?),
            watch_badge: None,
        });
    }
    Ok(results)
}

/// Update a movie's enrichment data from OMDb.
/// Only sets fields that are Some — None values leave the existing DB column unchanged.
pub async fn update_movie_enrichment(
    conn: &Connection,
    movie_id: i64,
    imdb_rating: Option<f64>,
    imdb_vote_count: Option<i64>,
    rt_critic_score: Option<i32>,
    rt_audience_score: Option<i32>,
) -> Result<()> {
    conn.execute(
        "UPDATE movies SET
            imdb_rating       = COALESCE(?1, imdb_rating),
            imdb_vote_count   = COALESCE(?2, imdb_vote_count),
            rt_critic_score   = COALESCE(?, rt_critic_score),
            rt_audience_score = COALESCE(?, rt_audience_score)
         WHERE id = ?",
        turso::params![
            imdb_rating,
            imdb_vote_count,
            rt_critic_score,
            rt_audience_score,
            movie_id
        ],
    )
    .await?;
    Ok(())
}

// ── Phase 8: Users ────────────────────────────────────────────────────────────

fn row_to_user(row: &turso::Row) -> Result<User> {
    Ok(User {
        id: value_to_opt_string(row.get_value(0)?).unwrap_or_default(),
        email: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
        username: value_to_opt_string(row.get_value(2)?),
        created_at: value_to_opt_string(row.get_value(3)?),
        last_login: value_to_opt_string(row.get_value(4)?),
    })
}

/// Find a user by email address.
pub async fn find_user_by_email(conn: &Connection, email: &str) -> Result<Option<User>> {
    let mut stmt = conn
        .prepare("SELECT id, email, username, created_at, last_login FROM users WHERE email = ?1")
        .await?;
    let mut rows = stmt.query(params![email]).await?;
    match rows.next().await? {
        Some(row) => Ok(Some(row_to_user(&row)?)),
        None => Ok(None),
    }
}

/// Find a user by their UUID.
pub async fn find_user_by_id(conn: &Connection, user_id: &str) -> Result<Option<User>> {
    let mut stmt = conn
        .prepare("SELECT id, email, username, created_at, last_login FROM users WHERE id = ?1")
        .await?;
    let mut rows = stmt.query(params![user_id]).await?;
    match rows.next().await? {
        Some(row) => Ok(Some(row_to_user(&row)?)),
        None => Ok(None),
    }
}

/// Find a user by email or create a new one. Returns the user either way.
pub async fn find_or_create_user(conn: &Connection, new_id: &str, email: &str) -> Result<User> {
    if let Some(user) = find_user_by_email(conn, email).await? {
        return Ok(user);
    }
    conn.execute(
        "INSERT INTO users (id, email) VALUES (?1, ?2)",
        params![new_id, email],
    )
    .await?;
    find_user_by_email(conn, email)
        .await?
        .ok_or_else(|| anyhow::anyhow!("user insert succeeded but select returned nothing"))
}

/// Record the current time as last_login for a user.
pub async fn touch_user_login(conn: &Connection, user_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE users SET last_login = datetime('now') WHERE id = ?1",
        params![user_id],
    )
    .await?;
    Ok(())
}

/// Set or update a user's username.
/// Returns an error if the username is already taken (UNIQUE constraint violation).
pub async fn set_username(conn: &Connection, user_id: &str, username: &str) -> Result<()> {
    conn.execute(
        "UPDATE users SET username = ?1 WHERE id = ?2",
        params![username, user_id],
    )
    .await?;
    Ok(())
}

/// Returns `true` if the username is not already claimed by any user.
pub async fn username_available(conn: &Connection, username: &str) -> Result<bool> {
    let mut stmt = conn
        .prepare("SELECT 1 FROM users WHERE username = ?1 LIMIT 1")
        .await?;
    let mut rows = stmt.query(params![username]).await?;
    Ok(rows.next().await?.is_none())
}

// ── Phase 8: Cleanup ──────────────────────────────────────────────────────────

/// Delete users who requested a magic link but never verified (no `last_login`)
/// and whose account is older than 24 hours.
///
/// Because `magic_tokens` has `ON DELETE CASCADE` referencing `users`, the
/// dangling tokens are removed automatically by SQLite.
pub async fn delete_unverified_users(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM users
         WHERE last_login IS NULL
           AND created_at < datetime('now', '-24 hours')",
        params![],
    )
    .await?;
    Ok(())
}

/// Delete magic tokens that have passed their `expires_at` timestamp and were
/// never consumed. Handles tokens whose owner *has* verified (last_login set)
/// so they aren't caught by `delete_unverified_users`.
pub async fn delete_expired_tokens(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM magic_tokens
         WHERE expires_at < datetime('now')
           AND used_at IS NULL",
        params![],
    )
    .await?;
    Ok(())
}

// ── Phase 8: Magic tokens ─────────────────────────────────────────────────────

/// Insert a new magic token for a user.
pub async fn create_magic_token(
    conn: &Connection,
    token: &str,
    user_id: &str,
    expires_at: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO magic_tokens (token, user_id, expires_at) VALUES (?1, ?2, ?3)",
        params![token, user_id, expires_at],
    )
    .await?;
    Ok(())
}

/// Verify and consume a magic token. Returns the owning `User` on success.
///
/// Returns `Ok(None)` when the token is unknown, already used, or expired.
pub async fn consume_magic_token(conn: &Connection, token: &str) -> Result<Option<User>> {
    let mut stmt = conn
        .prepare("SELECT user_id, expires_at, used_at FROM magic_tokens WHERE token = ?1")
        .await?;
    let mut rows = stmt.query(params![token]).await?;

    let row = match rows.next().await? {
        Some(r) => r,
        None => return Ok(None),
    };

    let user_id = value_to_opt_string(row.get_value(0)?).unwrap_or_default();
    let expires_at = value_to_opt_string(row.get_value(1)?).unwrap_or_default();
    let used_at = value_to_opt_string(row.get_value(2)?);

    if used_at.is_some() {
        return Ok(None); // already consumed
    }

    // Check expiry using SQLite's datetime comparison
    let mut exp_stmt = conn.prepare("SELECT 1 WHERE ?1 < datetime('now')").await?;
    let mut exp_rows = exp_stmt.query(params![expires_at.as_str()]).await?;
    if exp_rows.next().await?.is_some() {
        return Ok(None); // expired
    }

    conn.execute(
        "UPDATE magic_tokens SET used_at = datetime('now') WHERE token = ?1",
        params![token],
    )
    .await?;

    find_user_by_id(conn, &user_id).await
}

// ── Phase 8: Passkeys ─────────────────────────────────────────────────────────

/// Persist a WebAuthn passkey credential for a user.
///
/// `credential_id` — base64url-encoded credential ID.
/// `passkey_json`  — full `webauthn_rs::Passkey` serialised to JSON; needed
///                   to rebuild the `Passkey` list for authentication challenges.
pub async fn store_passkey(
    conn: &Connection,
    user_id: &str,
    credential_id: &str,
    passkey_json: &str,
    name: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO passkeys (user_id, credential_id, public_key, name)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(credential_id) DO UPDATE SET public_key = excluded.public_key",
        params![user_id, credential_id, passkey_json, name.unwrap_or("")],
    )
    .await?;
    Ok(())
}

/// Load all passkey JSON blobs for a user.
/// Deserialise each with `serde_json::from_str::<webauthn_rs::prelude::Passkey>`.
pub async fn get_user_passkey_jsons(conn: &Connection, user_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT public_key FROM passkeys WHERE user_id = ?1")
        .await?;
    let mut rows = stmt.query(params![user_id]).await?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().await? {
        if let Some(json) = value_to_opt_string(row.get_value(0)?) {
            result.push(json);
        }
    }
    Ok(result)
}

/// Update the sign count after a successful passkey authentication.
pub async fn update_passkey_sign_count(
    conn: &Connection,
    credential_id: &str,
    sign_count: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE passkeys SET sign_count = ?1 WHERE credential_id = ?2",
        params![sign_count, credential_id],
    )
    .await?;
    Ok(())
}

// ── Phase 8: Watchlist ────────────────────────────────────────────────────────

/// Fetch a single watchlist entry for a user × movie pair. Returns `None` if not present.
pub async fn get_watchlist_entry(
    conn: &Connection,
    user_id: &str,
    movie_id: i64,
) -> Result<Option<WatchlistEntry>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, user_id, movie_id, state, user_rating, created_at, updated_at
             FROM watchlist WHERE user_id = ?1 AND movie_id = ?2",
        )
        .await?;
    let mut rows = stmt.query(params![user_id, movie_id]).await?;
    match rows.next().await? {
        Some(row) => Ok(Some(row_to_watchlist_entry(&row)?)),
        None => Ok(None),
    }
}

/// Fetch all watchlist entries for a user, ordered by most recently updated.
pub async fn get_user_watchlist(conn: &Connection, user_id: &str) -> Result<Vec<WatchlistEntry>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, user_id, movie_id, state, user_rating, created_at, updated_at
             FROM watchlist WHERE user_id = ?1 ORDER BY updated_at DESC",
        )
        .await?;
    let mut rows = stmt.query(params![user_id]).await?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().await? {
        result.push(row_to_watchlist_entry(&row)?);
    }
    Ok(result)
}

/// Insert or update a watchlist entry. Uses UPSERT on the (user_id, movie_id) unique constraint.
pub async fn upsert_watchlist_entry(
    conn: &Connection,
    user_id: &str,
    movie_id: i64,
    state: &str,
    user_rating: Option<i32>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO watchlist (user_id, movie_id, state, user_rating, updated_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(user_id, movie_id) DO UPDATE SET
             state       = excluded.state,
             user_rating = excluded.user_rating,
             updated_at  = datetime('now')",
        params![user_id, movie_id, state, user_rating],
    )
    .await?;
    Ok(())
}

/// Remove a watchlist entry. Silently succeeds if the entry does not exist.
pub async fn delete_watchlist_entry(conn: &Connection, user_id: &str, movie_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM watchlist WHERE user_id = ?1 AND movie_id = ?2",
        params![user_id, movie_id],
    )
    .await?;
    Ok(())
}

fn row_to_watchlist_entry(row: &turso::Row) -> Result<WatchlistEntry> {
    let id = value_to_opt_i64(row.get_value(0)?);
    let user_id = value_to_opt_string(row.get_value(1)?).unwrap_or_default();
    let movie_id = value_to_opt_i64(row.get_value(2)?).unwrap_or(0);
    let state_str = value_to_opt_string(row.get_value(3)?).unwrap_or_default();
    let user_rating = value_to_opt_i32(row.get_value(4)?);
    let created_at = value_to_opt_string(row.get_value(5)?);
    let updated_at = value_to_opt_string(row.get_value(6)?);

    let state = WatchState::try_from(state_str.as_str())?;

    Ok(WatchlistEntry {
        id,
        user_id,
        movie_id,
        state,
        user_rating,
        created_at,
        updated_at,
        recommended_by: None,
    })
}

// ── Phase 10: Watch providers ─────────────────────────────────────────────────

/// Access tiers that count as "watchable at no extra cost" on a subscribed service.
pub const INCLUDED_ACCESS: [&str; 3] = ["flatrate", "free", "ads"];

/// Replace all provider rows for one movie × region with a fresh set.
/// `rows` are `(provider_id, provider_name, logo_path, access)` tuples.
pub async fn replace_movie_providers(
    conn: &Connection,
    movie_id: i64,
    region: &str,
    rows: &[(i32, String, Option<String>, String)],
) -> Result<()> {
    conn.execute(
        "DELETE FROM movie_providers WHERE movie_id = ?1 AND region = ?2",
        params![movie_id, region],
    )
    .await?;
    for (provider_id, provider_name, logo_path, access) in rows {
        conn.execute(
            "INSERT OR IGNORE INTO movie_providers
                (movie_id, region, provider_id, provider_name, logo_path, access)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                movie_id,
                region,
                *provider_id,
                provider_name.as_str(),
                logo_path.clone(),
                access.as_str()
            ],
        )
        .await?;
    }
    Ok(())
}

/// Record that a movie's providers were just fetched (drives the 7-day skip window).
pub async fn upsert_provider_sync(
    conn: &Connection,
    movie_id: i64,
    fetched_at: &str,
    tmdb_link: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO provider_sync (movie_id, fetched_at, tmdb_link)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(movie_id) DO UPDATE SET
             fetched_at = excluded.fetched_at,
             tmdb_link  = excluded.tmdb_link",
        params![movie_id, fetched_at, tmdb_link],
    )
    .await?;
    Ok(())
}

/// Sync providers for scored, acclaimed, and wildcard movies whose data is missing or > 7 days stale.
/// Returns `(movie_id, tmdb_id)` pairs.
pub async fn get_movies_needing_provider_sync(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<(i64, i64)>> {
    let mut stmt = conn
        .prepare(
            "SELECT m.id, m.tmdb_id
             FROM movies m
             LEFT JOIN provider_sync ps ON ps.movie_id = m.id
             WHERE (m.gem_score IS NOT NULL
                    OR m.id IN (SELECT movie_id FROM acclaimed)
                    OR m.id IN (SELECT movie_id FROM wildcards))
               AND (ps.fetched_at IS NULL OR ps.fetched_at < datetime('now','-7 days'))
             ORDER BY (m.gem_score IS NOT NULL) DESC, m.gem_score DESC
             LIMIT ?1",
        )
        .await?;
    let mut rows = stmt.query(params![limit]).await?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().await? {
        let movie_id = value_to_opt_i64(row.get_value(0)?).unwrap_or(0);
        let tmdb_id = value_to_opt_i64(row.get_value(1)?).unwrap_or(0);
        result.push((movie_id, tmdb_id));
    }
    Ok(result)
}

/// Bulk-load provider rows for every movie in a region, keyed by movie_id.
/// Used to enrich the in-memory list cache for filtering.
pub async fn get_providers_for_movies(
    conn: &Connection,
    region: &str,
) -> Result<HashMap<i64, Vec<MovieProvider>>> {
    let mut stmt = conn
        .prepare(
            "SELECT movie_id, provider_id, provider_name, logo_path, access
             FROM movie_providers WHERE region = ?1",
        )
        .await?;
    let mut rows = stmt.query(params![region]).await?;
    let mut map: HashMap<i64, Vec<MovieProvider>> = HashMap::new();
    while let Some(row) = rows.next().await? {
        let movie_id = value_to_opt_i64(row.get_value(0)?).unwrap_or(0);
        let provider = MovieProvider {
            provider_id: value_to_opt_i32(row.get_value(1)?).unwrap_or(0),
            provider_name: value_to_opt_string(row.get_value(2)?).unwrap_or_default(),
            logo_path: value_to_opt_string(row.get_value(3)?),
            access: value_to_opt_string(row.get_value(4)?).unwrap_or_default(),
        };
        map.entry(movie_id).or_default().push(provider);
    }
    Ok(map)
}

/// Distinct providers that stream ≥1 catalog title (included tiers only) in a
/// region, with catalog counts. Powers the picker so no dead checkboxes appear.
pub async fn get_distinct_providers(conn: &Connection, region: &str) -> Result<Vec<ProviderInfo>> {
    let mut stmt = conn
        .prepare(
            "SELECT provider_id, provider_name, logo_path, COUNT(DISTINCT movie_id) AS cnt
             FROM movie_providers
             WHERE region = ?1 AND access IN ('flatrate','free','ads')
             GROUP BY provider_id
             ORDER BY cnt DESC, provider_name ASC",
        )
        .await?;
    let mut rows = stmt.query(params![region]).await?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().await? {
        result.push(ProviderInfo {
            provider_id: value_to_opt_i32(row.get_value(0)?).unwrap_or(0),
            name: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
            logo_path: value_to_opt_string(row.get_value(2)?),
            count: value_to_opt_i64(row.get_value(3)?).unwrap_or(0),
        });
    }
    Ok(result)
}

/// All providers for one movie, grouped per region, with a TMDB watch-page link
/// (ToS: no per-title deep links — link to the aggregated TMDB page).
pub async fn get_movie_providers(
    conn: &Connection,
    movie_id: i64,
    tmdb_id: i64,
) -> Result<Vec<RegionProviders>> {
    let mut stmt = conn
        .prepare(
            "SELECT region, provider_id, provider_name, logo_path, access
             FROM movie_providers WHERE movie_id = ?1
             ORDER BY region ASC, access ASC, provider_name ASC",
        )
        .await?;
    let mut rows = stmt.query(params![movie_id]).await?;
    // Preserve region insertion order (US then BR, per ORDER BY).
    let mut order: Vec<String> = Vec::new();
    let mut by_region: HashMap<String, Vec<MovieProvider>> = HashMap::new();
    while let Some(row) = rows.next().await? {
        let region = value_to_opt_string(row.get_value(0)?).unwrap_or_default();
        let provider = MovieProvider {
            provider_id: value_to_opt_i32(row.get_value(1)?).unwrap_or(0),
            provider_name: value_to_opt_string(row.get_value(2)?).unwrap_or_default(),
            logo_path: value_to_opt_string(row.get_value(3)?),
            access: value_to_opt_string(row.get_value(4)?).unwrap_or_default(),
        };
        if !by_region.contains_key(&region) {
            order.push(region.clone());
        }
        by_region.entry(region).or_default().push(provider);
    }
    let result = order
        .into_iter()
        .map(|region| {
            let tmdb_link = Some(format!(
                "https://www.themoviedb.org/movie/{}/watch?locale={}",
                tmdb_id, region
            ));
            let providers = by_region.remove(&region).unwrap_or_default();
            RegionProviders {
                region,
                providers,
                tmdb_link,
            }
        })
        .collect();
    Ok(result)
}

/// A signed-in user's selected services, as `(region, provider_id)` pairs.
pub async fn get_user_providers(conn: &Connection, user_id: &str) -> Result<Vec<(String, i32)>> {
    let mut stmt = conn
        .prepare("SELECT region, provider_id FROM user_providers WHERE user_id = ?1")
        .await?;
    let mut rows = stmt.query(params![user_id]).await?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().await? {
        let region = value_to_opt_string(row.get_value(0)?).unwrap_or_default();
        let provider_id = value_to_opt_i32(row.get_value(1)?).unwrap_or(0);
        result.push((region, provider_id));
    }
    Ok(result)
}

/// Replace a user's entire provider selection (last-write-wins, no merge).
pub async fn replace_user_providers(
    conn: &Connection,
    user_id: &str,
    entries: &[(String, i32)],
) -> Result<()> {
    conn.execute(
        "DELETE FROM user_providers WHERE user_id = ?1",
        params![user_id],
    )
    .await?;
    for (region, provider_id) in entries {
        conn.execute(
            "INSERT OR IGNORE INTO user_providers (user_id, region, provider_id)
             VALUES (?1, ?2, ?3)",
            params![user_id, region.as_str(), *provider_id],
        )
        .await?;
    }
    Ok(())
}

// ──────────────────────────────────────────────
// Phase 11: Friend recommendations (Ethos C1)
// ──────────────────────────────────────────────

/// Canonical friendship pair: (smaller, larger) so UNIQUE(user_a, user_b)
/// covers both directions.
fn canonical_pair<'a>(u1: &'a str, u2: &'a str) -> (&'a str, &'a str) {
    if u1 < u2 {
        (u1, u2)
    } else {
        (u2, u1)
    }
}

/// SELECT fragment shared by every rec query that returns a MovieSummary.
/// Column order matches `row_to_movie_summary` below.
const REC_MOVIE_COLS: &str = "m.id, m.title, m.year, m.genre, m.director,
                    m.poster_url, m.imdb_rating, m.rt_critic_score,
                    m.gem_score, m.gem_rank, m.keywords";

fn row_to_movie_summary(row: &turso::Row) -> Result<MovieSummary> {
    Ok(MovieSummary {
        id: value_to_opt_i64(row.get_value(0)?).unwrap_or(0),
        title: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
        year: value_to_opt_i32(row.get_value(2)?),
        genre: value_to_opt_string(row.get_value(3)?),
        director: value_to_opt_string(row.get_value(4)?),
        poster_url: value_to_opt_string(row.get_value(5)?),
        imdb_rating: value_to_opt_f64(row.get_value(6)?),
        rt_critic_score: value_to_opt_i32(row.get_value(7)?),
        gem_score: value_to_opt_f64(row.get_value(8)?),
        gem_rank: value_to_opt_i64(row.get_value(9)?),
        keywords: value_to_opt_string(row.get_value(10)?),
        watch_badge: None,
    })
}

pub async fn get_username(conn: &Connection, user_id: &str) -> Result<Option<String>> {
    let mut rows = conn
        .query(
            "SELECT username FROM users WHERE id = ?1",
            params![user_id],
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(value_to_opt_string(row.get_value(0)?)),
        None => Ok(None),
    }
}

pub async fn get_user_id_by_username(conn: &Connection, username: &str) -> Result<Option<String>> {
    let mut rows = conn
        .query(
            "SELECT id FROM users WHERE username = ?1",
            params![username],
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(value_to_opt_string(row.get_value(0)?)),
        None => Ok(None),
    }
}

/// Create a share-link rec (no recipient yet). Single INSERT — no tx needed.
pub async fn create_share_rec(
    conn: &Connection,
    sender_id: &str,
    movie_id: i64,
    note: Option<&str>,
    token: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO recommendations (token, sender_id, movie_id, note) VALUES (?1, ?2, ?3, ?4)",
        params![token, sender_id, movie_id, note],
    )
    .await?;
    Ok(())
}

/// Create an in-app rec to an existing friend: rec + receipt atomically.
pub async fn create_direct_rec(
    conn: &mut Connection,
    sender_id: &str,
    recipient_id: &str,
    movie_id: i64,
    note: Option<&str>,
    token: &str,
) -> Result<()> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    tx.execute(
        "INSERT INTO recommendations (token, sender_id, movie_id, note) VALUES (?1, ?2, ?3, ?4)",
        params![token, sender_id, movie_id, note],
    )
    .await?;
    tx.execute(
        "INSERT INTO rec_receipts (rec_id, recipient_id)
         SELECT id, ?2 FROM recommendations WHERE token = ?1",
        params![token, recipient_id],
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Public token lookup: movie summary + sender username + note.
/// Returns None for unknown tokens (route maps to the uniform 404).
pub async fn get_rec_public(conn: &Connection, token: &str) -> Result<Option<RecPublic>> {
    let sql = format!(
        "SELECT {REC_MOVIE_COLS}, u.username, r.note
         FROM recommendations r
         JOIN movies m ON m.id = r.movie_id
         JOIN users u ON u.id = r.sender_id
         WHERE r.token = ?1"
    );
    let mut rows = conn.query(&sql, params![token]).await?;
    match rows.next().await? {
        Some(row) => Ok(Some(RecPublic {
            movie: row_to_movie_summary(&row)?,
            sender_username: value_to_opt_string(row.get_value(11)?)
                .unwrap_or_else(|| "?".to_string()),
            note: value_to_opt_string(row.get_value(12)?),
        })),
        None => Ok(None),
    }
}

/// Claim a rec: receipt + friendship in one IMMEDIATE transaction.
/// Self-claim is a successful no-op. Ok(false) = token unknown.
pub async fn claim_rec(conn: &mut Connection, token: &str, recipient_id: &str) -> Result<bool> {
    // Read sender first (also validates token).
    let sender_id = {
        let mut rows = conn
            .query(
                "SELECT sender_id FROM recommendations WHERE token = ?1",
                params![token],
            )
            .await?;
        match rows.next().await? {
            Some(row) => value_to_opt_string(row.get_value(0)?).unwrap_or_default(),
            None => return Ok(false),
        }
    };
    if sender_id == recipient_id {
        return Ok(true); // self-claim: no-op success
    }
    let (a, b) = canonical_pair(&sender_id, recipient_id);
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    tx.execute(
        "INSERT OR IGNORE INTO rec_receipts (rec_id, recipient_id)
         SELECT id, ?2 FROM recommendations WHERE token = ?1",
        params![token, recipient_id],
    )
    .await?;
    tx.execute(
        "INSERT OR IGNORE INTO friendships (user_a, user_b, origin) VALUES (?1, ?2, 'rec')",
        params![a, b],
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// Revoke a rec (sender only): explicit deletes in one IMMEDIATE transaction.
/// We deliberately do NOT rely on FK ON DELETE actions (turso pre-release
/// enforcement unverified). Friendship is NOT touched — retracting a movie
/// is not unfriending. Ok(false) = token unknown or caller is not the sender
/// (indistinguishable to the API by design — no existence oracle).
pub async fn revoke_rec(conn: &mut Connection, token: &str, sender_id: &str) -> Result<bool> {
    let rec_id = {
        let mut rows = conn
            .query(
                "SELECT id FROM recommendations WHERE token = ?1 AND sender_id = ?2",
                params![token, sender_id],
            )
            .await?;
        match rows.next().await? {
            Some(row) => value_to_opt_i64(row.get_value(0)?).unwrap_or(0),
            None => return Ok(false),
        }
    };
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await?;
    tx.execute(
        "DELETE FROM rec_receipts WHERE rec_id = ?1",
        params![rec_id],
    )
    .await?;
    tx.execute(
        "UPDATE watchlist SET via_rec_id = NULL WHERE via_rec_id = ?1",
        params![rec_id],
    )
    .await?;
    tx.execute(
        "DELETE FROM recommendations WHERE id = ?1",
        params![rec_id],
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// Inbox: recs received by this user, newest first. Grouping per friend
/// happens client-side (list is human-scale).
pub async fn get_received_recs(conn: &Connection, user_id: &str) -> Result<Vec<ReceivedRec>> {
    let sql = format!(
        "SELECT {REC_MOVIE_COLS}, u.username, r.note, r.token, rr.read_at, rr.created_at
         FROM rec_receipts rr
         JOIN recommendations r ON r.id = rr.rec_id
         JOIN movies m ON m.id = r.movie_id
         JOIN users u ON u.id = r.sender_id
         WHERE rr.recipient_id = ?1
         ORDER BY rr.created_at DESC"
    );
    let mut rows = conn.query(&sql, params![user_id]).await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(ReceivedRec {
            movie: row_to_movie_summary(&row)?,
            sender_username: value_to_opt_string(row.get_value(11)?)
                .unwrap_or_else(|| "?".to_string()),
            note: value_to_opt_string(row.get_value(12)?),
            token: value_to_opt_string(row.get_value(13)?).unwrap_or_default(),
            read: value_to_opt_string(row.get_value(14)?).is_some(),
            created_at: value_to_opt_string(row.get_value(15)?).unwrap_or_default(),
        });
    }
    Ok(out)
}

/// Sent recs with claim counts — the revocation surface.
pub async fn get_sent_recs(conn: &Connection, user_id: &str) -> Result<Vec<SentRec>> {
    let sql = format!(
        "SELECT {REC_MOVIE_COLS}, r.note, r.token, r.created_at,
                (SELECT COUNT(*) FROM rec_receipts rr WHERE rr.rec_id = r.id) AS claim_count
         FROM recommendations r
         JOIN movies m ON m.id = r.movie_id
         WHERE r.sender_id = ?1
         ORDER BY r.created_at DESC"
    );
    let mut rows = conn.query(&sql, params![user_id]).await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(SentRec {
            movie: row_to_movie_summary(&row)?,
            note: value_to_opt_string(row.get_value(11)?),
            token: value_to_opt_string(row.get_value(12)?).unwrap_or_default(),
            created_at: value_to_opt_string(row.get_value(13)?).unwrap_or_default(),
            claim_count: value_to_opt_i64(row.get_value(14)?).unwrap_or(0),
        });
    }
    Ok(out)
}

/// Mark one received rec read. Ownership enforced in the WHERE clause.
pub async fn mark_rec_read(conn: &Connection, token: &str, recipient_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE rec_receipts SET read_at = datetime('now')
         WHERE recipient_id = ?2 AND read_at IS NULL
           AND rec_id = (SELECT id FROM recommendations WHERE token = ?1)",
        params![token, recipient_id],
    )
    .await?;
    Ok(())
}

/// Nav badge count. Covered by idx_receipts_recipient.
pub async fn unread_rec_count(conn: &Connection, user_id: &str) -> Result<i64> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM rec_receipts WHERE recipient_id = ?1 AND read_at IS NULL",
            params![user_id],
        )
        .await?;
    match rows.next().await? {
        Some(row) => Ok(value_to_opt_i64(row.get_value(0)?).unwrap_or(0)),
        None => Ok(0),
    }
}

/// This user's friends (username + since). Users without a username set are
/// skipped — they cannot be addressed for direct sends yet.
pub async fn get_friends(conn: &Connection, user_id: &str) -> Result<Vec<FriendInfo>> {
    let mut rows = conn
        .query(
            "SELECT u.username, f.created_at
             FROM friendships f
             JOIN users u ON u.id = CASE WHEN f.user_a = ?1 THEN f.user_b ELSE f.user_a END
             WHERE (f.user_a = ?1 OR f.user_b = ?1) AND u.username IS NOT NULL
             ORDER BY f.created_at DESC",
            params![user_id],
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(FriendInfo {
            username: value_to_opt_string(row.get_value(0)?).unwrap_or_default(),
            since: value_to_opt_string(row.get_value(1)?).unwrap_or_default(),
        });
    }
    Ok(out)
}

pub async fn are_friends(conn: &Connection, user1: &str, user2: &str) -> Result<bool> {
    let (a, b) = canonical_pair(user1, user2);
    let mut rows = conn
        .query(
            "SELECT 1 FROM friendships WHERE user_a = ?1 AND user_b = ?2",
            params![a, b],
        )
        .await?;
    Ok(rows.next().await?.is_some())
}
