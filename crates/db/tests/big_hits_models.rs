use gem_finder_db::{migrations, models, Database};

async fn setup() -> turso::Connection {
    let db = Database::new_local(":memory:").await.unwrap();
    let conn = db.connect().await.unwrap();
    migrations::run(&conn).await.unwrap();
    conn
}

#[tokio::test]
async fn insert_big_hit_and_read_dates_by_tmdb_id() {
    let conn = setup().await;
    models::insert_big_hit(&conn, 680, 1994, Some("1994-09-10"), 55.0)
        .await
        .unwrap();
    let dates = models::get_big_hit_dates(&conn).await.unwrap();
    assert_eq!(dates, vec!["1994-09-10".to_string()]);
}

#[tokio::test]
async fn scoring_excludes_movies_whose_tmdb_id_is_a_big_hit() {
    let conn = setup().await;
    conn.execute(
        "INSERT INTO movies (tmdb_id, title, year, imdb_rating) VALUES (680, 'Pulp Fiction', 1994, 8.8)",
        turso::params![],
    )
    .await
    .unwrap();
    models::insert_big_hit(&conn, 680, 1994, Some("1994-09-10"), 55.0)
        .await
        .unwrap();

    let scored = models::get_all_movies_for_scoring(&conn).await.unwrap();
    assert!(
        scored.iter().all(|m| m.tmdb_id != 680),
        "a movie whose tmdb_id is a big_hit must be excluded from gem scoring"
    );
}
