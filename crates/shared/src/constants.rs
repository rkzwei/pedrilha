/// The sweet spot range for IMDb ratings that indicate a potential hidden gem.
/// Movies rated too low are genuinely bad; movies rated too high are already famous.
pub const IMDB_GEM_MIN: f64 = 6.5;
pub const IMDB_GEM_MAX: f64 = 7.9;

/// Minimum number of IMDb votes for a movie to be considered (avoids noise).
pub const MIN_IMDB_VOTES: i64 = 500;

/// Minimum age (in years) for a movie to qualify as a hidden gem.
///
/// Films released within this window have artificially low vote counts because
/// streaming-platform audiences rarely rate on TMDB/IMDb. A 2024 film with
/// 800 votes is not "forgotten" — it just hasn't had time to accumulate them.
/// Set to 3 so that only films from (current_year - 3) and earlier are eligible.
/// Note: recent foreign/indie films with legitimately low votes (e.g. Oscar-nominated
/// foreign-language films) are correctly included — age alone is not a quality filter.
pub const MIN_GEM_AGE_YEARS: i32 = 3;

/// The ideal IMDb vote count for a "pure" hidden gem (low votes = undiscovered).
pub const IDEAL_VOTE_COUNT: f64 = 10_000.0;

/// Maximum box office (in millions) for a movie that could be a hidden gem.
pub const BIG_HIT_BOX_OFFICE_THRESHOLD: f64 = 200.0;

/// Number of weeks around a big hit's release to consider other movies "obscured."
/// Set to 6 (42 days) to capture the Sorcerer/Star Wars case: Star Wars opened
/// May 25 1977, Sorcerer opened June 24 1977 — 30 days apart, just outside 4 weeks.
pub const OBSCURED_WINDOW_WEEKS: i64 = 6;

/// Starting year for data ingestion.
pub const DATA_START_YEAR: i32 = 1960;

/// Minimum IMDb rating for the "acclaimed classics" category.
/// Films at 8.0+ are universally considered excellent — not hidden gems, but
/// films everyone should see that they may not have gotten around to yet.
pub const ACCLAIMED_MIN_IMDB: f64 = 8.0;

/// Minimum Rotten Tomatoes critic score for the "acclaimed classics" category.
/// 80% = Certified Fresh territory; combined with IMDb 8.0+ this catches
/// films like The Godfather, Schindler's List, Parasite, etc.
pub const ACCLAIMED_MIN_RT: i32 = 80;

/// Weight coefficients for the gem score algorithm.
///
/// Design rationale:
/// - YEAR_DECAY (0.40) is the dominant weight. "Hidden gem" means "forgotten over time."
///   A 2023 film scores 0.10 on year_decay; a 1977 film scores 1.0 — a 0.30 gap from
///   this component alone. This prevents recent acclaimed films (good RT, decent IMDb)
///   from flooding the top rankings.
/// - IMDB_RATING (0.30): primary quality signal — sweet spot 6.5–7.9 required.
/// - VOTE_RATIO (0.20): "undiscovered right now" signal, independent of age. A 1985 film
///   with 500 votes is genuinely more hidden than one with 200k votes. Keeps obscure
///   films ahead of cult classics that got streaming-era rediscovery boosts.
/// - OBSCURED (0.05): data sparsity (~100 blockbusters) limits coverage; preserves
///   the Sorcerer/Star Wars signal without overfitting to the thin data.
/// - CRITIC_DISPARITY (0.05): RT is primarily a hard gate (< 65 → excluded). Residual
///   5% means a 2023 film with RT=97% gains only 0.047 here — it cannot compensate
///   for the 0.36 gap it loses to a 1977 film on year_decay.
pub mod weights {
    /// Weight for the IMDb rating being in the sweet spot.
    pub const IMDB_RATING: f64 = 0.30;
    /// Weight for high rating relative to low vote count.
    pub const VOTE_RATIO: f64 = 0.20;
    /// Weight for year decay (older = more forgotten). Dominant component.
    pub const YEAR_DECAY: f64 = 0.40;
    /// Weight for being obscured by a big hit.
    pub const OBSCURED: f64 = 0.05;
    /// Weight for critic score signal (RT is primarily a hard gate, not a scorer).
    pub const CRITIC_DISPARITY: f64 = 0.05;
}

/// Genre boost multipliers for hidden gem probability.
/// Stored as a static slice — no allocation per lookup.
pub mod genre_boosts {
    /// Returns a slice of (genre_name, boost_multiplier) pairs.
    /// Use `contains` on the movie's genre string to match (genres are comma-separated).
    pub const ALL: &[(&str, f64)] = &[
        ("Foreign", 1.3),
        ("Indie", 1.25),
        ("Documentary", 1.2),
        ("Drama", 1.1),
        ("Thriller", 1.05),
        ("Comedy", 0.95),
        ("Horror", 0.9),
        ("Romance", 0.85),
        ("Action", 0.8),
        ("Science Fiction", 0.75),
    ];
}

/// Seeded hidden gems — movies known to be incredible but underappreciated.
/// These are used to seed the database and validate the algorithm.
pub const SEEDED_GEMS: &[(&str, i32, &str)] = &[
    ("Sorcerer", 1977, "tt0076740"),
    ("Dinner in America", 2020, "tt9058654"),
    ("The Hurt Locker", 2009, "tt0887912"),
    // Add more as discovered
];
