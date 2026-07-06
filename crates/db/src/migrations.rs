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
    if !applied.contains(&6) {
        migrate_v6(conn).await?;
        record_version(conn, 6).await?;
    }
    if !applied.contains(&7) {
        migrate_v7(conn).await?;
        record_version(conn, 7).await?;
    }
    if !applied.contains(&8) {
        migrate_v8(conn).await?;
        record_version(conn, 8).await?;
    }
    if !applied.contains(&9) {
        migrate_v9(conn).await?;
        record_version(conn, 9).await?;
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

// ── Migration v6: events table (analytics) ──────────────────────────────────

async fn migrate_v6(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS events (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            event_type   TEXT NOT NULL
                         CHECK(event_type IN (
                             'movie_view',
                             'search_used',
                             'filter_genre',
                             'filter_era',
                             'pagination',
                             'section_view'
                         )),
            movie_id     INTEGER REFERENCES movies(id) ON DELETE SET NULL,
            genre        TEXT,
            era          INTEGER,
            section      TEXT
                         CHECK(section IS NULL OR section IN ('gems','acclaimed','wildcards')),
            page_num     INTEGER,
            session_hash TEXT NOT NULL,
            created_at   TEXT DEFAULT (datetime('now'))
        )",
        turso::params![],
    )
    .await?;

    for ddl in [
        "CREATE INDEX IF NOT EXISTS idx_events_event_type   ON events(event_type)",
        "CREATE INDEX IF NOT EXISTS idx_events_movie_id     ON events(movie_id)",
        "CREATE INDEX IF NOT EXISTS idx_events_session_hash ON events(session_hash)",
        "CREATE INDEX IF NOT EXISTS idx_events_created_at   ON events(created_at DESC)",
    ] {
        conn.execute(ddl, turso::params![]).await?;
    }

    tracing::info!("Applied migration v6: events table");
    Ok(())
}

// ── Migration v7: watch providers (phase 10) ────────────────────────────────

async fn migrate_v7(conn: &Connection) -> Result<()> {
    // movie_providers – one row per (movie, region, provider, access-tier).
    // access: 'flatrate'|'free'|'ads' = included; 'rent'|'buy' = paid rental.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS movie_providers (
            movie_id      INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
            region        TEXT    NOT NULL,        -- 'US' | 'BR'
            provider_id   INTEGER NOT NULL,        -- TMDB provider id
            provider_name TEXT    NOT NULL,
            logo_path     TEXT,
            access        TEXT    NOT NULL CHECK(access IN ('flatrate','free','ads','rent','buy')),
            PRIMARY KEY (movie_id, region, provider_id, access)
        )",
        turso::params![],
    )
    .await?;

    // provider_sync – per-movie fetch bookkeeping (skip if fetched < 7 days ago).
    conn.execute(
        "CREATE TABLE IF NOT EXISTS provider_sync (
            movie_id   INTEGER PRIMARY KEY REFERENCES movies(id) ON DELETE CASCADE,
            fetched_at TEXT NOT NULL,
            tmdb_link  TEXT
        )",
        turso::params![],
    )
    .await?;

    // user_providers – signed-in users' selected services, per region.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS user_providers (
            user_id     TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            region      TEXT    NOT NULL,
            provider_id INTEGER NOT NULL,
            PRIMARY KEY (user_id, region, provider_id)
        )",
        turso::params![],
    )
    .await?;

    for ddl in [
        "CREATE INDEX IF NOT EXISTS idx_mp_filter    ON movie_providers(region, provider_id, access)",
        "CREATE INDEX IF NOT EXISTS idx_mp_movie     ON movie_providers(movie_id)",
        "CREATE INDEX IF NOT EXISTS idx_up_user      ON user_providers(user_id)",
    ] {
        conn.execute(ddl, turso::params![]).await?;
    }

    tracing::info!("Applied migration v7: watch providers");
    Ok(())
}

// ── Migration v8: friend recommendations (word-of-mouth, Ethos C1) ──────────

