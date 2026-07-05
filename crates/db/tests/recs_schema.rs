use gem_finder_db::{migrations, Database};

#[tokio::test]
async fn migration_v8_creates_rec_tables() {
    let db = Database::new_local(":memory:").await.unwrap();
    let conn = db.connect().await.unwrap();
    migrations::run(&conn).await.unwrap();

    for table in ["recommendations", "rec_receipts", "friendships"] {
        let mut rows = conn
            .query(
                "SELECT name FROM sqlite_master WHERE type='table' AND name=?1",
                turso::params![table],
            )
            .await
            .unwrap();
        assert!(
            rows.next().await.unwrap().is_some(),
            "table {table} missing"
        );
    }

    // watchlist.via_rec_id exists (insert with it succeeds)
    conn.execute(
        "INSERT INTO users (id, email) VALUES ('u1', 'a@b.c')",
        turso::params![],
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO movies (tmdb_id, title) VALUES (1, 'M')",
        turso::params![],
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO watchlist (user_id, movie_id, state, via_rec_id) VALUES ('u1', 1, 'want_to_watch', NULL)",
        turso::params![],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn migration_v8_is_idempotent_on_rerun() {
    let db = Database::new_local(":memory:").await.unwrap();
    let conn = db.connect().await.unwrap();
    migrations::run(&conn).await.unwrap();
    migrations::run(&conn).await.unwrap(); // must not error
}
