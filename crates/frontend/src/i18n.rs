//! Hand-rolled PT/EN internationalization.
//!
//! Design: a `RwSignal<Lang>` lives in Leptos context (provided by `App`). Every
//! user-visible string is a field on the `Dict` struct, with one fully-populated
//! `const` instance per language (`EN`, `PT`). Components read the current dict via
//! `move || dict(lang.get()).some_field`, so a locale toggle updates every string
//! reactively. Modelling strings as struct fields (not stringly-typed keys) means a
//! missing or renamed translation is a compile error.
//!
//! Interpolated strings store a literal `{}` placeholder and are filled at the call
//! site with `.replace("{}", …)`.
//!
//! Locale is persisted to `localStorage` (`gf_lang`) and defaults, on first visit,
//! to the browser's `navigator.languages` (any `pt*` → PT, otherwise EN).

use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Pt,
    En,
}

impl Lang {
    /// BCP-47 tag for the `<html lang>` attribute.
    pub fn html_tag(self) -> &'static str {
        match self {
            Lang::Pt => "pt-BR",
            Lang::En => "en",
        }
    }
    /// Short code stored in localStorage.
    fn code(self) -> &'static str {
        match self {
            Lang::Pt => "pt",
            Lang::En => "en",
        }
    }
}

const LS_LANG: &str = "gf_lang";

/// The full set of translatable UI strings. See module docs for the pattern.
pub struct Dict {
    // ── Header / nav ──────────────────────────────────────────────────────────
    pub nav_gems: &'static str,
    pub nav_acclaimed: &'static str,
    pub nav_wildcards: &'static str,
    pub nav_admin: &'static str,
    pub nav_watchlist: &'static str,
    pub nav_signout: &'static str,
    pub nav_signin: &'static str,

    // ── Footer ────────────────────────────────────────────────────────────────
    pub footer_tagline: &'static str,
    pub footer_how_it_works: &'static str,
    pub footer_privacy: &'static str,
    pub footer_no_cookies_ads: &'static str,

    // ── Meta ──────────────────────────────────────────────────────────────────
    pub meta_description: &'static str,

    // ── 404 ───────────────────────────────────────────────────────────────────
    pub nf_meta_title: &'static str,
    pub nf_title: &'static str,
    pub nf_body: &'static str,
    pub nf_back: &'static str,

    // ── Document <title>s ─────────────────────────────────────────────────────
    pub title_home: &'static str,
    pub title_acclaimed: &'static str,
    pub title_wildcards: &'static str,
    pub title_signin: &'static str,
    pub title_signing_in: &'static str,
    pub title_admin: &'static str,
    pub title_watchlist: &'static str,
    pub title_privacy: &'static str,
    pub title_about: &'static str,
    pub title_changelog: &'static str,

    // ── Home / Acclaimed / Wildcards headers ──────────────────────────────────
    pub home_h1: &'static str,
    pub home_desc: &'static str,
    pub acclaimed_h1: &'static str,
    pub acclaimed_desc: &'static str,
    pub wildcards_h1: &'static str,
    pub wildcards_desc: &'static str,
    pub wildcards_desc2: &'static str,

    // ── Filter bar ────────────────────────────────────────────────────────────
    pub filter_search_placeholder: &'static str,
    pub filter_clear_search: &'static str,
    pub filter_genre: &'static str,
    pub filter_genre_count: &'static str, // "Genre ({})"
    pub filter_era: &'static str,
    pub filter_from_era: &'static str,
    pub filter_sort_by: &'static str,
    pub filter_direction: &'static str,
    pub filter_desc: &'static str,
    pub filter_asc: &'static str,
    pub filter_films_count: &'static str, // "{} films"
    pub filter_no_matches: &'static str,
    pub filter_clear: &'static str,

    // ── What can I watch? (Phase 10) ──────────────────────────────────────────
    pub filter_watchable: &'static str,
    pub filter_region: &'static str,
    pub filter_include_rentals: &'static str,
    pub filter_rentals_hint: &'static str,
    pub filter_active_chip: &'static str, // "{} services"
    pub filter_empty_hint: &'static str,
    pub badge_included: &'static str,
    pub badge_rent: &'static str,
    pub providers_attribution: &'static str,

    // ── Pagination ────────────────────────────────────────────────────────────
    pub page_label: &'static str,
    pub page_prev: &'static str,
    pub page_next: &'static str,

    // ── Movie grid states ─────────────────────────────────────────────────────
    pub grid_err_load: &'static str,
    pub grid_try_again: &'static str,
    pub grid_no_gems: &'static str,
    pub grid_no_watch_matches: &'static str,
    pub grid_clear_filters: &'static str,

    // ── Sign-in ───────────────────────────────────────────────────────────────
    pub signin_check_email: &'static str,
    pub signin_sent_prefix: &'static str,
    pub signin_expires: &'static str,
    pub signin_use_different: &'static str,
    pub signin_tagline: &'static str,
    pub signin_heading: &'static str,
    pub signin_blurb: &'static str,
    pub signin_email_label: &'static str,
    pub signin_sending: &'static str,
    pub signin_send_link: &'static str,
    pub signin_back: &'static str,
    pub signin_err_empty: &'static str,
    pub signin_err_invalid: &'static str,
    pub signin_err_generic: &'static str,

