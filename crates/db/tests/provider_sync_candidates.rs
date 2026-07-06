use gem_finder_db::{migrations, models, Database};

async fn test_conn() -> turso::Connection {
    let db = Database::new_local(":memory:").await.unwrap();
    let conn = db.connect().await.unwrap();
    migrations::run(&conn).await.unwrap();
    conn
}

async fn insert_movie(conn: &turso::Connection, tmdb_id: i64, gem_score: Option<f64>) -> i64 {
    conn.execute(
        "INSERT INTO movies (tmdb_id, title, gem_score) VALUES (?1, ?2, ?3)",
        turso::params![tmdb_id, format!("Movie {tmdb_id}"), gem_score],
    )
    .await
    .unwrap();
    let mut rows = conn
        .query(
            "SELECT id FROM movies WHERE tmdb_id = ?1",
            turso::params![tmdb_id],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().unwrap();
    row.get::<i64>(0).unwrap()
}

#[tokio::test]
async fn acclaimed_and_wildcards_are_provider_sync_candidates() {
    let conn = test_conn().await;

    let scored = insert_movie(&conn, 1, Some(0.9)).await;
    let acclaimed = insert_movie(&conn, 2, None).await;
    let wildcard = insert_movie(&conn, 3, None).await;
    let neither = insert_movie(&conn, 4, None).await;

    conn.execute(
        "INSERT INTO acclaimed (movie_id) VALUES (?1)",
        turso::params![acclaimed],
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO wildcards (movie_id) VALUES (?1)",
        turso::params![wildcard],
    )
    .await
    .unwrap();

    let candidates = models::get_movies_needing_provider_sync(&conn, 100)
        .await
        .unwrap();
    let ids: Vec<i64> = candidates.iter().map(|(id, _)| *id).collect();

    assert!(ids.contains(&scored), "scored movie must be a candidate");
    assert!(
        ids.contains(&acclaimed),
        "acclaimed movie must be a candidate"
    );
    assert!(
        ids.contains(&wildcard),
        "wildcard movie must be a candidate"
    );
    assert!(
        !ids.contains(&neither),
        "unscored non-acclaimed non-wildcard must NOT be a candidate"
    );
    // Scored movies come first (sync priority).
    assert_eq!(
        ids[0], scored,
        "scored movies sort before acclaimed/wildcards"
    );
}

#[tokio::test]
async fn fresh_sync_excludes_candidate() {
    let conn = test_conn().await;
    let acclaimed = insert_movie(&conn, 5, None).await;
    conn.execute(
        "INSERT INTO acclaimed (movie_id) VALUES (?1)",
        turso::params![acclaimed],
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO provider_sync (movie_id, fetched_at) VALUES (?1, datetime('now'))",
        turso::params![acclaimed],
    )
    .await
    .unwrap();

    let candidates = models::get_movies_needing_provider_sync(&conn, 100)
        .await
        .unwrap();
    assert!(
        candidates.is_empty(),
        "freshly synced movie must be skipped"
    );
}
