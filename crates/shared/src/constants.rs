/// The sweet spot range for IMDb ratings that indicate a potential hidden gem.
/// Movies rated too low are genuinely bad; movies rated too high are already famous.
pub const IMDB_GEM_MIN: f64 = 6.5;
pub const IMDB_GEM_MAX: f64 = 7.9;

/// Minimum number of IMDb votes for a movie to be considered (avoids noise).
pub const MIN_IMDB_VOTES: i64 = 500;

/// Minimum age (in years) for a movie to qualify as a hidden gem.
///
/// Set to 1 so that films from the previous calendar year and earlier are eligible.
/// Films released in the current year are excluded — vote counts have not yet settled
/// and the film hasn't had time to be overlooked. Films from last year onwards are
/// fair game; year_decay naturally gives them a low score relative to older films.
pub const MIN_GEM_AGE_YEARS: i32 = 1;

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

/// Minimum IMDb rating for RT-endorsed films (RT ≥ RT_ENDORSEMENT_THRESHOLD).
///
/// Films with strong critic backing can score even if their crowd rating falls below
/// the standard IMDB_GEM_MIN floor. This captures films like The Tunnel (2011):
/// IMDb 5.8 but RT 100% — critics endorsed it, audiences missed it by design
/// (unconventional distribution, niche genre). Without critic endorsement a low
/// crowd rating signals genuine poor quality, so the standard floor still applies.
pub const IMDB_GEM_MIN_RT_ENDORSED: f64 = 5.5;

/// Minimum RT critic score required to use the lower IMDB_GEM_MIN_RT_ENDORSED floor.
/// 75% = critics broadly endorsed the film (not just a handful of reviews).
pub const RT_ENDORSEMENT_THRESHOLD: i32 = 75;

/// RT critic score thresholds for the credibility multiplier applied to vote_ratio.
///
/// True hidden gems = critics endorsed it + audiences missed it.
/// Films where critics panned it + audiences stayed away are "informed avoidance" —
/// low vote counts there reflect critical rejection, not genuine undiscovery.
/// The RT credibility multiplier on vote_ratio distinguishes the two cases:
///
/// - rt >= RT_CREDIBILITY_HIGH (70%): multiplier = 1.0 — critics endorsed it fully
/// - rt in [RT_CREDIBILITY_FLOOR, RT_CREDIBILITY_HIGH): linear 0.0 → 1.0
/// - rt < RT_CREDIBILITY_FLOOR (40%): multiplier = 0.0 — informed avoidance, not undiscovery
/// - rt = None: multiplier = 0.8 — benefit of doubt (old films often lack RT data)
///
/// Films that score but have rt < WILDCARD_RT_THRESHOLD are classified as "wildcards":
/// listed separately as divisive films critics disagreed on, not hidden gems.
pub const RT_CREDIBILITY_HIGH: i32 = 70;
pub const RT_CREDIBILITY_FLOOR: i32 = 40;
pub const WILDCARD_RT_THRESHOLD: i32 = 50;

/// Weight coefficients for the gem score algorithm.
///
/// Design rationale:
/// - YEAR_DECAY (0.40) is the dominant weight. "Hidden gem" means "forgotten over time."
///   A 2023 film scores 0.10 on year_decay; a 1977 film scores 1.0 — a 0.30 gap from
///   this component alone. This prevents recent acclaimed films (good RT, decent IMDb)
///   from flooding the top rankings.
/// - IMDB_RATING (0.30): primary quality signal — sweet spot 6.5–7.9 required.
/// - VOTE_RATIO (0.25): "undiscovered right now" signal, modulated by the RT credibility
///   multiplier. High RT + low votes = critics endorsed, audiences missed (true gem).
///   Low RT + low votes = audiences stayed away because critics warned them off (wildcard).
///   The multiplier is applied to vote_ratio before weighting, not as an additive component.
/// - OBSCURED (0.05): data sparsity (~100 blockbusters) limits coverage; preserves
///   the Sorcerer/Star Wars signal without overfitting to the thin data.
///
/// Total: 0.30 + 0.25 + 0.40 + 0.05 = 1.00
pub mod weights {
    /// Weight for the IMDb rating being in the sweet spot.
    pub const IMDB_RATING: f64 = 0.30;
    /// Weight for high rating relative to low vote count (modulated by RT credibility multiplier).
    pub const VOTE_RATIO: f64 = 0.25;
    /// Weight for year decay (older = more forgotten). Dominant component.
    pub const YEAR_DECAY: f64 = 0.40;
    /// Weight for being obscured by a big hit.
    pub const OBSCURED: f64 = 0.05;
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

/// TMDB API rate limiting.
///
/// TMDB allows 40 requests per 10 seconds on the v3 API. With `sync_movies` making
/// up to 41 calls per page (1 discover + 20 × 2 movie detail/credits), sequential
/// processing already throttles the per-movie calls. The page-level delay protects
/// against the "all movies cached" re-run case where the inner loop is a no-op and
/// discover pages fire as fast as the network allows.
///
/// Wave design: every WAVE_SIZE pages, pause WAVE_DELAY_MS. This gives a natural
/// heartbeat — short pauses prevent 429s during normal operation; the wave pause
/// lets the sliding-window bucket fully recover between waves.
pub mod tmdb_rate_limit {
    /// Sleep between consecutive discover page requests (ms).
    pub const PAGE_DELAY_MS: u64 = 300;
    /// Pages per wave before a longer pause.
    pub const WAVE_SIZE: i32 = 20;
    /// Extra sleep at the end of each wave (ms). Lets the TMDB rate-limit bucket recover.
    pub const WAVE_DELAY_MS: u64 = 2_000;
    /// Absolute ceiling: TMDB's own limit is 500 pages per query.
    pub const MAX_PAGES: i32 = 500;
}

/// Seeded hidden gems — movies known to be incredible but underappreciated.
/// These are used to seed the database and validate the algorithm.
pub const SEEDED_GEMS: &[(&str, i32, &str)] = &[
    ("Sorcerer", 1977, "tt0076740"),
    ("Dinner in America", 2020, "tt9058654"),
    ("The Hurt Locker", 2009, "tt0887912"),
    ("The Messenger", 2009, "tt1340803"),
];