    // ── Movie detail ──────────────────────────────────────────────────────────
    pub detail_back: &'static str,
    pub detail_err_load: &'static str,
    pub detail_back_gems: &'static str,
    pub detail_unknown_director: &'static str,
    pub detail_gem_score: &'static str,
    pub detail_gem_tooltip: &'static str,
    pub detail_rating: &'static str,
    pub detail_critics: &'static str,
    pub detail_audience: &'static str,
    pub detail_rank: &'static str, // "rank #{}"
    pub detail_imdb_link: &'static str,
    pub detail_trailer: &'static str,
    pub detail_where_watch: &'static str,
    pub detail_open_stremio: &'static str,
    pub providers_included_with: &'static str,
    pub providers_free_ads: &'static str,
    pub providers_rent_buy: &'static str,
    pub detail_poster_alt: &'static str, // "{} poster"
    pub detail_track: &'static str,
    pub detail_signin_add: &'static str,
    pub detail_your_list: &'static str,
    pub detail_saved_prefix: &'static str,
    pub detail_saved_link: &'static str,
    pub detail_more_genre: &'static str,  // "More {} gems →"
    pub detail_more_decade: &'static str, // "More from the {}s →"

    // ── Watchlist state buttons / badges (shared) ─────────────────────────────
    pub wl_want: &'static str,
    pub wl_watched: &'static str,
    pub wl_not_interested: &'static str,
    pub wl_remove: &'static str,

    // ── Card tooltips + aria labels ───────────────────────────────────────────
    pub card_gem_tooltip: &'static str,
    pub card_gem_aria: &'static str, // "Gem score {} percent"
    pub card_rating_tooltip: &'static str,
    pub card_rating_aria: &'static str, // "Community rating {} out of 10"
    pub card_critic_tooltip: &'static str,
    pub card_critic_aria: &'static str, // "Critic score {} percent"

    // ── Verify ────────────────────────────────────────────────────────────────
    pub verify_verifying: &'static str,
    pub verify_incomplete: &'static str,
    pub verify_invalid: &'static str,
    pub verify_request_new: &'static str,

    // ── Admin ─────────────────────────────────────────────────────────────────
    pub admin_h1: &'static str,
    pub admin_intro: &'static str,
    pub admin_warn_smtp: &'static str,
    pub admin_warn_tmdb: &'static str,
    pub admin_warn_omdb: &'static str,
    pub admin_log_rotation: &'static str,
    pub admin_log_rotation_hint: &'static str,
    pub admin_seed_title: &'static str,
    pub admin_seed_desc: &'static str,
    pub admin_seed_btn: &'static str,
    pub admin_sync_title: &'static str,
    pub admin_sync_desc: &'static str,
    pub admin_sync_btn: &'static str,
    pub admin_enrich_title: &'static str,
    pub admin_enrich_desc: &'static str,
    pub admin_enrich_limit: &'static str,
    pub admin_enrich_btn: &'static str,
    pub admin_score_title: &'static str,
    pub admin_score_desc: &'static str,
    pub admin_score_btn: &'static str,
    pub admin_providers_title: &'static str,
    pub admin_providers_desc: &'static str,
    pub admin_providers_btn: &'static str,
    pub admin_run_logs: &'static str,
    pub admin_refresh: &'static str,
    pub admin_loading: &'static str,
    pub admin_no_logs: &'static str,
    pub admin_started: &'static str,
    pub admin_running: &'static str,

    // ── Watchlist page ────────────────────────────────────────────────────────
    pub watchlist_h1: &'static str,
    pub watchlist_desc: &'static str,
    pub watchlist_cb_want: &'static str,
    pub watchlist_cb_watched: &'static str,
    pub watchlist_signin_prompt: &'static str,
    pub watchlist_signin_btn: &'static str,
    pub watchlist_empty: &'static str,

    // ── Privacy policy ────────────────────────────────────────────────────────
    pub privacy_h1: &'static str,
    pub privacy_dates: &'static str,
    pub privacy_who_h: &'static str,
    pub privacy_who_p1: &'static str, // ends before the email link
    pub privacy_collect_h: &'static str,
    pub privacy_collect_p1: &'static str,
    pub privacy_collect_p2: &'static str,
    pub privacy_collect_p3: &'static str,
    pub privacy_session_h: &'static str,
    pub privacy_session_p1a: &'static str, // before <em>
    pub privacy_session_em: &'static str,  // the <em> phrase
    pub privacy_session_p1b: &'static str, // after <em>
    pub privacy_session_p2: &'static str,
    pub privacy_providers_h: &'static str,
    pub privacy_providers_p: &'static str,
    pub privacy_where_h: &'static str,
    pub privacy_where_p: &'static str,
    pub privacy_retention_h: &'static str,
    pub privacy_retention_p: &'static str,
    pub privacy_legal_h: &'static str,
    pub privacy_legal_p1: &'static str,
    pub privacy_legal_p2: &'static str,
    pub privacy_rights_h: &'static str,
    pub privacy_rights_p1: &'static str, // before the email link
    pub privacy_rights_p2: &'static str,
    pub privacy_changes_h: &'static str,
    pub privacy_changes_p: &'static str,

