use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
mod routes;
mod services;
use gem_finder_db::{migrations, models, Database};
use gem_finder_shared::types::{HealthResponse, Movie, MovieSummary, PaginatedResponse};
use serde::Deserialize;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Application state shared across all handlers.
#[derive(Clone)]
struct AppState {
    db: Arc<Database>,
}

#[derive(Deserialize)]
struct GemsQuery {
    page: Option<i32>,
    per_page: Option<i32>,
    min_year: Option<i32>,
    genre: Option<String>,
}

#[derive(Deserialize)]
struct AclaimedQuery {
    page: Option<i32>,
    per_page: Option<i32>,
}

#[tokio::main]
async fn main() {
    // Initialize tracing: write to BOTH stdout and gem_finder.log.
    // Two separate fmt layers share the same filter via registry().
    let log_file = tracing_appender::rolling::never(".", "gem_finder.log");
    let (file_writer, _guard) = tracing_appender::non_blocking(log_file);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // File layer: no ANSI escape codes so the log file is clean plain text.
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false);

    // Stdout layer: with ANSI colours for human reading in the terminal.
    let stdout_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stdout);

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    // Check for CLI subcommands
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "seed-test-data" => {
                run_seed_test_data().await;
                return;
            }
            _ => {
                eprintln!("Unknown subcommand: {}. Valid: seed-test-data", args[1]);
                std::process::exit(1);
            }
        }
    }

    // Connect to Turso (or local SQLite fallback)
    let db = Database::from_env()
        .await
        .expect("Failed to connect to database");

    tracing::info!("Connected to database");

    // Run migrations
    let conn = db
        .connect()
        .await
        .expect("Failed to get database connection");
    migrations::run(&conn)
        .await
        .expect("Failed to run migrations");

    tracing::info!("Database migrations complete");

    // Log startup to run_logs (before moving db into Arc)
    let startup_conn = db
        .connect()
        .await
        .expect("Failed to get database connection for startup log");
    if let Err(e) = models::insert_run_log(
        &startup_conn,
        "info",
        "startup",
        &format!("Gem Finder API v{} starting", env!("CARGO_PKG_VERSION")),
    )
    .await
    {
        tracing::warn!("Failed to write startup log: {}", e);
    }

    let state = AppState { db: Arc::new(db) };

    let router = Router::new()
        .route("/health", get(health_check))
        .route("/api/gems", get(get_gems))
        .route("/api/acclaimed", get(get_acclaimed))
        .route("/api/movies/{id}", get(get_movie))
        .route("/api/score", post(run_scoring))
        .route("/api/admin/sync", post(routes::admin::trigger_sync))
        .route("/api/admin/enrich", post(routes::admin::trigger_enrich))
        .route("/api/admin/logs", get(routes::admin::get_run_logs))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    // In production (SERVE_FRONTEND=1), serve the compiled WASM frontend from dist/.
    // trunk build writes index.html + wasm assets there. The fallback serves index.html
    // for all unmatched paths so the Leptos client-side router handles navigation.
    // In API-only or development mode (trunk serve handles the frontend), leave this off.
    let app = if std::env::var("SERVE_FRONTEND").is_ok() {
        use tower_http::services::{ServeDir, ServeFile};
        let serve_dir = ServeDir::new("dist").not_found_service(ServeFile::new("dist/index.html"));
        tracing::info!("Frontend serving enabled from dist/");
        router.fallback_service(serve_dir)
    } else {
        router
    };

    let addr = "0.0.0.0:3000";
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind address");

    axum::serve(listener, app).await.expect("Server failed");
}

