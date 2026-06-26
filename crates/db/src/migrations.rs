use anyhow::Result;
use turso::Connection;

/// Run all pending database migrations in version order.
/// Each migration runs exactly once; applied versions are recorded in
/// `schema_migrations`. Safe to call on every startup.
pub async fn run(conn: &Connection) -> Result<()> {
    bootstrap_migrations_table(conn).await?;
    let applied = applied_versions(conn).await?;

    if !applied.contains(&1) {
        migrate_v1(conn).await?;
        record_version(conn, 1).await?;
    }
    if !applied.contains(&2) {
        migrate_v2(conn).await?;
        record_version(conn, 2).await?;
    }
    if !applied.contains(&3) {
        migrate_v3(conn).await?;
        record_version(conn, 3).await?;
    }
    if !applied.contains(&4) {
        migrate_v4(conn).await?;
        record_version(conn, 4).await?;
    }
    if !applied.contains(&5) {
        migrate_v5(conn).await?;
        record_version(conn, 5).await?;
    }

    Ok(())
}

// ── Migration bookkeeping ────────────────────────────────────────────────────

async fn bootstrap_migrations_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version   INTEGER PRIMARY KEY,
            applied_at TEXT DEFAULT (datetime('now'))
        )",
        turso::params![],
    )
    .await?;
    Ok(())
}

async fn applied_versions(conn: &Connection) -> Result<Vec<i64>> {
    let mut rows = conn
        .query(
            "SELECT version FROM schema_migrations ORDER BY version",
            turso::params![],
        )
        .await?;
    let mut versions = Vec::new();
    while let Some(row) = rows.next().await? {
        versions.push(row.get::<i64>(0)?);
    }
    Ok(versions)
}

async fn record_version(conn: &Connection, version: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO schema_migrations (version) VALUES (?1)",
        turso::params![version],
    )
    .await?;
    Ok(())
}

// ── Migration v1: core tables (phases 1–6) ──────────────────────────────────

async fn migrate_v1(conn: &Connection) -> Result<()> {
    // movies
    conn.execute(
        "CREATE TABLE IF NOT EXISTS movies (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            tmdb_id          INTEGER UNIQUE NOT NULL,
            imdb_id          TEXT,
            title            TEXT NOT NULL,
            year             INTEGER,
            genre            TEXT,
            director         TEXT,
            overview         TEXT,
            poster_url       TEXT,
            tmdb_rating      REAL,
            tmdb_vote_count  INTEGER,
            imdb_rating      REAL,
            imdb_vote_count  INTEGER,
            rt_critic_score  INTEGER,
            rt_audience_score INTEGER,
            gem_score        REAL,
            gem_rank         INTEGER,
            release_date     TEXT,
            created_at       TEXT DEFAULT (datetime('now')),
            updated_at       TEXT DEFAULT (datetime('now'))
        )",
        turso::params![],
    )
    .await?;

    // big_hits – blockbusters used to compute obscured-by signal
    conn.execute(
        "CREATE TABLE IF NOT EXISTS big_hits (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            movie_id            INTEGER NOT NULL REFERENCES movies(id),
            year                INTEGER NOT NULL,
            box_office_millions REAL,
            popularity_score    REAL
        )",
        turso::params![],
    )
    .await?;

    // acclaimed – movies meeting the high-bar critic + community threshold
    conn.execute(
        "CREATE TABLE IF NOT EXISTS acclaimed (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            movie_id   INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
            created_at TEXT DEFAULT (datetime('now')),
            UNIQUE(movie_id)
        )",
        turso::params![],
    )
    .await?;

    // wildcards – algorithmically strong but low RT critic score
    conn.execute(
        "CREATE TABLE IF NOT EXISTS wildcards (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            movie_id   INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
            created_at TEXT DEFAULT (datetime('now')),
            UNIQUE(movie_id)
        )",
        turso::params![],
    )
    .await?;

    // run_logs – admin operation history
    conn.execute(
        "CREATE TABLE IF NOT EXISTS run_logs (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            level      TEXT NOT NULL DEFAULT 'info',
            event_type TEXT NOT NULL,
            message    TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now'))
        )",
        turso::params![],
    )
    .await?;

    // indexes
    for ddl in [
        "CREATE INDEX IF NOT EXISTS idx_movies_gem_score    ON movies(gem_score DESC)",
        "CREATE INDEX IF NOT EXISTS idx_movies_year         ON movies(year DESC)",
        "CREATE INDEX IF NOT EXISTS idx_movies_tmdb_id      ON movies(tmdb_id)",
        "CREATE INDEX IF NOT EXISTS idx_movies_imdb_rating  ON movies(imdb_rating DESC)",
    ] {
        conn.execute(ddl, turso::params![]).await?;
    }

    tracing::info!("Applied migration v1: core tables");
    Ok(())
}