async fn migrate_v8(conn: &Connection) -> Result<()> {
    // recommendations – one row per "recommend" action. Addressed externally
    // ONLY by `token` (128-bit random); the integer PK never leaves the API.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS recommendations (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            token      TEXT    NOT NULL UNIQUE,   -- uuid v4 simple (32 hex chars)
            sender_id  TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            movie_id   INTEGER NOT NULL REFERENCES movies(id) ON DELETE CASCADE,
            note       TEXT    CHECK(note IS NULL OR length(note) <= 140),
            created_at TEXT DEFAULT (datetime('now'))
        )",
        turso::params![],
    )
    .await?;

    // rec_receipts – who received/claimed a rec. One link can be claimed by
    // several users (group-chat fan-out); UNIQUE makes re-claims idempotent.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS rec_receipts (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            rec_id       INTEGER NOT NULL REFERENCES recommendations(id) ON DELETE CASCADE,
            recipient_id TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            read_at      TEXT,                    -- NULL = unread (nav badge)
            created_at   TEXT DEFAULT (datetime('now')),
            UNIQUE(rec_id, recipient_id)
        )",
        turso::params![],
    )
    .await?;

    // friendships – formed ONLY by a claimed rec in v1. `origin='search'` is
    // reserved for a future username-search flow (do not remove).
    // Canonical ordering user_a < user_b makes the pair unique regardless of direction.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS friendships (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            user_a     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            user_b     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            origin     TEXT NOT NULL CHECK(origin IN ('rec','search')),
            created_at TEXT DEFAULT (datetime('now')),
            UNIQUE(user_a, user_b),
            CHECK(user_a < user_b)
        )",
        turso::params![],
    )
    .await?;

    // Watchlist rows remember which rec brought them in ("de {username}" tag).
    // Revocation explicitly NULLs this (we do not rely on FK actions — see
    // revoke_rec), the REFERENCES clause is documentation.
    conn.execute(
        "ALTER TABLE watchlist ADD COLUMN via_rec_id INTEGER REFERENCES recommendations(id)",
        turso::params![],
    )
    .await?;

    for ddl in [
        "CREATE INDEX IF NOT EXISTS idx_recs_sender       ON recommendations(sender_id)",
        "CREATE INDEX IF NOT EXISTS idx_receipts_recipient ON rec_receipts(recipient_id, read_at)",
        "CREATE INDEX IF NOT EXISTS idx_receipts_rec       ON rec_receipts(rec_id)",
        "CREATE INDEX IF NOT EXISTS idx_friendships_a      ON friendships(user_a)",
        "CREATE INDEX IF NOT EXISTS idx_friendships_b      ON friendships(user_b)",
    ] {
        conn.execute(ddl, turso::params![]).await?;
    }

    tracing::info!("Applied migration v8: friend recommendations");
    Ok(())
}

// ── Migration v9: decouple big_hits from movies (key by tmdb_id) ─────────────

async fn migrate_v9(conn: &Connection) -> Result<()> {
    // Blockbusters are a scoring signal, not catalog rows. Keying big_hits by
    // tmdb_id stops the blockbuster sync from creating imdb_id-less `movies`
    // stubs that can never be enriched (and thus never reach the acclaimed tier).
    // SQLite/libSQL has no DROP CONSTRAINT, so this is a table recreate.
    conn.execute(
        "CREATE TABLE IF NOT EXISTS big_hits_new (
            id               INTEGER PRIMARY KEY AUTOINCREMENT,
            tmdb_id          INTEGER NOT NULL,
            year             INTEGER NOT NULL,
            release_date     TEXT,
            popularity_score REAL,
            UNIQUE(tmdb_id)
        )",
        turso::params![],
    )
    .await?;

    // Backfill from the old movie-keyed table (skip any orphaned rows).
    conn.execute(
        "INSERT OR IGNORE INTO big_hits_new (tmdb_id, year, release_date, popularity_score)
         SELECT m.tmdb_id, bh.year, m.release_date, bh.popularity_score
         FROM big_hits bh
         JOIN movies m ON m.id = bh.movie_id",
        turso::params![],
    )
    .await?;

    conn.execute("DROP TABLE big_hits", turso::params![]).await?;
    conn.execute(
        "ALTER TABLE big_hits_new RENAME TO big_hits",
        turso::params![],
    )
    .await?;

    tracing::info!("Applied migration v9: big_hits keyed by tmdb_id");
    Ok(())
}