/// CLI subcommand: seed the database with test data for algorithm validation.
///
/// Steps:
/// 1. Init TMDB service + config
/// 2. Seed the 3 known gems (The Sorcerer, Dinner in America, The Hurt Locker)
/// 3. Sync blockbusters (for obscured-by-big-hit signal)
/// 4. Run OMDb enrichment (for real IMDb ratings + RT scores)
/// 5. Run the batch scoring pipeline
/// 6. Log results and print a summary to stdout
async fn run_seed_test_data() {
    tracing::info!("=== seed-test-data: starting ===");

    let db = match Database::from_env().await {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to connect to database: {}", e);
            std::process::exit(1);
        }
    };

    let conn = match db.connect().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to get connection: {}", e);
            std::process::exit(1);
        }
    };

    // Step 0: migrations
    if let Err(e) = migrations::run(&conn).await {
        eprintln!("Migration failed: {}", e);
        std::process::exit(1);
    }
    tracing::info!("Migrations complete");

    // Log start
    let _ = models::insert_run_log(&conn, "info", "seed_started", "seed-test-data starting").await;

    // Step 1: Init TMDB
    let mut tmdb = match services::tmdb_sync::TmdbSyncService::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("TMDB init failed: {}. Set TMDB_API_KEY env var.", e);
            std::process::exit(1);
        }
    };
    if let Err(e) = tmdb.init_config().await {
        eprintln!("TMDB config init failed: {}", e);
        std::process::exit(1);
    }
    tracing::info!("TMDB service ready");

    // Step 2: Seed known gems
    tracing::info!("Seeding known gems...");
    match tmdb.seed_known_gems(&conn).await {
        Ok((seeded, total, results)) => {
            println!("\n--- Known Gems Seeding Results ---");
            for (title, result) in &results {
                let icon = match result.as_str() {
                    "seeded" => "✅",
                    "not_found" => "❌",
                    _ => "⚠️",
                };
                println!("  {} {}: {}", icon, title, result);
            }
            println!("  Total: {}/{} seeded\n", seeded, total);
            let _ = models::insert_run_log(
                &conn,
                "info",
                "seed_gems",
                &format!("Seeded {}/{} known gems", seeded, total),
            )
            .await;
        }
        Err(e) => {
            eprintln!("Seed known gems failed: {}", e);
        }
    }

    // Step 3: Sync blockbusters
    tracing::info!("Syncing blockbusters...");
    match tmdb.sync_blockbusters(&conn).await {
        Ok(count) => {
            println!("--- Blockbusters ---");
            println!("  {} blockbusters synced to big_hits table\n", count);
            let _ = models::insert_run_log(
                &conn,
                "info",
                "seed_blockbusters",
                &format!("Synced {} blockbusters", count),
            )
            .await;
        }
        Err(e) => {
            eprintln!("Blockbuster sync failed: {}", e);
        }
    }

    // Step 3b: Discover sync — gem candidates across 4 era windows.
    // Each window uses 5 pages (100 films, vote_count.desc) so we get the most-notable
    // films distributed across ALL years in the era, not just the newest 100.
    // This gives ~400 candidates spread across 1960–present.
    //
    // Windows: classics (1960-1984), modern classics (1984-1999),
    //          2000s (1999-2012), recent (2012-present)
    tracing::info!("Syncing gem candidates across era windows...");
    let era_windows: &[(i32, Option<i32>, &str)] = &[
        (1960, Some(1984), "classics 1960–1984"),
        (1984, Some(1999), "modern classics 1984–1999"),
        (1999, Some(2012), "2000s 1999–2012"),
        (2012, None, "recent 2012–present"),
    ];
    let mut windows_ok = 0usize;
    for (start, end, label) in era_windows {
        tracing::info!("  Era window: {}", label);
        match tmdb.sync_movies(&conn, *start, *end).await {
            Ok(()) => {
                println!("  ✓ {}", label);
                windows_ok += 1;
            }
            Err(e) => eprintln!("  ✗ {}: {}", label, e),
        }
    }
    println!("--- Gem Candidate Sync ---");
    println!(
        "  {}/{} era windows completed (see logs for per-window detail)\n",
        windows_ok,
        era_windows.len()
    );

    // Step 3c: Sync acclaimed candidates before OMDb enrichment so they get enriched
    // in the same run. Films with vote_avg ≥ 7.5 and vote_count ≥ 10,000 are the
    // acclaimed tier; classify_acclaimed_films (step 5b) then filters by IMDb + RT.
    tracing::info!("Syncing acclaimed candidates...");
    match tmdb.sync_acclaimed_candidates(&conn).await {
        Ok(count) => {
            println!("--- Acclaimed Candidate Sync ---");
            println!("  {} new acclaimed candidates synced\n", count);
        }
        Err(e) => {
            eprintln!("Acclaimed candidate sync failed: {}", e);
        }
    }

    // Step 4: OMDb enrichment — drain the entire unenriched queue.
    // Limit must be high enough that no unenriched movie reaches the scoring pipeline,
    // since missing RT data means the RT quality gate can't fire and bad films slip through.
    if let Ok(omdb) = services::omdb_sync::OmdbEnrichmentService::new() {
        tracing::info!("Running OMDb enrichment...");
        match omdb.enrich_movies(&conn, 2000).await {
            Ok((enriched, total, errors)) => {
                println!("--- OMDb Enrichment ---");
                println!("  {}/{} movies enriched", enriched, total);
                if !errors.is_empty() {
                    println!("  Errors ({}):", errors.len());
                    for e in errors.iter().take(5) {
                        println!("    - {}", e);
                    }
                }
                println!();
                let _ = models::insert_run_log(
                    &conn,
                    "info",
                    "seed_enrichment",
                    &format!("Enriched {}/{} movies via OMDb", enriched, total),
                )
                .await;
            }
            Err(e) => {
                eprintln!("OMDb enrichment failed: {}", e);
            }
        }
    } else {
        println!("--- OMDb Enrichment ---");
        println!("  Skipped: set OMDB_API_KEY env var to enable\n");
    }

    // Step 5: Run scoring
    tracing::info!("Running batch scoring...");
    match services::gem_score::run_batch_scoring(&conn).await {
        Ok(scored) => {
            println!("--- Scoring ---");
            println!("  {} movies scored\n", scored);
            let _ = models::insert_run_log(
                &conn,
                "info",
                "seed_scoring",
                &format!("Scored {} movies", scored),
            )
            .await;
        }
        Err(e) => {
            eprintln!("Scoring failed: {}", e);
        }
    }

    // Step 5b: Classify acclaimed films (populate acclaimed table from enriched movies)
    tracing::info!("Classifying acclaimed films...");
    match models::classify_acclaimed_films(&conn).await {
        Ok(count) => {
            println!("--- Acclaimed Classification ---");
            println!("  {} films in acclaimed table\n", count);
            let _ = models::insert_run_log(
                &conn,
                "info",
                "seed_acclaimed",
                &format!("Classified {} acclaimed films", count),
            )
            .await;
        }
        Err(e) => {
            eprintln!("Acclaimed classification failed: {}", e);
        }
    }

    // Step 6: Print summary
    println!("=== seed-test-data complete ===");
    println!();
    println!("To verify rankings, start the server and query:");
    println!("  curl http://localhost:3000/api/gems");
    println!();
    println!("To view run logs:");
    println!("  curl http://localhost:3000/api/admin/logs");

    let _ = models::insert_run_log(&conn, "info", "seed_complete", "seed-test-data finished").await;
}

