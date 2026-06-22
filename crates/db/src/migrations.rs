use anyhow::Result;
use turso::Connection;

/// Run all pending database migrations.
pub async fn run(conn: &Connection) -> Result<()> {
    create_movies_table(conn).await?;
    create_watchlist_table(conn).await?;
    create_big_hits_table(conn).await?;
    create_acclaimed_table(conn).await?;
    create_run_logs_table(conn).await?;
    create_indexes(conn).await?;
    Ok(())
}

async fn create_movies_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS movies (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tmdb_id INTEGER UNIQUE NOT NULL,
            imdb_id TEXT,
            title TEXT NOT NULL,
            year INTEGER,
            genre TEXT,
            director TEXT,
            overview TEXT,
            poster_url TEXT,
            tmdb_rating REAL,
            tmdb_vote_count INTEGER,
            imdb_rating REAL,
            imdb_vote_count INTEGER,
            rt_critic_score INTEGER,
            rt_audience_score INTEGER,
            gem_score REAL,
            gem_rank INTEGER,
            release_date TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        )",
        turso::params![],
    )
    .await?;
    Ok(())
}

async fn create_watchlist_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS watchlist (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            movie_id INTEGER NOT NULL REFERENCES movies(id),
            status TEXT NOT NULL DEFAULT 'to_watch' CHECK(status IN ('to_watch', 'watching', 'watched')),
            added_at TEXT DEFAULT (datetime('now'))
        )",
        turso::params![],
    )
    .await?;
    Ok(())
}

async fn create_big_hits_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS big_hits (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            movie_id INTEGER NOT NULL REFERENCES movies(id),
            year INTEGER NOT NULL,
            box_office_millions REAL,
            popularity_score REAL
        )",
        turso::params![],
    )
    .await?;
    Ok(())
}

async fn create_acclaimed_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS acclaimed (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            movie_id INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
            created_at TEXT DEFAULT (datetime('now')),
            UNIQUE(movie_id)
        )",
        turso::params![],
    )
    .await?;
    Ok(())
}

async fn create_run_logs_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS run_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            level TEXT NOT NULL DEFAULT 'info',
            event_type TEXT NOT NULL,
            message TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now'))
        )",
        turso::params![],
    )
    .await?;
    Ok(())
}

async fn create_indexes(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_movies_gem_score ON movies(gem_score DESC)",
        turso::params![],
    )
    .await?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_movies_year ON movies(year DESC)",
        turso::params![],
    )
    .await?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_movies_tmdb_id ON movies(tmdb_id)",
        turso::params![],
    )
    .await?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_watchlist_status ON watchlist(status)",
        turso::params![],
    )
    .await?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_movies_imdb_rating ON movies(imdb_rating DESC)",
        turso::params![],
    )
    .await?;
    Ok(())
}
