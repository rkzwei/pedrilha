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
        Ok(Self {
            inner: InnerDb::Local(inner),
        })
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
        let url = std::env::var("TURSO_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok();

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
        match &self.inner {
            InnerDb::Local(db) => Ok(db.connect()?),
            InnerDb::Remote(db) => Ok(db.connect().await?),
        }
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
