use anyhow::Result;
use turso::Builder;

/// Internal database variant - either local or synced with Turso Cloud.
enum InnerDb {
    Local(turso::Database),
    Remote(turso::sync::Database),
}

/// Wrapper around the Turso database connection.
///
/// Turso is a superset of SQLite with optional cloud sync.
///
/// # Local Usage
/// ```no_run
/// # async fn doc() -> anyhow::Result<()> {
/// use gem_finder_db::Database;
/// let db = Database::new_local(":memory:").await?;
/// # Ok(())
/// # }
/// ```
///
/// # Turso Cloud Remote Usage
/// ```no_run
/// # async fn doc() -> anyhow::Result<()> {
/// use gem_finder_db::Database;
/// let db = Database::new_remote(
///     "local.db",
///     "libsql://your-db.turso.io",
///     "your-auth-token",
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub struct Database {
    inner: InnerDb,
}

impl Database {
    /// Create a new local database (file or in-memory).
    pub async fn new_local(path: &str) -> Result<Self> {
        let inner = Builder::new_local(path).build().await?;
        let db = Self {
            inner: InnerDb::Local(inner),
        };
        // Skip pragmas for in-memory DBs (WAL not supported; used in tests only).
        if path != ":memory:" {
            if let Err(e) = db.apply_local_pragmas().await {
                tracing::warn!("SQLite pragma setup failed (non-fatal): {}", e);
            }
        }
        Ok(db)
    }

    /// Apply WAL mode and performance pragmas to a local SQLite file.
    ///
    /// WAL allows multiple concurrent readers while a write is in progress —
    /// critical for serving many users without read-blocking on admin ops.
    async fn apply_local_pragmas(&self) -> Result<()> {
        let conn = self.connect().await?;

        // journal_mode returns a row ("wal") — must be drained via query, not execute.
        let mut rows = conn
            .query("PRAGMA journal_mode=WAL", turso::params![])
            .await?;
        while rows.next().await?.is_some() {}

        // Remaining pragmas return no rows.
        for pragma in [
            "PRAGMA synchronous=NORMAL",  // safe with WAL; faster than FULL
            "PRAGMA cache_size=-65536",   // 64 MB page cache
            "PRAGMA busy_timeout=5000",   // wait 5s on lock contention, don't 500
            "PRAGMA temp_store=MEMORY",   // temp tables in RAM
            "PRAGMA mmap_size=134217728", // 128 MB memory-mapped I/O
        ] {
            conn.execute(pragma, turso::params![]).await?;
        }

        Ok(())
    }

    /// Create a new remote database that syncs with Turso Cloud.
    pub async fn new_remote(local_path: &str, remote_url: &str, auth_token: &str) -> Result<Self> {
        let inner = turso::sync::Builder::new_remote(local_path)
            .with_remote_url(remote_url)
            .with_auth_token(auth_token)
            .build()
            .await?;
        Ok(Self {
            inner: InnerDb::Remote(inner),
        })
    }

    /// Create a database connection from environment variables.
    ///
    /// Reads `TURSO_DATABASE_URL` and `TURSO_AUTH_TOKEN` from the environment
    /// for remote connections. Falls back to a local SQLite file if neither
    /// is set (defaults to `gem_finder.db`).
    pub async fn from_env() -> Result<Self> {
        // Filter out empty strings so setting a variable to "" in docker-compose
        // (to override a SECRETS.env value) is treated as "not set".
        let url = std::env::var("TURSO_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok()
            .filter(|s| !s.is_empty());

        match url {
            Some(remote_url) if remote_url.starts_with("libsql://") => {
                let token = std::env::var("TURSO_AUTH_TOKEN")
                    .expect("TURSO_AUTH_TOKEN must be set when using TURSO_DATABASE_URL");
                Self::new_remote("gem_finder.db", &remote_url, &token).await
            }
            Some(local_path) => Self::new_local(&local_path).await,
            None => Self::new_local("gem_finder.db").await,
        }
    }

    /// Get a connection to execute queries.
    pub async fn connect(&self) -> Result<turso::Connection> {
        let conn = match &self.inner {
            InnerDb::Local(db) => db.connect()?,
            InnerDb::Remote(db) => db.connect().await?,
        };
        // Concurrent writers (claim/revoke transactions) queue up to 5s
        // instead of failing immediately with a busy error. `PRAGMA
        // busy_timeout` set in apply_local_pragmas() is connection-scoped in
        // SQLite and does not carry over to connections opened later — it
        // only ever applied to the one throwaway connection used to set WAL
        // mode at startup. This call is what actually takes effect per
        // request/test connection.
        if let Err(e) = conn.busy_timeout(std::time::Duration::from_secs(5)) {
            tracing::warn!("busy_timeout setup failed (non-fatal): {}", e);
        }
        Ok(conn)
    }

    /// Push local changes to remote (only for synced databases).
    pub async fn push(&self) -> Result<()> {
        if let InnerDb::Remote(db) = &self.inner {
            db.push().await?;
        }
        Ok(())
    }

    /// Pull remote changes to local (only for synced databases).
    /// Returns true if changes were applied.
    pub async fn pull(&self) -> Result<bool> {
        if let InnerDb::Remote(db) = &self.inner {
            let changed = db.pull().await?;
            return Ok(changed);
        }
        Ok(false)
    }

    /// Check if this is a remote (synced) database.
    pub fn is_remote(&self) -> bool {
        matches!(self.inner, InnerDb::Remote(_))
    }
}

/// Create an in-memory database (useful for testing).
pub async fn in_memory_db() -> Result<Database> {
    Database::new_local(":memory:").await
}
