use anyhow::Result;
use gem_finder_shared::types::{Movie, MovieSummary, RunLogEntry};
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
             rt_critic_score, rt_audience_score, gem_score, gem_rank, release_date)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
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
        "SELECT id, title, year, genre, director, poster_url, imdb_rating, rt_critic_score, gem_score, gem_rank
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
             created_at, updated_at FROM movies
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
        });
    }
    Ok(results)
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
    conn.execute(
        &format!(
            "INSERT OR IGNORE INTO acclaimed (movie_id)
             SELECT id FROM movies
             WHERE imdb_rating >= {}
               AND rt_critic_score >= {}",
            ACCLAIMED_MIN_IMDB, ACCLAIMED_MIN_RT
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
                    m.gem_score, m.gem_rank
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
/// Uses INSERT OR IGNORE — safe to call after every scoring run.
pub async fn classify_wildcards(conn: &Connection) -> Result<i64> {
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
                    m.gem_score, m.gem_rank
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
            imdb_rating       = COALESCE(?, imdb_rating),
            imdb_vote_count   = COALESCE(?, imdb_vote_count),
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
