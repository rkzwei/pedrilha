use gem_finder_db::{migrations, Database};

#[tokio::test]
async fn big_hits_is_keyed_by_tmdb_id_after_migration() {
    let db = Database::new_local(":memory:").await.unwrap();
    let conn = db.connect().await.unwrap();
    migrations::run(&conn).await.unwrap();

    // New schema: inserting a big_hit by tmdb_id (no movies row required) works.
    conn.execute(
        "INSERT INTO big_hits (tmdb_id, year, release_date, popularity_score)
         VALUES (?1, ?2, ?3, ?4)",
        turso::params![680_i64, 1994_i32, "1994-09-10", 55.0_f64],
    )
    .await
    .unwrap();

    let mut rows = conn
        .query(
            "SELECT tmdb_id, release_date FROM big_hits WHERE tmdb_id = 680",
            turso::params![],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().expect("row present");
    assert_eq!(row.get::<i64>(0).unwrap(), 680);
    assert_eq!(row.get::<String>(1).unwrap(), "1994-09-10");
}
