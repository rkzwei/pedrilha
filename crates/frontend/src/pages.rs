use crate::api;
use gem_finder_shared::types::MovieSummary;
use leptos::prelude::*;
use leptos::task::spawn_local;
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
                <h1 class="text-4xl font-bold text-white mb-2">"Hidden Gems"</h1>
                <p class="text-gray-400 text-lg">"Movies rated 6.5–7.9 on IMDb that deserve your attention."</p>
            </div>

            <div class="flex flex-wrap gap-4 mb-6 p-4 bg-gray-900 rounded-lg border border-gray-800">
                <div class="flex flex-col gap-1">
                    <label class="text-xs text-gray-500 uppercase tracking-wide">"Era"</label>
                    <select class="bg-gray-800 text-white border border-gray-700 rounded px-3 py-2 text-sm"
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
                    <label class="text-xs text-gray-500 uppercase tracking-wide">"Genre"</label>
                    <select class="bg-gray-800 text-white border border-gray-700 rounded px-3 py-2 text-sm"
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
                    <span class="text-sm text-gray-500">
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
                view!{ <div class="py-16 text-center text-gray-400">"No gems match your filters."</div> }.into_any()
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
                        <button class="px-4 py-2 bg-gray-800 text-white rounded disabled:opacity-30 hover:bg-gray-700"
                            disabled=prev_disabled
                            on:click=move |_| set_page.update(|p| *p -= 1)>"← Prev"</button>
                        <span class="text-gray-400 text-sm">{format!("Page {} of {}", cur, tp)}</span>
                        <button class="px-4 py-2 bg-gray-800 text-white rounded disabled:opacity-30 hover:bg-gray-700"
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
                <h1 class="text-4xl font-bold text-white mb-2">"Acclaimed"</h1>
                <p class="text-gray-400 text-lg">"IMDb 8.0+ and RT 80%+ — films everyone should see."</p>
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
                        <button class="px-4 py-2 bg-gray-800 text-white rounded disabled:opacity-30 hover:bg-gray-700"
                            disabled=prev_disabled
                            on:click=move |_| set_page.update(|p| *p -= 1)>"← Prev"</button>
                        <span class="text-gray-400 text-sm">{format!("Page {} of {}", cur, tp)}</span>
                        <button class="px-4 py-2 bg-gray-800 text-white rounded disabled:opacity-30 hover:bg-gray-700"
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
            <A href="/" attr:class="text-emerald-400 hover:text-emerald-300 text-sm mb-6 inline-block">"← Back to Gems"</A>
            {move || if loading.get() {
                view!{ <div class="animate-pulse mt-6 space-y-4">
                    <div class="h-8 bg-gray-800 rounded w-2/3" />
                    <div class="h-4 bg-gray-800 rounded w-1/3" />
                    <div class="flex gap-8 mt-8">
                        <div class="w-48 h-72 bg-gray-800 rounded flex-shrink-0" />
                        <div class="flex-1 space-y-3">
                            <div class="h-4 bg-gray-800 rounded" />
                            <div class="h-4 bg-gray-800 rounded w-5/6" />
                            <div class="h-4 bg-gray-800 rounded w-4/6" />
                        </div>
                    </div>
                </div> }.into_any()
            } else if let Some(err) = error.get() {
                view!{ <div class="py-16 text-center"><p class="text-red-400 mb-4">{err}</p>
                    <A href="/" attr:class="text-emerald-400">"← Back"</A></div> }.into_any()
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
                        <h1 class="text-3xl font-bold text-white mb-1">{title.clone()}</h1>
                        <p class="text-gray-400 mb-6">{year} " · " {director}</p>
                        <div class="flex gap-8 flex-wrap">
                            <div class="flex-shrink-0">
                                {if poster.is_empty() {
                                    view!{ <div class="w-48 h-72 bg-gray-800 rounded-lg flex items-center justify-center">
                                        <span class="text-5xl">"🎬"</span></div> }.into_any()
                                } else {
                                    view!{ <img src=poster alt=format!("{} poster", title) class="w-48 rounded-lg shadow-xl" /> }.into_any()
                                }}
                            </div>
                            <div class="flex-1 min-w-0">
                                <div class="flex flex-wrap gap-3 mb-6">
                                    {gem_str.map(|s| view!{
                                        <div class="flex flex-col items-center bg-emerald-900 border border-emerald-700 rounded-lg px-4 py-2">
                                            <span class="text-xs text-emerald-400 uppercase tracking-wide">"GEM"</span>
                                            <span class="text-2xl font-bold text-emerald-300">{s}</span>
                                        </div>
                                    })}
                                    {imdb_str.map(|r| view!{
                                        <div class="flex flex-col items-center bg-yellow-900 border border-yellow-700 rounded-lg px-4 py-2">
                                            <span class="text-xs text-yellow-400 uppercase tracking-wide">"IMDb"</span>
                                            <span class="text-2xl font-bold text-yellow-300">{r}</span>
                                        </div>
                                    })}
                                    {rt_str.map(|r| view!{
                                        <div class="flex flex-col items-center bg-red-900 border border-red-700 rounded-lg px-4 py-2">
                                            <span class="text-xs text-red-400 uppercase tracking-wide">"RT"</span>
                                            <span class="text-2xl font-bold text-red-300">{r}</span>
                                        </div>
                                    })}
                                </div>
                                {if !genre.is_empty() {
                                    let tags: Vec<_> = genre.split(", ").map(|g| {
                                        let g = g.to_string();
                                        view!{ <span class="px-2 py-1 bg-gray-800 border border-gray-700 rounded text-xs text-gray-300">{g}</span> }
                                    }).collect();
                                    view!{ <div class="flex flex-wrap gap-2 mb-4">{tags}</div> }.into_any()
                                } else { view!{ <div /> }.into_any() }}
                                {if !overview.is_empty() {
                                    view!{ <p class="text-gray-300 leading-relaxed mb-6">{overview}</p> }.into_any()
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
        <A href=href attr:class="group block bg-gray-800 rounded-lg overflow-hidden shadow-lg hover:shadow-xl hover:ring-1 hover:ring-emerald-700 transition-all duration-200">
            <div class="relative overflow-hidden" style="aspect-ratio:2/3">
                {if has_poster {
                    view!{ <img src=poster alt=title.clone() loading="lazy"
                        class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-300" /> }.into_any()
                } else {
                    view!{ <div class="w-full h-full bg-gray-700 flex items-center justify-center">
                        <span class="text-5xl">"🎬"</span></div> }.into_any()
                }}
                {gem_score.map(|s| view!{
                    <div class="absolute top-2 right-2 bg-emerald-600 text-white text-xs font-bold px-1.5 py-0.5 rounded">
                        {s}</div>
                })}
            </div>
            <div class="p-3">
                <h3 class="text-white font-medium text-sm leading-snug line-clamp-2 mb-1">{title}</h3>
                <div class="flex items-center justify-between text-xs">
                    <span class="text-gray-400">{year}</span>
                    <div class="flex gap-2">
                        {imdb.map(|r| view!{ <span class="text-yellow-400">"★ "{r}</span> })}
                        {rt.map(|r| view!{ <span class="text-red-400">"🍅 "{r}</span> })}
                    </div>
                </div>
                {if !director.is_empty() {
                    view!{ <p class="text-xs text-gray-500 mt-1 truncate">{director}</p> }.into_any()
                } else { view!{ <span /> }.into_any() }}
            </div>
       
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
    let (sync_state, set_sync_state) = signal(ActionState::Idle);
    let (enrich_state, set_enrich_state) = signal(ActionState::Idle);
    let (score_state, set_score_state) = signal(ActionState::Idle);
    let (logs, set_logs) = signal(Vec::<serde_json::Value>::new());
    let (logs_loading, set_logs_loading) = signal(false);

    Effect::new(move |_| {
        set_logs_loading.set(true);
        spawn_local(async move {
            if let Ok(resp) = api::admin_logs().await {
                if let Some(arr) = resp.get("logs").and_then(|v| v.as_array()) {
                    set_logs.set(arr.clone());
                }
            }
            set_logs_loading.set(false);
        });
    });

    let run_sync = move |_| {
        set_sync_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_sync().await {
                Ok(v) => {
                    let msg = format!(
                        "✓ {} era windows, {} blockbusters, {}/{} gems",
                        v.get("era_windows").and_then(|x| x.as_i64()).unwrap_or(0),
                        v.get("blockbusters_synced").and_then(|x| x.as_i64()).unwrap_or(0),
                        v.get("known_gems_seeded").and_then(|x| x.as_i64()).unwrap_or(0),
                        v.get("known_gems_total").and_then(|x| x.as_i64()).unwrap_or(0),
                    );
                    set_sync_state.set(ActionState::Done(msg));
                }
                Err(e) => set_sync_state.set(ActionState::Failed(e)),
            }
        });
    };

    let run_enrich = move |_| {
        set_enrich_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_enrich(500).await {
                Ok(v) => {
                    let msg = format!(
                        "✓ {}/{} enriched, {} errors",
                        v.get("enriched").and_then(|x| x.as_i64()).unwrap_or(0),
                        v.get("total_candidates").and_then(|x| x.as_i64()).unwrap_or(0),
                        v.get("errors_count").and_then(|x| x.as_i64()).unwrap_or(0),
                    );
                    set_enrich_state.set(ActionState::Done(msg));
                }
                Err(e) => set_enrich_state.set(ActionState::Failed(e)),
            }
        });
    };

    let run_score = move |_| {
        set_score_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_score().await {
                Ok(v) => {
                    let msg = format!(
                        "✓ {} movies in {:.1}s",
                        v.get("movies_scored").and_then(|x| x.as_i64()).unwrap_or(0),
                        v.get("duration_secs").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    );
                    set_score_state.set(ActionState::Done(msg));
                }
                Err(e) => set_score_state.set(ActionState::Failed(e)),
            }
        });
    };

    view! {
        <div class="max-w-3xl mx-auto px-4 py-8">
            <h1 class="text-3xl font-bold text-white mb-2">"Admin"</h1>
            <p class="text-gray-400 mb-8">"Manage sync, enrichment, and scoring pipelines."</p>

            <div class="space-y-4 mb-10">
                <AdminAction
                    label="Sync Movies"
                    description="Fetch hidden gem candidates + blockbusters from TMDB (all eras)"
                    button_label="Run Sync"
                    button_class="bg-emerald-700 hover:bg-emerald-600"
                    state=sync_state
                    on_click=run_sync
                />
                <AdminAction
                    label="Enrich (OMDb)"
                    description="Fetch IMDb ratings + Rotten Tomatoes scores (500 movies)"
                    button_label="Run Enrich"
                    button_class="bg-blue-700 hover:bg-blue-600"
                    state=enrich_state
                    on_click=run_enrich
                />
                <AdminAction
                    label="Score Gems"
                    description="Recompute gem scores for all movies in the DB"
                    button_label="Run Score"
                    button_class="bg-purple-700 hover:bg-purple-600"
                    state=score_state
                    on_click=run_score
                />
            </div>

            <h2 class="text-xl font-semibold text-white mb-3">"Recent Logs"</h2>
            {move || if logs_loading.get() {
                view!{ <p class="text-gray-500 text-sm animate-pulse">"Loading logs…"</p> }.into_any()
            } else if logs.get().is_empty() {
                view!{ <p class="text-gray-500 text-sm">"No logs yet."</p> }.into_any()
            } else {
                let rows = logs.get().into_iter().map(|entry| {
                    let level = entry.get("level").and_then(|v| v.as_str()).unwrap_or("info").to_string();
                    let event = entry.get("event_type").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let msg   = entry.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let ts    = entry.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let level_class = match level.as_str() {
                        "error" => "text-red-400",
                        "warn"  => "text-yellow-400",
                        _       => "text-emerald-400",
                    }.to_string();
                    view!{
                        <tr class="border-t border-gray-800 text-xs font-mono">
                            <td class={format!("py-1.5 pr-3 {}", level_class)}>{level}</td>
                            <td class="py-1.5 pr-3 text-gray-400">{event}</td>
                            <td class="py-1.5 pr-3 text-gray-300 max-w-xs truncate">{msg}</td>
                            <td class="py-1.5 text-gray-600 whitespace-nowrap">{ts}</td>
                        </tr>
                    }
                }).collect::<Vec<_>>();
                view!{
                    <div class="overflow-x-auto">
                        <table class="w-full">
                            <thead>
                                <tr class="text-xs text-gray-600 uppercase">
                                    <th class="pb-2 text-left">"Level"</th>
                                    <th class="pb-2 text-left">"Event"</th>
                                    <th class="pb-2 text-left">"Message"</th>
                                    <th class="pb-2 text-left">"Time"</th>
                                </tr>
                            </thead>
                            <tbody>{rows}</tbody>
                        </table>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn AdminAction(
    label: &'static str,
    description: &'static str,
    button_label: &'static str,
    button_class: &'static str,
    #[prop(into)] state: Signal<ActionState>,
    on_click: impl Fn(web_sys::MouseEvent) + 'static,
) -> impl IntoView {
    view! {
        <div class="flex items-center gap-4 p-4 bg-gray-900 rounded-lg border border-gray-800">
            <div class="flex-1 min-w-0">
                <p class="text-white font-medium text-sm">{label}</p>
                <p class="text-xs text-gray-500 mt-0.5">{description}</p>
            </div>
            <div class="flex items-center gap-3 flex-shrink-0">
                <span class="text-xs max-w-xs truncate" class:text-gray-400={move || state.get() == ActionState::Idle}
                    class:text-yellow-400={move || state.get() == ActionState::Running}
                    class:animate-pulse={move || state.get() == ActionState::Running}
                    class:text-emerald-400={move || matches!(state.get(), ActionState::Done(_))}
                    class:text-red-400={move || matches!(state.get(), ActionState::Failed(_))}>
                    {move || match state.get() {
                        ActionState::Idle       => String::new(),
                        ActionState::Running    => "Running…".to_string(),
                        ActionState::Done(msg)  => msg,
                        ActionState::Failed(e)  => format!("✗ {}", e),
                    }}
                </span>
                <button
                    class={format!("px-4 py-2 {} text-white rounded text-sm disabled:opacity-40 transition-colors", button_class)}
                    disabled=move || state.get() == ActionState::Running
                    on:click=on_click>
                    {button_label}
                </button>
            </div>
        </div>
    }
}