/// Health check endpoint.
async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let db_status = match state.db.connect().await {
        Ok(conn) => match conn.execute("SELECT 1", turso::params![]).await {
            Ok(_) => "connected".to_string(),
            Err(e) => format!("error: {}", e),
        },
        Err(e) => format!("error: {}", e),
    };

    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: db_status,
    })
}

/// Get a paginated list of hidden gems.
async fn get_gems(
    State(state): State<AppState>,
    Query(query): Query<GemsQuery>,
) -> Result<Json<PaginatedResponse<MovieSummary>>, StatusCode> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let movies = models::get_top_gems(
        &conn,
        page,
        per_page,
        query.min_year,
        query.genre.as_deref(),
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total = models::get_gems_count(&conn, query.min_year, query.genre.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PaginatedResponse {
        data: movies,
        page,
        per_page,
        total,
    }))
}

/// Trigger the batch gem scoring pipeline.
/// POST /api/score
async fn run_scoring(State(state): State<AppState>) -> Result<Json<serde_json::Value>, StatusCode> {
    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let scored_count = services::gem_score::run_batch_scoring(&conn)
        .await
        .map_err(|e| {
            tracing::error!("Batch scoring failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(serde_json::json!({
        "status": "success",
        "movies_scored": scored_count,
    })))
}

/// Get a paginated list of acclaimed films (IMDb ≥ 8.0, RT ≥ 80%).
/// GET /api/acclaimed
async fn get_acclaimed(
    State(state): State<AppState>,
    Query(query): Query<AclaimedQuery>,
) -> Result<Json<PaginatedResponse<MovieSummary>>, StatusCode> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);

    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let movies = models::get_acclaimed_films(&conn, page, per_page)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let total = models::get_acclaimed_count(&conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(PaginatedResponse {
        data: movies,
        page,
        per_page,
        total,
    }))
}

/// Get a single movie by ID.
async fn get_movie(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Movie>, StatusCode> {
    let conn = state
        .db
        .connect()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let movie = models::get_movie_by_id(&conn, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(movie))
}
