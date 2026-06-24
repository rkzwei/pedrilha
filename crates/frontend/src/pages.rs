use crate::api;
use gem_finder_shared::types::MovieSummary;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::web_sys;
use leptos_router::{components::A, hooks::use_params_map};

const GENRES: &[&str] = &[
    "Action",
    "Comedy",
    "Drama",
    "Horror",
    "Thriller",
    "Science Fiction",
    "Romance",
    "Documentary",
    "Foreign",
    "Indie",
];
const DECADE_OPTIONS: &[(i32, &str)] = &[
    (1960, "1960s+"),
    (1970, "1970s+"),
    (1980, "1980s+"),
    (1990, "1990s+"),
    (2000, "2000s+"),
    (2010, "2010s+"),
    (2020, "2020s+"),
];
const PER_PAGE: i32 = 20;

// ── Home page ────────────────────────────────────────────────────────────────
#[component]
pub fn HomePage() -> impl IntoView {
    let (page, set_page) = signal(1i32);
    let (min_year, set_min_year) = signal(Option::<i32>::None);
    let (genre, set_genre) = signal(Option::<String>::None);
    let (movies, set_movies) = signal(Vec::<MovieSummary>::new());
    let (total, set_total) = signal(0i64);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);

    Effect::new(move |_| {
        let p = page.get();
        let y = min_year.get();
        let g = genre.get();
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::fetch_gems(p, PER_PAGE, y, g).await {
                Ok(resp) => {
                    set_movies.set(resp.data);
                    set_total.set(resp.total);
                    set_loading.set(false);
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_loading.set(false);
                }
            }
        });
    });

    let total_pages = move || ((total.get() as f64) / (PER_PAGE as f64)).ceil() as i32;

    view! {
        <div class="max-w-7xl mx-auto px-4 py-8">
            <div class="mb-8">
                <h1 class="text-4xl font-bold text-stone-100 mb-2">"Hidden Gems"</h1>
                <p class="text-stone-400 text-lg">"Movies rated 6.5–7.9 on IMDb that deserve your attention."</p>
            </div>

            <div class="flex flex-wrap gap-4 mb-6 p-4 bg-[#17100a] rounded border border-[#2b1e14]">
                <div class="flex flex-col gap-1">
                    <label class="text-xs text-stone-500 uppercase tracking-wide">"Era"</label>
                    <select class="bg-[#201610] text-stone-200 border border-[#342517] rounded px-3 py-2 text-sm"
                        on:change=move |ev| {
                            let v = event_target_value(&ev);
                            set_page.set(1);
                            set_min_year.set(if v.is_empty() { None } else { v.parse().ok() });
                        }>
                        <option value="">"All eras"</option>
                        {DECADE_OPTIONS.iter().map(|(y,l)| view!{ <option value={y.to_string()}>{*l}</option> }).collect::<Vec<_>>()}
                    </select>
                </div>
                <div class="flex flex-col gap-1">
                    <label class="text-xs text-stone-500 uppercase tracking-wide">"Genre"</label>
                    <select class="bg-[#201610] text-stone-200 border border-[#342517] rounded px-3 py-2 text-sm"
                        on:change=move |ev| {
                            let v = event_target_value(&ev);
                            set_page.set(1);
                            set_genre.set(if v.is_empty() { None } else { Some(v) });
                        }>
                        <option value="">"All genres"</option>
                        {GENRES.iter().map(|g| view!{ <option value={*g}>{*g}</option> }).collect::<Vec<_>>()}
                    </select>
                </div>
                <div class="flex items-end ml-auto">
                    <span class="text-sm text-stone-500">
                        {move || { let t = total.get(); if t > 0 { format!("{} gems", t) } else { String::new() } }}
                    </span>
                </div>
            </div>

            {move || if loading.get() {
                view!{ <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                    {(0..10).map(|_| view!{ <SkeletonCard /> }).collect::<Vec<_>>()}
                </div> }.into_any()
            } else if let Some(err) = error.get() {
                view!{ <div class="py-16 text-center"><p class="text-red-400">{err}</p></div> }.into_any()
            } else if movies.get().is_empty() {
                view!{ <div class="py-16 text-center text-stone-500">"No gems match your filters."</div> }.into_any()
            } else {
                view!{ <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                    {move || movies.get().into_iter().map(|m| view!{ <MovieCard movie=m /> }).collect::<Vec<_>>()}
                </div> }.into_any()
            }}

            {move || {
                let tp = total_pages();
                if tp <= 1 { return view!{ <div /> }.into_any(); }
                let cur = page.get();
                let prev_disabled = page.get() <= 1;
                let next_disabled = page.get() >= tp;
                view!{
                    <div class="flex items-center justify-center gap-4 mt-10">
                        <button class="px-4 py-2 bg-[#201610] text-stone-200 rounded disabled:opacity-30 hover:bg-[#2b1e14]"
                            disabled=prev_disabled
                            on:click=move |_| set_page.update(|p| *p -= 1)>"← Prev"</button>
                        <span class="text-stone-400 text-sm">{format!("Page {} of {}", cur, tp)}</span>
                        <button class="px-4 py-2 bg-[#201610] text-stone-200 rounded disabled:opacity-30 hover:bg-[#2b1e14]"
                            disabled=next_disabled
                            on:click=move |_| set_page.update(|p| *p += 1)>"Next →"</button>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ── Acclaimed page ───────────────────────────────────────────────────────────
#[component]
pub fn AcclaimedPage() -> impl IntoView {
    let (page, set_page) = signal(1i32);
    let (movies, set_movies) = signal(Vec::<MovieSummary>::new());
    let (total, set_total) = signal(0i64);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);

    Effect::new(move |_| {
        let p = page.get();
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::fetch_acclaimed(p, PER_PAGE).await {
                Ok(resp) => {
                    set_movies.set(resp.data);
                    set_total.set(resp.total);
                    set_loading.set(false);
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_loading.set(false);
                }
            }
        });
    });

    let total_pages = move || ((total.get() as f64) / (PER_PAGE as f64)).ceil() as i32;

    view! {
        <div class="max-w-7xl mx-auto px-4 py-8">
            <div class="mb-8">
                <h1 class="text-4xl font-bold text-stone-100 mb-2">"Acclaimed"</h1>
                <p class="text-stone-400 text-lg">"IMDb 8.0+ and RT 80%+ — films everyone should see."</p>
            </div>
            {move || if loading.get() {
                view!{ <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                    {(0..10).map(|_| view!{ <SkeletonCard /> }).collect::<Vec<_>>()}
                </div> }.into_any()
            } else if let Some(err) = error.get() {
                view!{ <div class="py-16 text-center"><p class="text-red-400">{err}</p></div> }.into_any()
            } else {
                view!{ <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                    {move || movies.get().into_iter().map(|m| view!{ <MovieCard movie=m /> }).collect::<Vec<_>>()}
                </div> }.into_any()
            }}
            {move || {
                let tp = total_pages();
                if tp <= 1 { return view!{ <div /> }.into_any(); }
                let cur = page.get();
                let prev_disabled = page.get() <= 1;
                let next_disabled = page.get() >= tp;
                view!{
                    <div class="flex items-center justify-center gap-4 mt-10">
                        <button class="px-4 py-2 bg-[#201610] text-stone-200 rounded disabled:opacity-30 hover:bg-[#2b1e14]"
                            disabled=prev_disabled
                            on:click=move |_| set_page.update(|p| *p -= 1)>"← Prev"</button>
                        <span class="text-stone-400 text-sm">{format!("Page {} of {}", cur, tp)}</span>
                        <button class="px-4 py-2 bg-[#201610] text-stone-200 rounded disabled:opacity-30 hover:bg-[#2b1e14]"
                            disabled=next_disabled
                            on:click=move |_| set_page.update(|p| *p += 1)>"Next →"</button>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ── Wildcards page ───────────────────────────────────────────────────────────
#[component]
pub fn WildcardsPage() -> impl IntoView {
    let (page, set_page) = signal(1i32);
    let (movies, set_movies) = signal(Vec::<MovieSummary>::new());
    let (total, set_total) = signal(0i64);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);

    Effect::new(move |_| {
        let p = page.get();
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::fetch_wildcards(p, PER_PAGE).await {
                Ok(resp) => {
                    set_movies.set(resp.data);
                    set_total.set(resp.total);
                    set_loading.set(false);
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_loading.set(false);
                }
            }
        });
    });

    let total_pages = move || ((total.get() as f64) / (PER_PAGE as f64)).ceil() as i32;

    view! {
        <div class="max-w-7xl mx-auto px-4 py-8">
            <div class="mb-8">
                <h1 class="text-4xl font-bold text-stone-100 mb-2">"Wildcards"</h1>
                <p class="text-stone-400 text-lg">"Films critics disagreed on — they score well algorithmically but have RT below 50%."</p>
                <p class="text-stone-500 text-sm mt-2">"Low vote counts here may reflect critical rejection rather than genuine undiscovery. Make of them what you will."</p>
            </div>
            {move || if loading.get() {
                view!{ <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                    {(0..10).map(|_| view!{ <SkeletonCard /> }).collect::<Vec<_>>()}
                </div> }.into_any()
            } else if let Some(err) = error.get() {
                view!{ <div class="py-16 text-center"><p class="text-red-400">{err}</p></div> }.into_any()
            } else if movies.get().is_empty() {
                view!{ <div class="py-16 text-center text-stone-500">"No wildcards yet — run scoring to populate this list."</div> }.into_any()
            } else {
                view!{ <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                    {move || movies.get().into_iter().map(|m| view!{ <MovieCard movie=m /> }).collect::<Vec<_>>()}
                </div> }.into_any()
            }}
            {move || {
                let tp = total_pages();
                if tp <= 1 { return view!{ <div /> }.into_any(); }
                let cur = page.get();
                let prev_disabled = page.get() <= 1;
                let next_disabled = page.get() >= tp;
                view!{
                    <div class="flex items-center justify-center gap-4 mt-10">
                        <button class="px-4 py-2 bg-[#201610] text-stone-200 rounded disabled:opacity-30 hover:bg-[#2b1e14]"
                            disabled=prev_disabled
                            on:click=move |_| set_page.update(|p| *p -= 1)>"← Prev"</button>
                        <span class="text-stone-400 text-sm">{format!("Page {} of {}", cur, tp)}</span>
                        <button class="px-4 py-2 bg-[#201610] text-stone-200 rounded disabled:opacity-30 hover:bg-[#2b1e14]"
                            disabled=next_disabled
                            on:click=move |_| set_page.update(|p| *p += 1)>"Next →"</button>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ── Movie detail page ────────────────────────────────────────────────────────
#[component]
pub fn MovieDetail() -> impl IntoView {
    let params = use_params_map();
    let (movie, set_movie) = signal(Option::<gem_finder_shared::types::Movie>::None);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);

    let movie_id = move || params.with(|p| p.get("id").and_then(|v| v.parse::<i64>().ok()));

    spawn_local(async move {
        match movie_id() {
            None => {
                set_error.set(Some("Invalid movie ID".into()));
                set_loading.set(false);
            }
            Some(id) => match api::fetch_movie(id).await {
                Ok(m) => {
                    set_movie.set(Some(m));
                    set_loading.set(false);
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_loading.set(false);
                }
            },
        }
    });

    view! {
        <div class="max-w-4xl mx-auto px-4 py-8">
            <A href="/" attr:class="text-orange-600 hover:text-orange-500 text-sm mb-6 inline-block">"← Back to Gems"</A>
            {move || if loading.get() {
                view!{ <div class="animate-pulse mt-6 space-y-4">
                    <div class="h-8 bg-[#2b1e14] rounded w-2/3" />
                    <div class="h-4 bg-[#2b1e14] rounded w-1/3" />
                    <div class="flex gap-8 mt-8">
                        <div class="w-48 h-72 bg-[#2b1e14] rounded flex-shrink-0" />
                        <div class="flex-1 space-y-3">
                            <div class="h-4 bg-[#2b1e14] rounded" />
                            <div class="h-4 bg-[#2b1e14] rounded w-5/6" />
                            <div class="h-4 bg-[#2b1e14] rounded w-4/6" />
                        </div>
                    </div>
                </div> }.into_any()
            } else if let Some(err) = error.get() {
                view!{ <div class="py-16 text-center"><p class="text-red-400 mb-4">{err}</p>
                    <A href="/" attr:class="text-orange-600">"← Back"</A></div> }.into_any()
            } else if let Some(m) = movie.get() {
                let poster   = m.poster_url.clone().unwrap_or_default();
                let title    = m.title.clone();
                let year     = m.year.map(|y| y.to_string()).unwrap_or_default();
                let director = m.director.clone().unwrap_or_else(|| "Unknown".into());
                let genre    = m.genre.clone().unwrap_or_default();
                let overview = m.overview.clone().unwrap_or_default();
                let imdb_str = m.imdb_rating.map(|r| format!("{:.1}", r));
                let rt_str   = m.rt_critic_score.map(|r| format!("{}%", r));
                let gem_str  = m.gem_score.map(|s| format!("{:.0}%", s * 100.0));
                let imdb_id  = m.imdb_id.clone();

                view!{
                    <div class="mt-6">
                        <h1 class="text-3xl font-bold text-stone-100 mb-1">{title.clone()}</h1>
                        <p class="text-stone-400 mb-6">{year} " · " {director}</p>
                        <div class="flex gap-8 flex-wrap">
                            <div class="flex-shrink-0">
                                {if poster.is_empty() {
                                    view!{ <div class="w-48 h-72 bg-[#201610] rounded flex items-center justify-center">
                                        <span class="text-5xl">"🎬"</span></div> }.into_any()
                                } else {
                                    view!{ <img src=poster alt=format!("{} poster", title) class="w-48 rounded shadow-xl" /> }.into_any()
                                }}
                            </div>
                            <div class="flex-1 min-w-0">
                                <div class="flex flex-wrap gap-3 mb-6">
                                    {gem_str.map(|s| view!{
                                        <div class="flex flex-col items-center bg-orange-950 border border-orange-800 rounded px-4 py-2">
                                            <span class="text-xs text-orange-600 uppercase tracking-wide">"GEM"</span>
                                            <span class="text-2xl font-bold text-orange-500">{s}</span>
                                        </div>
                                    })}
                                    {imdb_str.map(|r| view!{
                                        <div class="flex flex-col items-center bg-yellow-900 border border-yellow-700 rounded px-4 py-2">
                                            <span class="text-xs text-yellow-400 uppercase tracking-wide">"IMDb"</span>
                                            <span class="text-2xl font-bold text-yellow-300">{r}</span>
                                        </div>
                                    })}
                                    {rt_str.map(|r| view!{
                                        <div class="flex flex-col items-center bg-red-900 border border-red-700 rounded px-4 py-2">
                                            <span class="text-xs text-red-400 uppercase tracking-wide">"RT"</span>
                                            <span class="text-2xl font-bold text-red-300">{r}</span>
                                        </div>
                                    })}
                                </div>
                                {if !genre.is_empty() {
                                    let tags: Vec<_> = genre.split(", ").map(|g| {
                                        let g = g.to_string();
                                        view!{ <span class="px-2 py-1 bg-[#201610] border border-[#2b1e14] rounded text-xs text-stone-300">{g}</span> }
                                    }).collect();
                                    view!{ <div class="flex flex-wrap gap-2 mb-4">{tags}</div> }.into_any()
                                } else { view!{ <div /> }.into_any() }}
                                {if !overview.is_empty() {
                                    view!{ <p class="text-stone-300 leading-relaxed mb-6">{overview}</p> }.into_any()
                                } else { view!{ <div /> }.into_any() }}
                                {imdb_id.map(|id| view!{
                                    <a href={format!("https://www.imdb.com/title/{}", id)}
                                        target="_blank" rel="noopener noreferrer"
                                        class="text-sm text-yellow-400 hover:text-yellow-300 border border-yellow-700 rounded px-3 py-1">
                                        "View on IMDb →"</a>
                                })}
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else { view!{ <div /> }.into_any() }}
        </div>
    }
}

// ── Skeleton card (loading placeholder) ──────────────────────────────────────
#[component]
fn SkeletonCard() -> impl IntoView {
    view! {
        <div class="bg-[#201610] rounded overflow-hidden animate-pulse">
            <div class="bg-[#2b1e14]" style="aspect-ratio:2/3" />
            <div class="p-3 space-y-2">
                <div class="h-3 bg-[#2b1e14] rounded w-3/4" />
                <div class="h-3 bg-[#2b1e14] rounded w-1/2" />
            </div>
        </div>
    }
}

// ── Movie card ───────────────────────────────────────────────────────────────
#[component]
fn MovieCard(movie: MovieSummary) -> impl IntoView {
    let href = format!("/movie/{}", movie.id);
    let poster = movie.poster_url.clone().unwrap_or_default();
    let has_poster = !poster.is_empty();
    let gem_score = movie.gem_score.map(|s| format!("{:.0}%", s * 100.0));
    let year = movie.year.map(|y| y.to_string()).unwrap_or_default();
    let title = movie.title.clone();
    let director = movie.director.clone().unwrap_or_default();
    let imdb = movie.imdb_rating.map(|r| format!("{:.1}", r));
    let rt = movie.rt_critic_score.map(|r| format!("{}%", r));

    view! {
        <A href=href attr:class="group block bg-[#201610] rounded overflow-hidden hover:ring-1 hover:ring-orange-700 transition-all duration-200">
            <div class="relative overflow-hidden" style="aspect-ratio:2/3">
                {if has_poster {
                    view!{ <img src=poster alt=title.clone() loading="lazy"
                        class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-300" /> }.into_any()
                } else {
                    view!{ <div class="w-full h-full bg-[#2b1e14] flex items-center justify-center">
                        <span class="text-5xl">"🎬"</span></div> }.into_any()
                }}
                {gem_score.map(|s| view!{
                    <div class="absolute top-2 right-2 bg-orange-700 text-[#0d0906] text-xs font-bold px-1.5 py-0.5 rounded">
                        {s}</div>
                })}
            </div>
            <div class="p-3">
                <h3 class="text-stone-100 font-medium text-sm leading-snug line-clamp-2 mb-1">{title}</h3>
                <div class="flex items-center justify-between text-xs">
                    <span class="text-stone-400">{year}</span>
                    <div class="flex gap-2">
                        {imdb.map(|r| view!{ <span class="text-yellow-400">"★ "{r}</span> })}
                        {rt.map(|r| view!{ <span class="text-red-400">"🍅 "{r}</span> })}
                    </div>
                </div>
                {if !director.is_empty() {
                    view!{ <p class="text-xs text-stone-500 mt-1 truncate">{director}</p> }.into_any()
                } else { view!{ <span /> }.into_any() }}
            </div>
        </A>
    }
}

// ── Admin page ───────────────────────────────────────────────────────────────
#[derive(Clone, PartialEq)]
enum ActionState {
    Idle,
    Running,
    Done(String),
    Failed(String),
}

#[component]
pub fn AdminPage() -> impl IntoView {
    let (seed_state, set_seed_state) = signal(ActionState::Idle);
    let (sync_state, set_sync_state) = signal(ActionState::Idle);
    let (enrich_state, set_enrich_state) = signal(ActionState::Idle);
    let (score_state, set_score_state) = signal(ActionState::Idle);
    let (enrich_limit, set_enrich_limit) = signal(10_000i64);
    let (logs, set_logs) = signal(Vec::<serde_json::Value>::new());
    let (logs_loading, set_logs_loading) = signal(false);

    // Fetch logs — called on mount and on the polling interval
    let fetch_logs = move || {
        set_logs_loading.set(true);
        spawn_local(async move {
            if let Ok(resp) = api::admin_logs().await {
                if let Some(arr) = resp.get("logs").and_then(|v| v.as_array()) {
                    set_logs.set(arr.clone());
                }
            }
            set_logs_loading.set(false);
        });
    };

    // Initial load
    Effect::new(move |_| { fetch_logs(); });


    let run_seed = move |_| {
        set_seed_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_seed().await {
                Ok(_) => set_seed_state.set(ActionState::Done("Started — watch logs below".into())),
                Err(e) => set_seed_state.set(ActionState::Failed(e)),
            }
        });
    };

    let run_sync = move |_| {
        set_sync_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_sync().await {
                Ok(_) => set_sync_state.set(ActionState::Done("Started — watch logs below".into())),
                Err(e) => set_sync_state.set(ActionState::Failed(e)),
            }
        });
    };

    let run_enrich = move |_| {
        let limit = enrich_limit.get();
        set_enrich_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_enrich(limit).await {
                Ok(_) => set_enrich_state.set(ActionState::Done("Started — watch logs below".into())),
                Err(e) => set_enrich_state.set(ActionState::Failed(e)),
            }
        });
    };

    let run_score = move |_| {
        set_score_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_score().await {
                Ok(_) => set_score_state.set(ActionState::Done("Started — watch logs below".into())),
                Err(e) => set_score_state.set(ActionState::Failed(e)),
            }
        });
    };

    view! {
        <div class="max-w-3xl mx-auto px-4 py-8">
            <h1 class="text-3xl font-bold text-stone-100 mb-2">"Admin"</h1>
            <p class="text-stone-400 mb-8">"Operations run on the server — you can close this page. Check logs below for progress."</p>

            // Full pipeline — seed test data
            <div class="mb-6 p-4 bg-[#17100a] rounded border border-orange-800">
                <div class="flex items-start justify-between gap-4">
                    <div class="flex-1 min-w-0">
                        <p class="text-orange-500 font-semibold text-sm">"Seed Test Data"</p>
                        <p class="text-xs text-stone-400 mt-0.5">"Runs the full pipeline: sync all eras + blockbusters, enrich via OMDb (2000 limit), score, classify acclaimed. Use on a fresh database. Takes 20–60+ min."</p>
                    </div>
                    <div class="flex items-center gap-3 flex-shrink-0">
                        <span class="text-xs max-w-xs truncate"
                            class:text-stone-500={move || seed_state.get() == ActionState::Idle}
                            class:text-yellow-400={move || seed_state.get() == ActionState::Running}
                            class:animate-pulse={move || seed_state.get() == ActionState::Running}
                            class:text-orange-600={move || matches!(seed_state.get(), ActionState::Done(_))}
                            class:text-red-400={move || matches!(seed_state.get(), ActionState::Failed(_))}>
                            {move || match seed_state.get() {
                                ActionState::Idle       => String::new(),
                                ActionState::Running    => "Running…".to_string(),
                                ActionState::Done(msg)  => msg,
                                ActionState::Failed(e)  => format!("✗ {}", e),
                            