    // ── About ─────────────────────────────────────────────────────────────────
    pub about_h1: &'static str,
    pub about_subtitle: &'static str,
    pub about_gem_h: &'static str,
    pub about_gem_p1: &'static str,
    pub about_gem_p2: &'static str,
    pub about_gem_p3: &'static str,
    pub about_acclaimed_h: &'static str,
    pub about_acclaimed_p: &'static str,
    pub about_wildcards_h: &'static str,
    pub about_wildcards_p: &'static str,
    // "The Name" story — prose fragments woven between fixed <em> terms
    // ("Pedrilha", "pedra", the song title, "com pedrinhas de brilhantes"),
    // which are the same in both languages.
    pub about_name_h: &'static str,
    pub about_name_1: &'static str, // after <em>Pedrilha</em>, before <em>pedra</em>
    pub about_name_2: &'static str, // after <em>pedra</em>, before the song title
    pub about_name_3: &'static str, // after the song title, before the lyric
    pub about_name_4: &'static str, // after the lyric, to the end

    // ── Changelog ─────────────────────────────────────────────────────────────
    pub changelog_h1: &'static str,
    pub changelog_desc: &'static str,
}

pub const EN: Dict = Dict {
    nav_gems: "GEMS",
    nav_acclaimed: "ACCLAIMED",
    nav_wildcards: "WILDCARDS",
    nav_admin: "ADMIN",
    nav_watchlist: "WATCHLIST",
    nav_signout: "SIGN OUT",
    nav_signin: "SIGN IN",

    footer_tagline: "Pedrilha — Unearthing what the blockbusters buried.",
    footer_how_it_works: "How it works",
    footer_privacy: "Privacy Policy",
    footer_no_cookies_ads: "No cookies · No ads",

    meta_description: "Discover hidden gem movies",

    nf_meta_title: "Not found — Pedrilha",
    nf_title: "404 — NOT IN THE VAULT",
    nf_body: "This reel doesn't exist.",
    nf_back: "← Back to the gems",

    title_home: "Hidden Gems — Pedrilha",
    title_acclaimed: "Acclaimed — Pedrilha",
    title_wildcards: "Wildcards — Pedrilha",
    title_signin: "Sign in — Pedrilha",
    title_signing_in: "Signing in — Pedrilha",
    title_admin: "Admin — Pedrilha",
    title_watchlist: "Watchlist — Pedrilha",
    title_privacy: "Privacy Policy — Pedrilha",
    title_about: "How it works — Pedrilha",
    title_changelog: "Changelog — Pedrilha",

    home_h1: "Hidden Gems",
    home_desc: "Films in the 6.5–7.9 rating sweet spot — seen by few, worth seeing by many.",
    acclaimed_h1: "Acclaimed",
    acclaimed_desc: "8.0+ community rating and 80%+ critic score — films everyone should see.",
    wildcards_h1: "Wildcards",
    wildcards_desc: "Loved by the algorithm, panned by critics — cult classics and guilty pleasures live here.",
    wildcards_desc2: "A low critic score isn't always wrong — but sometimes it is.",

    filter_search_placeholder: "Search titles…  ( / )",
    filter_clear_search: "Clear search",
    filter_genre: "Genre",
    filter_genre_count: "Genre ({})",
    filter_era: "Era",
    filter_from_era: "From era",
    filter_sort_by: "Sort by",
    filter_direction: "Direction",
    filter_desc: "↓ Desc",
    filter_asc: "↑ Asc",
    filter_films_count: "{} films",
    filter_no_matches: "No matches",
    filter_clear: "✕ clear",

    filter_watchable: "What can I watch?",
    filter_region: "Region",
    filter_include_rentals: "Include rentals",
    filter_rentals_hint: "Also show titles to rent or buy (no subscription needed).",
    filter_active_chip: "{} watch filters",
    filter_empty_hint: "No providers synced yet for this region.",
    badge_included: "Included",
    badge_rent: "Rent",
    providers_attribution: "Streaming data by JustWatch",

    page_label: "Page",
    page_prev: "← Prev",
    page_next: "Next →",

    grid_err_load: "Couldn't load films — the server may be waking up.",
    grid_try_again: "Try again",
    grid_no_gems: "No gems match those filters.",
    grid_no_watch_matches: "Nothing here streams on your selected services. Try adding services, including rentals, or clearing the filter.",
    grid_clear_filters: "Clear filters",

    signin_check_email: "Check your email",
    signin_sent_prefix: "We sent a sign-in link to ",
    signin_expires: "Click the link in the email to sign in. It expires in 15 minutes.",
    signin_use_different: "Use a different email",
    signin_tagline: "Track films you want to see or have seen.",
    signin_heading: "Sign in",
    signin_blurb: "Enter your email — we'll send a magic link. New here? Your account is created automatically.",
    signin_email_label: "Email",
    signin_sending: "Sending…",
    signin_send_link: "Send sign-in link",
    signin_back: "← Back to Pedrilha",
    signin_err_empty: "Please enter your email address.",
    signin_err_invalid: "That doesn't look like a valid email address.",
    signin_err_generic: "Something went wrong — please try again.",

    detail_back: "← Back",
    detail_err_load: "Couldn't load this film — the server may be waking up.",
    detail_back_gems: "← Back to Gems",
    detail_unknown_director: "Unknown",
    detail_gem_score: "✦ Gem Score",
    detail_gem_tooltip: "What is this? — how scoring works",
    detail_rating: "Rating",
    detail_critics: "Critics",
    detail_audience: "Audience",
    detail_rank: "rank #{}",
    detail_imdb_link: "View on IMDb →",
    detail_trailer: "Trailer on YouTube →",
    detail_where_watch: "Where to watch →",
    detail_open_stremio: "Open in Stremio",
    providers_included_with: "Included with",
    providers_free_ads: "Free with ads",
    providers_rent_buy: "Rent or buy",
    detail_poster_alt: "{} poster",
    detail_track: "Track this film in your watchlist",
    detail_signin_add: "Sign in to add",
    detail_your_list: "Your list",
    detail_saved_prefix: "Saved — ",
    detail_saved_link: "view your watchlist →",
    detail_more_genre: "More {} gems →",
    detail_more_decade: "More from the {}s →",

    wl_want: "🔖 Want to watch",
    wl_watched: "✓ Watched",
    wl_not_interested: "✗ Not interested",
    wl_remove: "Remove",

    card_gem_tooltip: "Gem Score — how undiscovered this film is (100% = top gem)",
    card_gem_aria: "Gem score {} percent",
    card_rating_tooltip: "Community rating (0–10)",
    card_rating_aria: "Community rating {} out of 10",
    card_critic_tooltip: "Critic score",
    card_critic_aria: "Critic score {} percent",

    verify_verifying: "Verifying…",
    verify_incomplete: "This sign-in link is incomplete — no token provided.",
    verify_invalid: "This sign-in link is invalid or has expired — they last 15 minutes.",
    verify_request_new: "Request a new link",

    admin_h1: "Admin",
    admin_intro: "Operations run on the server — you can close this page. Check logs below for progress.",
    admin_warn_smtp: "Magic-link sign-in is unavailable. Users cannot create accounts or sign in.",
    admin_warn_tmdb: "Movie sync is unavailable. The database cannot be populated with new films.",
    admin_warn_omdb: "OMDb enrichment is unavailable. IMDb ratings and RT scores will not be fetched.",
    admin_log_rotation: "Log rotation",
    admin_log_rotation_hint: "set LOG_ROTATION=never|daily|hourly in .env, restart to apply",
    admin_seed_title: "Seed Test Data",
    admin_seed_desc: "Runs the full pipeline: sync all eras + blockbusters, enrich via OMDb (2000 limit), score, classify acclaimed. Use on a fresh database. Takes 20–60+ min.",
    admin_seed_btn: "Seed",
    admin_sync_title: "Sync Movies",
    admin_sync_desc: "Fetch new movies from TMDB across all era windows + blockbusters. 5–15 min.",
    admin_sync_btn: "Sync",
    admin_enrich_title: "Enrich via OMDb",
    admin_enrich_desc: "Fetch community ratings and critic scores. 10–30 min for large batches.",
    admin_enrich_limit: "Limit:",
    admin_enrich_btn: "Enrich",
    admin_score_title: "Run Scoring",
    admin_score_desc: "Recalculate gem scores and re-classify wildcards. Under 1 min.",
    admin_score_btn: "Score",
    admin_providers_title: "Sync Streaming Providers",
    admin_providers_desc: "Fetch where each gem streams (TMDB / JustWatch) for US + BR. 10–30 min.",
    admin_providers_btn: "Providers",
    admin_run_logs: "Run Logs",
    admin_refresh: "Refresh",
    admin_loading: "Loading…",
    admin_no_logs: "No logs yet. Run an operation above.",
    admin_started: "Started — watch logs below",
    admin_running: "Running…",

    watchlist_h1: "Watchlist",
    watchlist_desc: "Films you're tracking.",
    watchlist_cb_want: "🔖 Want to Watch",
    watchlist_cb_watched: "✓ Watched",
    watchlist_signin_prompt: "Sign in to see your watchlist.",
    watchlist_signin_btn: "Sign in",
    watchlist_empty: "Nothing here yet — find a film and add it to your watchlist.",

    privacy_h1: "PRIVACY POLICY",
    privacy_dates: "Effective date: 2025-01-01 · Last updated: 2026-06-28",
    privacy_who_h: "Who we are",
    privacy_who_p1: "Pedrilha is operated by RK. Questions: ",
    privacy_collect_h: "What we collect",
    privacy_collect_p1: "If you create an account, we store your email address to send you a sign-in link and to identify your watchlist and ratings. We do not store passwords.",
    privacy_collect_p2: "We also collect anonymous usage analytics to understand how the site is used and improve it: pages visited, movies clicked, filters applied, and pagination events. This data contains no personal information.",
    privacy_collect_p3: "We use no advertising trackers and sell no data to third parties — ever.",
    privacy_session_h: "How session identity works",
    privacy_session_p1a: "For analytics, we compute a one-way SHA-256 hash from your IP address, browser user-agent string, and the current UTC date. This hash rotates every day — the same device produces a different hash on different days. Your raw IP address and user-agent are ",
    privacy_session_em: "never written to disk",
    privacy_session_p1b: ". The hash cannot be reversed.",
    privacy_session_p2: "We use no cookies for analytics. Authentication uses a short-lived token stored in your browser's local storage, which is cleared when you sign out.",
    privacy_providers_h: "Analytics providers",
    privacy_providers_p: "We run self-hosted, privacy-first analytics (Umami) on our own infrastructure. No data is shared with Google Analytics, Meta, or any third-party analytics service. All data stays on our servers.",
    privacy_where_h: "Where data is stored",
    privacy_where_p: "All data is stored on servers located in New York, USA.",
    privacy_retention_h: "Retention",
    privacy_retention_p: "Analytics events are automatically deleted after 12 months. Account data (email address, watchlist, ratings) is retained until you request deletion.",
    privacy_legal_h: "Legal basis",
    privacy_legal_p1: "We process your email address to perform the contract you enter into when creating an account (GDPR Art. 6(1)(b)).",
    privacy_legal_p2: "We process analytics data on the basis of legitimate interest (GDPR Art. 6(1)(f)): understanding how the site is used so we can improve it. Because we use no cookies and store no personal data in analytics, no consent is required under the ePrivacy Directive.",
    privacy_rights_h: "Your rights",
    privacy_rights_p1: "Under GDPR you have the right to access, correct, or erase data we hold about you, and to object to processing. To exercise any of these rights, email ",
    privacy_rights_p2: "Because analytics events are stored as daily-rotating anonymous hashes with no link to your account, we cannot identify or delete your specific analytics history. We can delete your account and all associated watchlist and rating data on request.",
    privacy_changes_h: "Changes",
    privacy_changes_p: "If we materially change how we handle data, we will update the date at the top of this page. Continued use of the site constitutes acceptance.",

    about_h1: "HOW IT WORKS",
    about_subtitle: "Understanding the Gem Score",
    about_gem_h: "The Gem Score",
    about_gem_p1: "The sweet spot is a community rating between 6.5 and 7.9. High enough that people genuinely liked it, low enough that it never became a household name. Films with fewer votes rank higher than films everyone has already seen, and older films get a small nudge up because time buries things.",
    about_gem_p2: "Critics panning a film knocks it out of contention entirely. This isn't a list of so-bad-they're-good movies. If a film opened the same weekend as a massive blockbuster and got overshadowed, it picks up a small boost for that.",
    about_gem_p3: "Each time scores are calculated, the best-ranking film gets 100%. Everything else is measured against it.",
    about_acclaimed_h: "Acclaimed",
    about_acclaimed_p: "Community rating of 8.0 or above, critic score of 80% or above. Films that audiences and critics both got behind.",
    about_wildcards_h: "Wildcards",
    about_wildcards_p: "Scores well by the numbers, but critics hated it. Cult classics, midnight movies, guilty pleasures. The kind of films that find their audience years later.",
    about_name_h: "The Name",
    about_name_1: " is an affectionate diminutive of ",
    about_name_2: " — stone, in Portuguese. There's an old Brazilian folk song, ",
    about_name_3: ", that dreams of paving a street ",
    about_name_4: " — with little diamond stones — for someone you love to walk on. That's the idea here: a lot of small, overlooked stones, laid out for someone to find.",

    changelog_h1: "Changelog",
    changelog_desc: "Notable changes to Pedrilha.",
};