// ── Migration v2: user accounts + watchlist (phase 8) ───────────────────────

async fn migrate_v2(conn: &Connection) -> Result<()> {
    // Drop legacy Phase-1 placeholder watchlist (wrong schema – no user_id,
    // wrong state values, no user_rating). No production data to preserve.
    conn.execute("DROP TABLE IF EXISTS watchlist", turso::params![])
        .await?;

    // users – one row per registered email address
    conn.execute(
        "CREATE TABLE IF NOT EXISTS users (
            id         TEXT PRIMARY KEY,          -- UUID v4
            email      TEXT UNIQUE NOT NULL,
            username   TEXT UNIQUE,               -- user-chosen display name; NULL until set
            created_at TEXT DEFAULT (datetime('now')),
            last_login TEXT
        )",
        turso::params![],
    )
    .await?;

    // magic_tokens – one-time passwordless auth tokens
    conn.execute(
        "CREATE TABLE IF NOT EXISTS magic_tokens (
            token      TEXT PRIMARY KEY,          -- UUID v4, unguessable
            user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at TEXT NOT NULL,             -- ISO-8601 UTC
            used_at    TEXT,                      -- NULL until consumed
            created_at TEXT DEFAULT (datetime('now'))
        )",
        turso::params![],
    )
    .await?;

    // watchlist – user ↔ movie relationship with optional 1-10 rating
    conn.execute(
        "CREATE TABLE IF NOT EXISTS watchlist (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id     TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            movie_id    INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
            state       TEXT    NOT NULL CHECK(state IN ('want_to_watch','watched','not_interested')),
            user_rating INTEGER CHECK(user_rating IS NULL OR (user_rating >= 1 AND user_rating <= 10)),
            created_at  TEXT DEFAULT (datetime('now')),
            updated_at  TEXT DEFAULT (datetime('now')),
            UNIQUE(user_id, movie_id)
        )",
        turso::params![],
    )
    .await?;

    // passkeys – WebAuthn credentials registered by a user after magic-link bootstrap
    conn.execute(
        "CREATE TABLE IF NOT EXISTS passkeys (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            credential_id TEXT NOT NULL UNIQUE,   -- base64url-encoded credential ID
            public_key    TEXT NOT NULL,           -- webauthn-rs serialized JSON blob
            sign_count    INTEGER NOT NULL DEFAULT 0,
            name          TEXT,                    -- user label, e.g. 'MacBook Touch ID'
            created_at    TEXT DEFAULT (datetime('now'))
        )",
        turso::params![],
    )
    .await?;

    // indexes
    for ddl in [
        "CREATE INDEX IF NOT EXISTS idx_watchlist_user_id  ON watchlist(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_watchlist_movie_id ON watchlist(movie_id)",
        "CREATE INDEX IF NOT EXISTS idx_magic_tokens_user  ON magic_tokens(user_id)",
        "CREATE INDEX IF NOT EXISTS idx_passkeys_user_id   ON passkeys(user_id)",
    ] {
        conn.execute(ddl, turso::params![]).await?;
    }

    tracing::info!("Applied migration v2: user accounts + watchlist");
    Ok(())
}

// ── Migration v3: cleanup indexes ───────────────────────────────────────────

async fn migrate_v3(conn: &Connection) -> Result<()> {
    for ddl in [
        // Used by delete_unverified_users to find old unverified accounts efficiently.
        "CREATE INDEX IF NOT EXISTS idx_users_created_at    ON users(created_at)",
        // Used by delete_expired_tokens to purge stale tokens efficiently.
        "CREATE INDEX IF NOT EXISTS idx_magic_tokens_expires ON magic_tokens(expires_at)",
    ] {
        conn.execute(ddl, turso::params![]).await?;
    }
    tracing::info!("Applied migration v3: cleanup indexes");
    Ok(())
}

// ── Migration v4: revenue + collection_id columns ───────────────────────────

async fn migrate_v4(conn: &Connection) -> Result<()> {
    conn.execute(
        "ALTER TABLE movies ADD COLUMN revenue INTEGER",
        turso::params![],
    )
    .await?;
    conn.execute(
        "ALTER TABLE movies ADD COLUMN collection_id INTEGER",
        turso::params![],
    )
    .await?;
    tracing::info!("Applied migration v4: revenue + collection_id columns");
    Ok(())
}

// ── Migration v5: keywords column ───────────────────────────────────────────

async fn migrate_v5(conn: &Connection) -> Result<()> {
    conn.execute(
        "ALTER TABLE movies ADD COLUMN keywords TEXT",
        turso::params![],
    )
    .await?;
    tracing::info!("Applied migration v5: keywords column");
    Ok(())
}
