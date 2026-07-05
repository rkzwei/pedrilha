use gem_finder_db::{migrations, models, Database};

async fn setup() -> (Database, turso::Connection) {
    let db = Database::new_local(":memory:").await.unwrap();
    let conn = db.connect().await.unwrap();
    migrations::run(&conn).await.unwrap();
    // Two users + one movie fixture
    for (id, email, username) in [
        ("user-aaa", "a@x.com", Some("alice")),
        ("user-bbb", "b@x.com", Some("bob")),
        ("user-ccc", "c@x.com", None),
    ] {
        conn.execute(
            "INSERT INTO users (id, email, username) VALUES (?1, ?2, ?3)",
            turso::params![id, email, username],
        )
        .await
        .unwrap();
    }
    conn.execute(
        "INSERT INTO movies (tmdb_id, title, year) VALUES (100, 'Sorcerer', 1977)",
        turso::params![],
    )
    .await
    .unwrap();
    (db, conn)
}

async fn movie_id(conn: &turso::Connection) -> i64 {
    let mut rows = conn
        .query("SELECT id FROM movies LIMIT 1", turso::params![])
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
}

#[tokio::test]
async fn claim_creates_receipt_and_canonical_friendship() {
    let (db, conn) = setup().await;
    let mid = movie_id(&conn).await;
    models::create_share_rec(&conn, "user-bbb", mid, Some("said it was great"), "tok1")
        .await
        .unwrap();

    let mut claim_conn = db.connect().await.unwrap();
    let claimed = models::claim_rec(&mut claim_conn, "tok1", "user-aaa").await.unwrap();
    assert!(claimed);

    // Friendship stored canonically: 'user-aaa' < 'user-bbb'
    let mut rows = conn
        .query(
            "SELECT user_a, user_b, origin FROM friendships",
            turso::params![],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().expect("friendship row missing");
    assert_eq!(row.get::<String>(0).unwrap(), "user-aaa");
    assert_eq!(row.get::<String>(1).unwrap(), "user-bbb");
    assert_eq!(row.get::<String>(2).unwrap(), "rec");

    // Inbox shows it, unread
    let received = models::get_received_recs(&conn, "user-aaa").await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].sender_username, "bob");
    assert_eq!(received[0].note.as_deref(), Some("said it was great"));
    assert!(!received[0].read);
    assert_eq!(models::unread_rec_count(&conn, "user-aaa").await.unwrap(), 1);
}

#[tokio::test]
async fn double_claim_is_idempotent() {
    let (db, conn) = setup().await;
    let mid = movie_id(&conn).await;
    models::create_share_rec(&conn, "user-bbb", mid, None, "tok2")
        .await
        .unwrap();
    let mut c1 = db.connect().await.unwrap();
    assert!(models::claim_rec(&mut c1, "tok2", "user-aaa").await.unwrap());
    let mut c2 = db.connect().await.unwrap();
    assert!(models::claim_rec(&mut c2, "tok2", "user-aaa").await.unwrap());
    let received = models::get_received_recs(&conn, "user-aaa").await.unwrap();
    assert_eq!(received.len(), 1, "second claim must not duplicate");
}

#[tokio::test]
async fn self_claim_is_noop() {
    let (db, conn) = setup().await;
    let mid = movie_id(&conn).await;
    models::create_share_rec(&conn, "user-bbb", mid, None, "tok3")
        .await
        .unwrap();
    let mut c = db.connect().await.unwrap();
    assert!(models::claim_rec(&mut c, "tok3", "user-bbb").await.unwrap());
    let received = models::get_received_recs(&conn, "user-bbb").await.unwrap();
    assert!(received.is_empty(), "self-claim must not create a receipt");
    let mut rows = conn
        .query("SELECT COUNT(*) FROM friendships", turso::params![])
        .await
        .unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        0,
        "self-claim must not create a friendship"
    );
}

#[tokio::test]
async fn multi_claim_fans_out() {
    let (db, conn) = setup().await;
    let mid = movie_id(&conn).await;
    models::create_share_rec(&conn, "user-bbb", mid, None, "tok4")
        .await
        .unwrap();
    let mut c1 = db.connect().await.unwrap();
    models::claim_rec(&mut c1, "tok4", "user-aaa").await.unwrap();
    let mut c2 = db.connect().await.unwrap();
    models::claim_rec(&mut c2, "tok4", "user-ccc").await.unwrap();

    let mut rows = conn
        .query("SELECT COUNT(*) FROM friendships", turso::params![])
        .await
        .unwrap();
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        2,
        "each claimer gets own friendship with sender"
    );
}