pub const PT: Dict = Dict {
    nav_gems: "PEDRINHAS",
    nav_acclaimed: "ACLAMADOS",
    nav_wildcards: "CURINGAS",
    nav_admin: "ADMIN",
    nav_watchlist: "MINHA LISTA",
    nav_signout: "SAIR",
    nav_signin: "ENTRAR",

    footer_tagline: "Pedrilha — Desenterrando o que os blockbusters enterraram.",
    footer_how_it_works: "Como funciona",
    footer_privacy: "Política de Privacidade",
    footer_no_cookies_ads: "Sem cookies · Sem anúncios",

    meta_description: "Descubra filmes que são pedrinhas escondidas",

    nf_meta_title: "Não encontrado — Pedrilha",
    nf_title: "404 — NÃO ESTÁ NO COFRE",
    nf_body: "Este rolo não existe.",
    nf_back: "← Voltar às pedrinhas",

    title_home: "Pedrinhas Escondidas — Pedrilha",
    title_acclaimed: "Aclamados — Pedrilha",
    title_wildcards: "Curingas — Pedrilha",
    title_signin: "Entrar — Pedrilha",
    title_signing_in: "Entrando — Pedrilha",
    title_admin: "Admin — Pedrilha",
    title_watchlist: "Minha Lista — Pedrilha",
    title_privacy: "Política de Privacidade — Pedrilha",
    title_about: "Como funciona — Pedrilha",
    title_changelog: "Novidades — Pedrilha",

    home_h1: "Pedrinhas Escondidas",
    home_desc: "Filmes na faixa ideal de nota 6,5–7,9 — vistos por poucos, dignos de serem vistos por muitos.",
    acclaimed_h1: "Aclamados",
    acclaimed_desc: "Nota do público 8,0+ e nota da crítica 80%+ — filmes que todo mundo deveria ver.",
    wildcards_h1: "Curingas",
    wildcards_desc: "Amados pelo algoritmo, detonados pela crítica — clássicos cult e prazeres culpados moram aqui.",
    wildcards_desc2: "Uma nota baixa da crítica nem sempre está errada — mas às vezes está.",

    filter_search_placeholder: "Buscar títulos…  ( / )",
    filter_clear_search: "Limpar busca",
    filter_genre: "Gênero",
    filter_genre_count: "Gênero ({})",
    filter_era: "Época",
    filter_from_era: "A partir de",
    filter_sort_by: "Ordenar por",
    filter_direction: "Direção",
    filter_desc: "↓ Maior",
    filter_asc: "↑ Menor",
    filter_films_count: "{} filmes",
    filter_no_matches: "Nenhum resultado",
    filter_clear: "✕ limpar",

    filter_watchable: "O que posso assistir?",
    filter_region: "Região",
    filter_include_rentals: "Incluir aluguéis",
    filter_rentals_hint: "Também mostrar títulos para alugar ou comprar (sem assinatura).",
    filter_active_chip: "{} filtros de streaming",
    filter_empty_hint: "Nenhum serviço sincronizado ainda para esta região.",
    badge_included: "Incluído",
    badge_rent: "Alugar",
    providers_attribution: "Dados de streaming por JustWatch",

    page_label: "Página",
    page_prev: "← Anterior",
    page_next: "Próxima →",

    grid_err_load: "Não foi possível carregar os filmes — o servidor pode estar acordando.",
    grid_try_again: "Tentar de novo",
    grid_no_gems: "Nenhuma pedrinha corresponde a esses filtros.",
    grid_no_watch_matches: "Nada aqui está nos serviços selecionados. Adicione serviços, inclua aluguéis ou limpe o filtro.",
    grid_clear_filters: "Limpar filtros",

    signin_check_email: "Verifique seu e-mail",
    signin_sent_prefix: "Enviamos um link de acesso para ",
    signin_expires: "Clique no link do e-mail para entrar. Ele expira em 15 minutos.",
    signin_use_different: "Usar outro e-mail",
    signin_tagline: "Acompanhe filmes que você quer ver ou já viu.",
    signin_heading: "Entrar",
    signin_blurb: "Digite seu e-mail — enviaremos um link mágico. Novo por aqui? Sua conta é criada automaticamente.",
    signin_email_label: "E-mail",
    signin_sending: "Enviando…",
    signin_send_link: "Enviar link de acesso",
    signin_back: "← Voltar ao Pedrilha",
    signin_err_empty: "Por favor, digite seu endereço de e-mail.",
    signin_err_invalid: "Isso não parece um endereço de e-mail válido.",
    signin_err_generic: "Algo deu errado — por favor, tente de novo.",

    detail_back: "← Voltar",
    detail_err_load: "Não foi possível carregar este filme — o servidor pode estar acordando.",
    detail_back_gems: "← Voltar às Pedrinhas",
    detail_unknown_director: "Desconhecido",
    detail_gem_score: "✦ Nota Pedrinha",
    detail_gem_tooltip: "O que é isso? — como funciona a pontuação",
    detail_rating: "Nota",
    detail_critics: "Crítica",
    detail_audience: "Público",
    detail_rank: "posição #{}",
    detail_imdb_link: "Ver no IMDb →",
    detail_trailer: "Trailer no YouTube →",
    detail_where_watch: "Onde assistir →",
    detail_open_stremio: "Abrir no Stremio",
    providers_included_with: "Incluído em",
    providers_free_ads: "Grátis com anúncios",
    providers_rent_buy: "Alugar ou comprar",
    detail_poster_alt: "pôster de {}",
    detail_track: "Acompanhe este filme na sua lista",
    detail_signin_add: "Entre para adicionar",
    detail_your_list: "Sua lista",
    detail_saved_prefix: "Salvo — ",
    detail_saved_link: "ver sua lista →",
    detail_more_genre: "Mais pedrinhas de {} →",
    detail_more_decade: "Mais dos anos {} →",

    wl_want: "🔖 Quero ver",
    wl_watched: "✓ Já vi",
    wl_not_interested: "✗ Sem interesse",
    wl_remove: "Remover",

    card_gem_tooltip: "Nota Pedrinha — o quão desconhecido é este filme (100% = pedrinha máxima)",
    card_gem_aria: "Nota pedrinha {} por cento",
    card_rating_tooltip: "Nota do público (0–10)",
    card_rating_aria: "Nota do público {} de 10",
    card_critic_tooltip: "Nota da crítica",
    card_critic_aria: "Nota da crítica {} por cento",

    verify_verifying: "Verificando…",
    verify_incomplete: "Este link de acesso está incompleto — nenhum token informado.",
    verify_invalid: "Este link de acesso é inválido ou expirou — eles duram 15 minutos.",
    verify_request_new: "Solicitar um novo link",

    admin_h1: "Admin",
    admin_intro: "As operações rodam no servidor — você pode fechar esta página. Veja os logs abaixo para acompanhar o progresso.",
    admin_warn_smtp: "O acesso por link mágico está indisponível. Usuários não conseguem criar contas nem entrar.",
    admin_warn_tmdb: "A sincronização de filmes está indisponível. O banco de dados não pode ser populado com novos filmes.",
    admin_warn_omdb: "O enriquecimento via OMDb está indisponível. Notas do IMDb e da RT não serão obtidas.",
    admin_log_rotation: "Rotação de logs",
    admin_log_rotation_hint: "defina LOG_ROTATION=never|daily|hourly no .env e reinicie para aplicar",
    admin_seed_title: "Popular Dados de Teste",
    admin_seed_desc: "Executa o pipeline completo: sincroniza todas as épocas + blockbusters, enriquece via OMDb (limite 2000), pontua e classifica os aclamados. Use num banco novo. Leva de 20 a 60+ min.",
    admin_seed_btn: "Popular",
    admin_sync_title: "Sincronizar Filmes",
    admin_sync_desc: "Busca novos filmes no TMDB em todas as janelas de época + blockbusters. 5–15 min.",
    admin_sync_btn: "Sincronizar",
    admin_enrich_title: "Enriquecer via OMDb",
    admin_enrich_desc: "Busca notas do público e da crítica. 10–30 min para lotes grandes.",
    admin_enrich_limit: "Limite:",
    admin_enrich_btn: "Enriquecer",
    admin_score_title: "Executar Pontuação",
    admin_score_desc: "Recalcula as notas pedrinha e reclassifica os curingas. Menos de 1 min.",
    admin_score_btn: "Pontuar",
    admin_providers_title: "Sincronizar Serviços de Streaming",
    admin_providers_desc: "Busca onde cada pedrinha está disponível (TMDB / JustWatch) para EUA + BR. 10–30 min.",
    admin_providers_btn: "Serviços",
    admin_run_logs: "Logs de Execução",
    admin_refresh: "Atualizar",
    admin_loading: "Carregando…",
    admin_no_logs: "Nenhum log ainda. Execute uma operação acima.",
    admin_started: "Iniciado — veja os logs abaixo",
    admin_running: "Executando…",

    watchlist_h1: "Minha Lista",
    watchlist_desc: "Filmes que você está acompanhando.",
    watchlist_cb_want: "🔖 Quero Ver",
    watchlist_cb_watched: "✓ Já Vi",
    watchlist_signin_prompt: "Entre para ver sua lista.",
    watchlist_signin_btn: "Entrar",
    watchlist_empty: "Nada aqui ainda — encontre um filme e adicione à sua lista.",

    privacy_h1: "POLÍTICA DE PRIVACIDADE",
    privacy_dates: "Data de vigência: 2025-01-01 · Última atualização: 2026-06-28",
    privacy_who_h: "Quem somos",
    privacy_who_p1: "O Pedrilha é operado por RK. Dúvidas: ",
    privacy_collect_h: "O que coletamos",
    privacy_collect_p1: "Se você cria uma conta, armazenamos seu endereço de e-mail para enviar um link de acesso e para identificar sua lista e suas avaliações. Não armazenamos senhas.",
    privacy_collect_p2: "Também coletamos análises de uso anônimas para entender como o site é usado e melhorá-lo: páginas visitadas, filmes clicados, filtros aplicados e eventos de paginação. Esses dados não contêm nenhuma informação pessoal.",
    privacy_collect_p3: "Não usamos rastreadores de publicidade e não vendemos dados a terceiros — nunca.",
    privacy_session_h: "Como funciona a identidade de sessão",
    privacy_session_p1a: "Para as análises, calculamos um hash SHA-256 unidirecional a partir do seu endereço IP, da string de user-agent do navegador e da data UTC atual. Esse hash muda todos os dias — o mesmo dispositivo gera um hash diferente em dias diferentes. Seu endereço IP e user-agent brutos ",
    privacy_session_em: "nunca são gravados em disco",
    privacy_session_p1b: ". O hash não pode ser revertido.",
    privacy_session_p2: "Não usamos cookies para análises. A autenticação usa um token de curta duração armazenado no armazenamento local do seu navegador, que é apagado quando você sai.",
    privacy_providers_h: "Provedores de análise",
    privacy_providers_p: "Rodamos análises auto-hospedadas e focadas em privacidade (Umami) na nossa própria infraestrutura. Nenhum dado é compartilhado com Google Analytics, Meta ou qualquer serviço de análise de terceiros. Todos os dados permanecem em nossos servidores.",
    privacy_where_h: "Onde os dados são armazenados",
    privacy_where_p: "Todos os dados são armazenados em servidores localizados em Nova York, EUA.",
    privacy_retention_h: "Retenção",
    privacy_retention_p: "Os eventos de análise são apagados automaticamente após 12 meses. Os dados da conta (endereço de e-mail, lista, avaliações) são mantidos até que você solicite a exclusão.",
    privacy_legal_h: "Base legal",
    privacy_legal_p1: "Processamos seu endereço de e-mail para executar o contrato firmado ao criar uma conta (GDPR Art. 6(1)(b)).",
    privacy_legal_p2: "Processamos os dados de análise com base em legítimo interesse (GDPR Art. 6(1)(f)): entender como o site é usado para que possamos melhorá-lo. Como não usamos cookies e não armazenamos dados pessoais nas análises, nenhum consentimento é exigido pela Diretiva ePrivacy.",
    privacy_rights_h: "Seus direitos",
    privacy_rights_p1: "Sob o GDPR, você tem o direito de acessar, corrigir ou apagar os dados que mantemos sobre você e de se opor ao processamento. Para exercer qualquer um desses direitos, escreva para ",
    privacy_rights_p2: "Como os eventos de análise são armazenados como hashes anônimos que mudam diariamente e sem vínculo com sua conta, não conseguimos identificar nem apagar seu histórico de análises específico. Podemos apagar sua conta e todos os dados de lista e avaliação associados mediante solicitação.",
    privacy_changes_h: "Alterações",
    privacy_changes_p: "Se mudarmos de forma significativa como tratamos os dados, atualizaremos a data no topo desta página. O uso continuado do site constitui aceitação.",

    about_h1: "COMO FUNCIONA",
    about_subtitle: "Entendendo a Nota Pedrinha",
    about_gem_h: "A Nota Pedrinha",
    about_gem_p1: "A faixa ideal é uma nota do público entre 6,5 e 7,9. Alta o bastante para que as pessoas tenham realmente gostado, baixa o bastante para que nunca tenha virado nome de família. Filmes com menos votos ficam à frente dos filmes que todo mundo já viu, e filmes mais antigos ganham um pequeno empurrão porque o tempo enterra as coisas.",
    about_gem_p2: "A crítica detonar um filme o tira totalmente da disputa. Esta não é uma lista de filmes tão ruins que são bons. Se um filme estreou no mesmo fim de semana que um blockbuster gigante e foi ofuscado, ele ganha um pequeno impulso por isso.",
    about_gem_p3: "A cada vez que as notas são calculadas, o filme melhor colocado recebe 100%. Todo o resto é medido em relação a ele.",
    about_acclaimed_h: "Aclamados",
    about_acclaimed_p: "Nota do público de 8,0 ou mais e nota da crítica de 80% ou mais. Filmes que o público e a crítica abraçaram.",
    about_wildcards_h: "Curingas",
    about_wildcards_p: "Vão bem nos números, mas a crítica odiou. Clássicos cult, sessões da meia-noite, prazeres culpados. O tipo de filme que encontra seu público anos depois.",
    about_name_h: "O Nome",
    about_name_1: " é um diminutivo carinhoso de ",
    about_name_2: " — pedra. Há uma antiga canção folclórica brasileira, ",
    about_name_3: ", que sonha em calçar uma rua ",
    about_name_4: " — pedrinhas de brilhantes — para alguém que você ama caminhar. É essa a ideia aqui: um monte de pedrinhas pequenas e esquecidas, dispostas para alguém encontrar.",

    changelog_h1: "Novidades",
    changelog_desc: "Mudanças importantes no Pedrilha.",
};

