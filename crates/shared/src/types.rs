use serde::{Deserialize, Serialize};

/// A movie with all its metadata and calculated gem score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Movie {
    pub id: Option<i64>,
    pub tmdb_id: i64,
    pub imdb_id: Option<String>,
    pub title: String,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub director: Option<String>,
    pub overview: Option<String>,
    pub poster_url: Option<String>,
    pub tmdb_rating: Option<f64>,
    pub tmdb_vote_count: Option<i64>,
    pub imdb_rating: Option<f64>,
    pub imdb_vote_count: Option<i64>,
    pub rt_critic_score: Option<i32>,
    pub rt_audience_score: Option<i32>,
    pub gem_score: Option<f64>,
    pub gem_rank: Option<i64>,
    pub release_date: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// The result of the hidden gem scoring algorithm for a movie.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GemScore {
    pub movie_id: i64,
    pub raw_score: f64,
    pub normalized_score: f64,
    pub components: GemScoreComponents,
}

/// Breakdown of the individual components that contribute to a gem score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GemScoreComponents {
    /// Score from IMDb rating being in the hidden gem sweet spot (6.5–7.9)
    pub imdb_rating_score: f64,
    /// Raw score from high rating relative to low vote count (before RT multiplier)
    pub vote_ratio_score: f64,
    /// Score from year recency/forgotten status
    pub year_decay_score: f64,
    /// Score from being released near a major blockbuster (obscured factor)
    pub obscured_by_big_hit_score: f64,
    /// RT credibility multiplier applied to vote_ratio (0.0–1.0).
    /// High RT + low votes = critics endorsed, audiences missed (true hidden gem signal).
    /// Low RT + low votes = informed avoidance, not undiscovery.
    pub rt_credibility_multiplier: f64,
    /// Genre-based multiplier
    pub genre_boost: f64,
}

/// A "big hit" movie that may have obscured other releases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BigHit {
    pub id: Option<i64>,
    pub movie_id: i64,
    pub year: i32,
    pub box_office_millions: Option<f64>,
    pub popularity_score: Option<f64>,
}

/// A user's watchlist entry (for future use).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchlistEntry {
    pub id: Option<i64>,
    pub movie_id: i64,
    pub status: WatchStatus,
    pub added_at: Option<String>,
}

/// Status of a movie in a user's watchlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatchStatus {
    ToWatch,
    Watching,
    Watched,
}

/// API response wrapper for paginated results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub page: i32,
    pub per_page: i32,
    pub total: i64,
}

/// A simplified movie summary for list views (gems and acclaimed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovieSummary {
    pub id: i64,
    pub title: String,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub director: Option<String>,
    pub poster_url: Option<String>,
    pub imdb_rating: Option<f64>,
    pub rt_critic_score: Option<i32>,
    pub gem_score: Option<f64>,
    pub gem_rank: Option<i64>,
}

/// TMDB API response for movie search (used during ingestion).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbMovie {
    pub id: i64,
    pub title: String,
    pub release_date: Option<String>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<i64>,
    pub popularity: Option<f64>,
    pub genre_ids: Option<Vec<i32>>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
}

/// Paginated response from TMDB discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbDiscoverResponse {
    pub page: i32,
    pub results: Vec<TmdbMovie>,
    pub total_pages: i32,
    pub total_results: i32,
}

/// Response from TMDB's `/find/{external_id}` endpoint (lookup by IMDb ID, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbFindResponse {
    pub movie_results: Vec<TmdbMovie>,
}

/// Detailed movie info from TMDB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbMovieDetail {
    pub id: i64,
    pub imdb_id: Option<String>,
    pub title: String,
    pub release_date: Option<String>,
    pub vote_average: Option<f64>,
    pub vote_count: Option<i64>,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub genres: Vec<TmdbGenre>,
    pub runtime: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbGenre {
    pub id: i32,
    pub name: String,
}

/// Credits for a movie from TMDB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbCredits {
    pub id: i64,
    pub cast: Vec<TmdbCastMember>,
    pub crew: Vec<TmdbCrewMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbCastMember {
    pub id: i64,
    pub name: String,
    pub character: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbCrewMember {
    pub id: i64,
    pub name: String,
    pub job: String,
    pub department: String,
}

/// TMDB configuration for images.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbConfig {
    pub images: TmdbImageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbImageConfig {
    pub base_url: String,
    pub secure_base_url: String,
    pub poster_sizes: Vec<String>,
}

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub database: String,
}

// ──────────────────────────────────────────────
// Run Logging Types
// ──────────────────────────────────────────────

/// Severity level for a run-log event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// A single run-log entry recorded every time the app runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLogEntry {
    pub id: Option<i64>,
    pub level: String,
    pub event_type: String,
    pub message: String,
    pub created_at: Option<String>,
}

// ──────────────────────────────────────────────
// OMDb API Types
// ──────────────────────────────────────────────

/// Response from the OMDb API by IMDb ID.
///
/// OMDb returns PascalCase JSON fields. serde(rename_all = "PascalCase") handles most
/// fields, but a few (imdbRating, imdbVotes, imdbID) start with lowercase "imdb" so they
/// need individual overrides.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct OmdbResponse {
    pub title: Option<String>,
    pub year: Option<String>,
    pub rated: Option<String>,
    pub released: Option<String>,
    pub runtime: Option<String>,
    pub genre: Option<String>,
    pub director: Option<String>,
    pub writer: Option<String>,
    pub actors: Option<String>,
    pub plot: Option<String>,
    pub language: Option<String>,
    pub country: Option<String>,
    pub awards: Option<String>,
    pub poster: Option<String>,
    pub ratings: Option<Vec<OmdbRating>>,
    pub metascore: Option<String>,
    #[serde(rename = "imdbRating")]
    pub imdb_rating: Option<String>,
    #[serde(rename = "imdbVotes")]
    pub imdb_votes: Option<String>,
    #[serde(rename = "imdbID")]
    pub imdb_id: Option<String>,
    #[serde(rename = "Type")]
    pub type_: Option<String>,
    pub dvd: Option<String>,
    pub box_office: Option<String>,
    pub production: Option<String>,
    pub website: Option<String>,
    pub response: Option<String>,
    pub error: Option<String>,
}

/// A single rating entry in the OMDb response (e.g. Internet Movie Database, Rotten Tomatoes, Metacritic).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct OmdbRating {
    pub source: Option<String>,
    pub value: Option<String>,
}