#[tokio::test]
async fn revoke_removes_receipts_keeps_friendship_and_watchlist() {
    let (db, conn) = setup().await;
    let mid = movie_id(&conn).await;
    models::create_share_rec(&conn, "user-bbb", mid, None, "tok5")
        .await
        .unwrap();
    let mut c = db.connect().await.unwrap();
    models::claim_rec(&mut c, "tok5", "user-aaa").await.unwrap();

    // Recipient added to watchlist via the rec.
    // Scoped block: an un-drained `Rows` cursor holds a read lock on `conn`
    // until dropped, which under non-WAL `:memory:` test DBs blocks the
    // writer transactions below (busy_timeout then exhausts for real,
    // since the lock is never released, not merely contended).
    let rec_id = {
        let mut rows = conn
            .query(
                "SELECT id FROM recommendations WHERE token='tok5'",
                turso::params![],
            )
            .await
            .unwrap();
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
    };
    conn.execute(
        "INSERT INTO watchlist (user_id, movie_id, state, via_rec_id) VALUES ('user-aaa', ?1, 'want_to_watch', ?2)",
        turso::params![mid, rec_id],
    )
    .await
    .unwrap();

    // Non-sender cannot revoke
    let mut c2 = db.connect().await.unwrap();
    assert!(!models::revoke_rec(&mut c2, "tok5", "user-aaa").await.unwrap());

    // Sender revokes
    let mut c3 = db.connect().await.unwrap();
    assert!(models::revoke_rec(&mut c3, "tok5", "user-bbb").await.unwrap());

    // Receipts gone, friendship persists, watchlist row persists untagged
    assert!(models::get_received_recs(&conn, "user-aaa").await.unwrap().is_empty());
    assert!(models::are_friends(&conn, "user-aaa", "user-bbb").await.unwrap());
    let mut rows = conn
        .query(
            "SELECT via_rec_id FROM watchlist WHERE user_id='user-aaa'",
            turso::params![],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().expect("watchlist row must survive revoke");
    assert!(
        matches!(row.get_value(0).unwrap(), turso::Value::Null),
        "via_rec_id must be NULL after revoke"
    );
    // Token now dead
    assert!(models::get_rec_public(&conn, "tok5").await.unwrap().is_none());
}

#[tokio::test]
async fn direct_rec_requires_no_link_and_marks_read_works() {
    let (db, conn) = setup().await;
    let mid = movie_id(&conn).await;
    // alice & bob become friends first (via a claimed rec)
    models::create_share_rec(&conn, "user-bbb", mid, None, "tok6").await.unwrap();
    let mut c = db.connect().await.unwrap();
    models::claim_rec(&mut c, "tok6", "user-aaa").await.unwrap();

    // bob sends direct rec to alice
    let mut c2 = db.connect().await.unwrap();
    models::create_direct_rec(&mut c2, "user-bbb", "user-aaa", mid, None, "tok7")
        .await
        .unwrap();
    assert_eq!(models::unread_rec_count(&conn, "user-aaa").await.unwrap(), 2);

    models::mark_rec_read(&conn, "tok7", "user-aaa").await.unwrap();
    assert_eq!(models::unread_rec_count(&conn, "user-aaa").await.unwrap(), 1);

    // sent list shows both with claim counts
    let sent = models::get_sent_recs(&conn, "user-bbb").await.unwrap();
    assert_eq!(sent.len(), 2);

    // friends list from both sides
    let f_alice = models::get_friends(&conn, "user-aaa").await.unwrap();
    assert_eq!(f_alice.len(), 1);
    assert_eq!(f_alice[0].username, "bob");
    let f_bob = models::get_friends(&conn, "user-bbb").await.unwrap();
    assert_eq!(f_bob[0].username, "alice");
}

#[tokio::test]
async fn username_lookups() {
    let (_db, conn) = setup().await;
    assert_eq!(
        models::get_username(&conn, "user-aaa").await.unwrap().as_deref(),
        Some("alice")
    );
    assert_eq!(models::get_username(&conn, "user-ccc").await.unwrap(), None);
    assert_eq!(
        models::get_user_id_by_username(&conn, "bob").await.unwrap().as_deref(),
        Some("user-bbb")
    );
    assert_eq!(
        models::get_user_id_by_username(&conn, "nobody").await.unwrap(),
        None
    );
}