/// The dictionary for a given language.
#[inline]
pub fn dict(lang: Lang) -> &'static Dict {
    match lang {
        Lang::En => &EN,
        Lang::Pt => &PT,
    }
}

/// Translate a genre *label* for display. The *value* (and what's sent to the API /
/// URL) stays the canonical English TMDB name — only the visible label changes.
/// Accepts a runtime `&str` (selected genres arrive as `String`) and returns a
/// `'static` label for the fixed genre set.
pub fn genre_label(lang: Lang, en: &str) -> &'static str {
    let (en_label, pt_label): (&'static str, &'static str) = match en {
        "Action" => ("Action", "Ação"),
        "Animation" => ("Animation", "Animação"),
        "Comedy" => ("Comedy", "Comédia"),
        "Crime" => ("Crime", "Policial"),
        "Drama" => ("Drama", "Drama"),
        "Horror" => ("Horror", "Terror"),
        "Romance" => ("Romance", "Romance"),
        "Science Fiction" => ("Science Fiction", "Ficção Científica"),
        "Thriller" => ("Thriller", "Suspense"),
        "Documentary" => ("Documentary", "Documentário"),
        "Western" => ("Western", "Faroeste"),
        "War" => ("War", "Guerra"),
        "Musical" => ("Musical", "Musical"),
        _ => ("", ""),
    };
    match lang {
        Lang::En => en_label,
        Lang::Pt => pt_label,
    }
}

