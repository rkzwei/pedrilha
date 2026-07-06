use gem_finder_db::{migrations, models, Database};

/// Fixed "today" for deterministic tests.
const CURRENT_YEAR: i32 = 2026;

async fn setup() -> (Database, turso::Connection) {
    let db = Database::new_local(":memory:").await.unwrap();
    let conn = db.connect().await.unwrap();
    migrations::run(&conn).await.unwrap();
    (db, conn)
}

#[allow(clippy::too_many_arguments)]
async fn insert_movie(
    conn: &turso::Connection,
    tmdb_id: i64,
    title: &str,
    year: i32,
    imdb_rating: f64,
    rt_critic_score: i32,
    imdb_vote_count: i64,
) {
    conn.execute(
        "INSERT INTO movies (tmdb_id, title, year, imdb_rating, rt_critic_score, imdb_vote_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        turso::params![
            tmdb_id,
            title,
            year,
            imdb_rating,
            rt_critic_score,
            imdb_vote_count
        ],
    )
    .await
    .unwrap();
}

async fn acclaimed_titles(conn: &turso::Connection) -> Vec<String> {
    let mut rows = conn
        .query(
            "SELECT m.title FROM acclaimed a JOIN movies m ON m.id = a.movie_id ORDER BY m.title",
            turso::params![],
        )
        .await
        .unwrap();
    let mut titles = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        titles.push(row.get::<String>(0).unwrap());
    }
    titles
}

#[tokio::test]
async fn audience_canonized_classic_qualifies_despite_low_rt() {
    let (_db, conn) = setup().await;
    // Forrest Gump-like: critics lukewarm (RT 71), audience canonized it
    // (2.3M votes at 8.8) over 30+ years.
    insert_movie(&conn, 13, "Forrest Gump", 1994, 8.8, 71, 2_300_000).await;

    models::classify_acclaimed_films(&conn, CURRENT_YEAR)
        .await
        .unwrap();

    assert_eq!(
        acclaimed_titles(&conn).await,
        vec!["Forrest Gump"],
        "high-vote 20+ year old classic must qualify without RT >= 80"
    );
}

#[tokio::test]
async fn recent_high_vote_film_does_not_qualify_via_audience_branch() {
    let (_db, conn) = setup().await;
    // Recent blockbuster: huge votes but hasn't survived 20 years of judgment,
    // and RT below the critic bar.
    insert_movie(
        &conn,
        14,
        "Recent Blockbuster",
        CURRENT_YEAR - 5,
        8.5,
        70,
        1_000_000,
    )
    .await;

    models::classify_acclaimed_films(&conn, CURRENT_YEAR)
        .await
        .unwrap();

    assert!(
        acclaimed_titles(&conn).await.is_empty(),
        "film younger than 20 years must not qualify via the audience branch"
    );
}

#[tokio::test]
async fn old_low_vote_film_does_not_qualify_via_audience_branch() {
    let (_db, conn) = setup().await;
    // Old and well-rated but without mass audience canonization or critic backing.
    insert_movie(&conn, 15, "Obscure Oldie", 1990, 8.2, 70, 100_000).await;

    models::classify_acclaimed_films(&conn, CURRENT_YEAR)
        .await
        .unwrap();

    assert!(
        acclaimed_titles(&conn).await.is_empty(),
        "old film below the vote floor must not qualify via the audience branch"
    );
}

#[tokio::test]
async fn critic_branch_still_admits_rt_certified_films() {
    let (_db, conn) = setup().await;
    // Existing behavior preserved: IMDb >= 8.0 AND RT >= 80 qualifies
    // regardless of vote count or age.
    insert_movie(&conn, 16, "Parasite", 2019, 8.5, 99, 900_000).await;

    models::classify_acclaimed_films(&conn, CURRENT_YEAR)
        .await
        .unwrap();

    assert_eq!(
        acclaimed_titles(&conn).await,
        vec!["Parasite"],
        "RT >= 80 critic branch must keep working unchanged"
    );
}

#[tokio::test]
async fn reclassification_does_not_evict_audience_canonized_classic() {
    let (_db, conn) = setup().await;
    insert_movie(&conn, 13, "Forrest Gump", 1994, 8.8, 71, 2_300_000).await;

    // Run twice: the stale-entry DELETE must use the same criteria as the
    // INSERT, or audience-branch films get evicted on the next run.
    models::classify_acclaimed_films(&conn, CURRENT_YEAR)
        .await
        .unwrap();
    models::classify_acclaimed_films(&conn, CURRENT_YEAR)
        .await
        .unwrap();

    assert_eq!(
        acclaimed_titles(&conn).await,
        vec!["Forrest Gump"],
        "reclassification must not evict audience-canonized classics"
    );
}

#[tokio::test]
async fn imdb_floor_still_applies_to_audience_branch() {
    let (_db, conn) = setup().await;
    // Massive votes and old, but below the IMDb 8.0 floor.
    insert_movie(&conn, 17, "Popular But Mid", 1995, 7.8, 70, 1_500_000).await;

    models::classify_acclaimed_films(&conn, CURRENT_YEAR)
        .await
        .unwrap();

    assert!(
        acclaimed_titles(&conn).await.is_empty(),
        "IMDb >= 8.0 floor applies to both branches"
    );
}