/// Translate a sort *label* for display. The value (`score`, `rating`, `rt`, `year`,
/// `title`) is the API sort key and never changes.
pub fn sort_label(lang: Lang, value: &str) -> &'static str {
    match (lang, value) {
        (Lang::En, "score") => "Score",
        (Lang::En, "rating") => "Rating",
        (Lang::En, "rt") => "Critics",
        (Lang::En, "year") => "Year",
        (Lang::En, "title") => "Title",
        (Lang::Pt, "score") => "Pedrinha",
        (Lang::Pt, "rating") => "Nota",
        (Lang::Pt, "rt") => "Crítica",
        (Lang::Pt, "year") => "Ano",
        (Lang::Pt, "title") => "Título",
        // Fallback to the gem-score label if an unknown key ever arrives.
        (Lang::En, _) => "Score",
        (Lang::Pt, _) => "Pedrinha",
    }
}

/// Read the `Lang` signal from context. Panics if `App` did not provide it.
pub fn use_lang() -> RwSignal<Lang> {
    use_context::<RwSignal<Lang>>().expect("Lang context missing — provide it in App")
}

/// First-run locale: localStorage override, else `navigator.languages`
/// (any `pt*` ⇒ PT), else EN.
pub fn detect_lang() -> Lang {
    if let Some(ls) = crate::local_storage() {
        if let Ok(Some(v)) = ls.get_item(LS_LANG) {
            match v.as_str() {
                "pt" => return Lang::Pt,
                "en" => return Lang::En,
                _ => {}
            }
        }
    }
    if let Some(nav) = web_sys::window().map(|w| w.navigator()) {
        let langs = nav.languages();
        for i in 0..langs.length() {
            if let Some(s) = langs.get(i).as_string() {
                let s = s.to_lowercase();
                if s.starts_with("pt") {
                    return Lang::Pt;
                }
                if s.starts_with("en") {
                    return Lang::En;
                }
            }
        }
        if let Some(l) = nav.language() {
            if l.to_lowercase().starts_with("pt") {
                return Lang::Pt;
            }
        }
    }
    Lang::En
}

/// Persist the chosen locale to localStorage.
pub fn save_lang(lang: Lang) {
    if let Some(ls) = crate::local_storage() {
        let _ = ls.set_item(LS_LANG, lang.code());
    }
}
