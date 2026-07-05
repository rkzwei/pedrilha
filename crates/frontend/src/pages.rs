use crate::api;
use crate::i18n::{dict, genre_label, sort_label, use_lang};
use crate::{jwt_is_admin, save_auth_to_storage, use_watch, AuthState, WatchPrefs};
use gem_finder_shared::id_encode::encode_movie_id;
use gem_finder_shared::types::{Movie, MovieSummary, ProviderInfo, WatchState, WatchTier};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::{
    components::A,
    hooks::{use_location, use_navigate, use_params_map, use_query_map},
    NavigateOptions,
};
use wasm_bindgen::prelude::*;

const GENRES: &[&str] = &[
    "Action",
    "Animation",
    "Comedy",
    "Crime",
    "Drama",
    "Horror",
    "Romance",
    "Science Fiction",
    "Thriller",
    "Documentary",
    "Western",
    "War",
    "Musical",
];
const SORT_OPTIONS: &[(&str, &str)] = &[
    ("score", "Score"),
    ("rating", "Rating"),
    ("rt", "Critics"),
    ("year", "Year"),
    ("title", "Title"),
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

/// Thousands separator for film counts (3503 → "3,503").
fn fmt_thousands(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{}", out)
    } else {
        out
    }
}

/// "1994-03-10" → "10 Mar" (year is shown separately).
fn fmt_release_day(d: &str) -> Option<String> {
    let mut it = d.split('-');
    let _year = it.next()?;
    let month: usize = it.next()?.parse().ok()?;
    let day: u32 = it.next()?.parse().ok()?;
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    if !(1..=12).contains(&month) {
        return None;
    }
    Some(format!("{} {}", day, MONTHS[month - 1]))
}

/// Percent-encode a URL query component.
fn urlenc(s: &str) -> String {
    String::from(js_sys::encode_uri_component(s))
}

// ── URL builder ───────────────────────────────────────────────────────────────
fn build_url(
    path: &str,
    page: i32,
    genres: &Option<String>,
    year: &Option<i32>,
    q: &Option<String>,
    sort: &str,
    sort_dir: &str,
) -> String {
    let mut url = format!("{}?page={}", path, page);
    if let Some(g) = genres {
        url.push_str(&format!("&genres={}", g));
    }
    if let Some(y) = year {
        url.push_str(&format!("&year={}", y));
    }
    if let Some(s) = q {
        url.push_str(&format!("&q={}", s));
    }
    url.push_str(&format!("&sort={}&sort_dir={}", sort, sort_dir));
    url
}

// ── Client-side email validation ──────────────────────────────────────────────
fn is_valid_email_client(email: &str) -> bool {
    let Some(at) = email.find('@') else {
        return false;
    };
    if at == 0 {
        return false;
    }
    let domain = &email[at + 1..];
    let Some(dot) = domain.rfind('.') else {
        return false;
    };
    dot != 0 && dot != domain.len() - 1 && (domain.len() - dot) >= 3
}

// ── Filter bar ────────────────────────────────────────────────────────────────
// open_dd: 0 = none, 1 = genre panel, 2 = era panel, 3 = sort panel
#[component]
fn FilterBar(
    genres: Signal<Vec<String>>,
    year: Signal<Option<i32>>,
    search: Signal<Option<String>>,
    total: Signal<i64>,
    on_genres: Callback<Vec<String>>,
    on_year: Callback<Option<i32>>,
    on_search: Callback<Option<String>>,
    sort: Signal<String>,
    sort_dir: Signal<String>,
    on_sort: Callback<(String, String)>,
) -> impl IntoView {
    let lang = use_lang();
    let d = move || dict(lang.get());
    let (open_dd, set_open_dd) = signal(0u8);
    let active_count = move || genres.get().len() + year.get().map(|_| 1).unwrap_or(0);

    // Local signal so the input feels instant; on_search is debounced 300ms.
    let (local_search, set_local_search) = signal(search.get_untracked().unwrap_or_default());
    let debounce_handle: StoredValue<Option<i32>> = StoredValue::new(None);
    let search_ref: NodeRef<leptos::html::Input> = NodeRef::new();

    // Escape closes any open panel; "/" focuses search (unless already typing).
    let key_handle =
        window_event_listener(leptos::ev::keydown, move |ev| match ev.key().as_str() {
            "Escape" => set_open_dd.set(0),
            "/" => {
                let in_field = ev
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                    .map(|e| matches!(e.tag_name().as_str(), "INPUT" | "TEXTAREA"))
                    .unwrap_or(false);
                if !in_field {
                    ev.prevent_default();
                    if let Some(inp) = search_ref.get_untracked() {
                        let _ = inp.focus();
                    }
                }
            }
            _ => {}
        });
    on_cleanup(move || key_handle.remove());

    // Sync the box when the URL's q changes externally (Clear filters, back/forward)
    // — but never while the user is typing in it.
    Effect::new(move |_| {
        let s = search.get().unwrap_or_default();
        let focused = search_ref
            .get_untracked()
            .map(|inp| {
                let node: &web_sys::Node = inp.as_ref();
                web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.active_element())
                    .map(|a| a.is_same_node(Some(node)))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
        if !focused && s != local_search.get_untracked() {
            set_local_search.set(s);
        }
    });

    // Button class helpers
    let dd_btn = |is_open: bool, is_active: bool| -> String {
        let base = "flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md border transition-colors cursor-pointer";
        if is_open || is_active {
            format!("{} border-sc-accent text-sc-accent bg-sc-accent-deep", base)
        } else {
            format!("{} border-sc-border text-stone-400 bg-sc-card hover:border-stone-600 hover:text-stone-200", base)
        }
    };
    let opt_btn = |active: bool| -> &'static str {
        if active {
            "px-2 py-1 text-xs rounded border border-sc-accent text-sc-accent bg-sc-accent-deep transition-colors cursor-pointer font-medium"
        } else {
            "px-2 py-1 text-xs rounded border border-sc-border text-stone-400 bg-sc-card hover:border-stone-600 hover:text-stone-200 transition-colors cursor-pointer"
        }
    };

    view! {
        <div class="relative mb-6 space-y-2">

            // ── Click-away overlay ────────────────────────────────────────────
            {move || (open_dd.get() != 0).then(|| view! {
                <div
                    style="position:fixed;inset:0;z-index:40"
                    on:click=move |_| set_open_dd.set(0)
                />
            })}

            // ── Search ────────────────────────────────────────────────────────
            <div class="relative">
                <input
                    type="text"
                    placeholder=move || d().filter_search_placeholder
                    node_ref=search_ref
                    class="w-full bg-sc-card text-stone-200 border border-sc-border-input rounded-md px-4 py-2.5 text-sm placeholder-stone-600 focus:outline-none focus:border-sc-accent-border"
                    prop:value=move || local_search.get()
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        set_local_search.set(v.clone());
                        // Cancel any pending debounce timer.
                        if let Some(h) = debounce_handle.get_value() {
                            if let Some(w) = web_sys::window() {
                                w.clear_timeout_with_handle(h);
                            }
                        }
                        // Schedule search 300 ms after the user stops typing.
                        let val = if v.is_empty() { None } else { Some(v) };
                        let cb = Closure::once(move || { on_search.run(val); });
                        let handle = web_sys::window()
                            .and_then(|w| {
                                w.set_timeout_with_callback_and_timeout_and_arguments_0(
                                    cb.as_ref().unchecked_ref::<js_sys::Function>(),
                                    300,
                                ).ok()
                            })
                            .unwrap_or(-1);
                        cb.forget();
                        debounce_handle.set_value(Some(handle));
                    }
                />
                {move || (!local_search.get().is_empty()).then(|| view! {
                    <button
                        class="absolute right-3 top-1/2 -translate-y-1/2 text-stone-500 hover:text-stone-200 text-sm leading-none px-1"
                        aria-label=move || d().filter_clear_search
                        on:click=move |_| {
                            // Cancel pending debounce, clear instantly.
                            if let Some(h) = debounce_handle.get_value() {
                                if let Some(w) = web_sys::window() {
                                    w.clear_timeout_with_handle(h);
                                }
                            }
                            set_local_search.set(String::new());
                            on_search.run(None);
                        }
                    >"✕"</button>
                })}
            </div>

            // ── Filter buttons row ────────────────────────────────────────────
            <div class="flex items-center gap-2" style="position:relative;z-index:50">

                // ── Genre dropdown ────────────────────────────────────────────
                <div class="relative">
                    <button
                        class=move || dd_btn(open_dd.get() == 1, !genres.get().is_empty())
                        aria-haspopup="true"
                        aria-expanded=move || if open_dd.get() == 1 { "true" } else { "false" }
                        on:click=move |_| set_open_dd.update(|v| *v = if *v == 1 { 0 } else { 1 })
                    >
                        // Closed-state label shows what's selected: names for 1–3, count for more.
                        <span class="truncate max-w-[200px]">
                            {move || {
                                let g = genres.get();
                                let l = lang.get();
                                match g.len() {
                                    0 => d().filter_genre.to_string(),
                                    1..=3 => g.iter()
                                        .map(|x| genre_label(l, x))
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                    n => d().filter_genre_count.replace("{}", &n.to_string()),
                                }
                            }}
                        </span>
                        <span class="text-stone-600 text-xs">"▾"</span>
                    </button>

                    {move || (open_dd.get() == 1).then(|| view! {
                        <div style="position:absolute;top:calc(100% + 6px);left:0;min-width:260px;background-color:var(--sc-panel,#17100a);border:1px solid var(--sc-border);border-radius:10px;box-shadow:0 24px 48px rgba(0,0,0,0.7);padding:12px;z-index:200">
                            <p style="font-size:0.65rem;text-transform:uppercase;letter-spacing:0.08em;color:var(--sc-accent-border);margin-bottom:8px;font-weight:600">{move || d().filter_genre}</p>
                            <div class="grid grid-cols-2 gap-1">
                                {GENRES.iter().map(|g| {
                                    let gs = g.to_string();
                                    let gs2 = gs.clone();
                                    let gv = *g;
                                    view! {
                                        <button
                                            class=move || opt_btn(genres.get().contains(&gs))
                                            on:click=move |_| {
                                                let mut cur = genres.get_untracked();
                                                if let Some(pos) = cur.iter().position(|x| x == &gs2) {
                                                    cur.remove(pos);
                                                } else {
                                                    cur.push(gs2.clone());
                                                }
                                                on_genres.run(cur);
                                            }
                                        >{move || genre_label(lang.get(), gv)}</button>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })}
                </div>

                // ── Era dropdown ──────────────────────────────────────────────
                <div class="relative">
                    <button
                        class=move || dd_btn(open_dd.get() == 2, year.get().is_some())
                        aria-haspopup="true"
                        aria-expanded=move || if open_dd.get() == 2 { "true" } else { "false" }
                        on:click=move |_| set_open_dd.update(|v| *v = if *v == 2 { 0 } else { 2 })
                    >
                        {move || year.get()
                            .and_then(|y| DECADE_OPTIONS.iter().find(|(v,_)| *v == y).map(|(_,l)| *l))
                            .unwrap_or(d().filter_era)}
                        <span class="text-stone-600 text-xs">"▾"</span>
                    </button>

                    {move || (open_dd.get() == 2).then(|| view! {
                        <div style="position:absolute;top:calc(100% + 6px);left:0;min-width:160px;background-color:var(--sc-panel,#17100a);border:1px solid var(--sc-border);border-radius:10px;box-shadow:0 24px 48px rgba(0,0,0,0.7);padding:12px;z-index:200">
                            <p style="font-size:0.65rem;text-transform:uppercase;letter-spacing:0.08em;color:var(--sc-accent-border);margin-bottom:8px;font-weight:600">{move || d().filter_from_era}</p>
                            <div class="flex flex-col gap-1">
                                {DECADE_OPTIONS.iter().map(|(y, l)| {
                                    let yv = *y;
                                    view! {
                                        <button
                                            class=move || opt_btn(year.get() == Some(yv))
                                            on:click=move |_| {
                                                if year.get_untracked() == Some(yv) {
                                                    on_year.run(None);
                                                } else {
                                                    on_year.run(Some(yv));
                                                    set_open_dd.set(0);
                                                }
                                            }
                                        >{*l}</button>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        </div>
                    })}
                </div>

                // ── Sort dropdown ─────────────────────────────────────────────
                <div class="relative">
                    <button
                        class=move || dd_btn(open_dd.get() == 3, false)
                        aria-haspopup="true"
                        aria-expanded=move || if open_dd.get() == 3 { "true" } else { "false" }
                        on:click=move |_| set_open_dd.update(|v| *v = if *v == 3 { 0 } else { 3 })
                    >
                        {move || {
                            let label = sort_label(lang.get(), &sort.get());
                            let dir = if sort_dir.get() == "asc" { "↑" } else { "↓" };
                            format!("{} {}", label, dir)
                        }}
                        <span class="text-stone-600 text-xs">"▾"</span>
                    </button>

                    {move || (open_dd.get() == 3).then(|| {
                        let cur_sort = sort.get();
                        let cur_dir = sort_dir.get();
                        view! {
                            <div style="position:absolute;top:calc(100% + 6px);left:0;min-width:150px;background-color:var(--sc-panel,#17100a);border:1px solid var(--sc-border);border-radius:10px;box-shadow:0 24px 48px rgba(0,0,0,0.7);padding:12px;z-index:200">
                                <p style="font-size:0.65rem;text-transform:uppercase;letter-spacing:0.08em;color:var(--sc-accent-border);margin-bottom:8px;font-weight:600">{move || d().filter_sort_by}</p>
                                <div class="flex flex-col gap-1 mb-3">
                                    {SORT_OPTIONS.iter().map(|(val, _label)| {
                                        let v = val.to_string();
                                        let v2 = v.clone();
                                        let vlabel = *val;
                                        let cd = cur_dir.clone();
                                        let cs = cur_sort.clone();
                                        view! {
                                            <button
                                                class=move || opt_btn(cs == v)
                                                on:click=move |_| {
                                                    on_sort.run((v2.clone(), cd.clone()));
                                                    set_open_dd.set(0);
                                                }
                                            >{move || sort_label(lang.get(), vlabel)}</button>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                                <p style="font-size:0.65rem;text-transform:uppercase;letter-spacing:0.08em;color:var(--sc-accent-border);margin-bottom:6px;font-weight:600">{move || d().filter_direction}</p>
                                <div class="flex gap-1">
                                    {
                                        let cs2 = cur_sort.clone();
                                        let cd2 = cur_dir.clone();
                                        view! {
                                            <button
                                                class=move || opt_btn(cd2 == "desc")
                                                on:click=move |_| {
                                                    on_sort.run((cs2.clone(), "desc".to_string()));
                                                    set_open_dd.set(0);
                                                }
                                            >{move || d().filter_desc}</button>
                                        }
                                    }
                                    {
                                        let cs3 = cur_sort.clone();
                                        let cd3 = cur_dir.clone();
                                        view! {
                                            <button
                                                class=move || opt_btn(cd3 == "asc")
                                                on:click=move |_| {
                                                    on_sort.run((cs3.clone(), "asc".to_string()));
                                                    set_open_dd.set(0);
                                                }
                                            >{move || d().filter_asc}</button>
                                        }
                                    }
                                </div>
                            </div>
                        }
                    })}
                </div>

                // ── Film count (always visible) + clear ───────────────────────
                {move || {
                    let t = total.get();
                    let filtered = active_count() > 0 || search.get().is_some();
                    let label = if t == 0 && filtered {
                        d().filter_no_matches.to_string()
                    } else {
                        d().filter_films_count.replace("{}", &fmt_thousands(t))
                    };
                    view! {
                        <span class="ml-auto flex items-center gap-3">
                            <span class="text-xs text-stone-400 tabular-nums">{label}</span>
                            {(active_count() > 0).then(|| view! {
                                <button
                                    class="text-xs text-stone-500 hover:text-stone-300 transition-colors"
                                    on:click=move |_| { on_genres.run(vec![]); on_year.run(None); }
                                >{move || d().filter_clear}</button>
                            })}
                        </span>
                    }
                }}

            </div>
        </div>
    }
}

// ── Pagination bar ────────────────────────────────────────────────────────────
#[component]
fn PaginationBar(
    page: i32,
    total_pages: i32,
    on_prev: Callback<()>,
    on_next: Callback<()>,
    on_first: Callback<()>,
    on_last: Callback<()>,
    on_page: Callback<i32>,
) -> impl IntoView {
    let lang = use_lang();
    let d = move || dict(lang.get());
    let btn = "px-3 py-2 bg-sc-card text-stone-200 rounded disabled:opacity-30 hover:bg-sc-border text-sm";
    let at_start = page <= 1;
    let at_end = page == total_pages || total_pages < 1;
    let wrap = if total_pages > 1 {
        "flex items-center justify-center gap-2 mt-10 flex-wrap"
    } else {
        "hidden"
    };
    view! {
        <div class=wrap>
            <button class=btn prop:disabled=at_start
                on:click=move |_| on_first.run(())>"«"</button>
            <button class=btn prop:disabled=at_start
                on:click=move |_| on_prev.run(())>{move || d().page_prev}</button>
            <span class="flex items-center gap-1 text-stone-400 text-sm">
                <span>{move || d().page_label}</span>
                <input
                    type="number"
                    min="1"
                    max=total_pages
                    prop:value=page
                    class="w-12 px-1 py-1 bg-sc-card text-stone-200 rounded text-center text-sm border border-sc-border focus:outline-none focus:border-sc-accent [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none"
                    on:change=move |e| {
                        let v = event_target_value(&e)
                            .parse::<i32>()
                            .unwrap_or(page)
                            .clamp(1, total_pages);
                        on_page.run(v);
                    }
                />
                <span>{format!("/ {}", total_pages)}</span>
            </span>
            <button class=btn prop:disabled=at_end
                on:click=move |_| on_next.run(())>{move || d().page_next}</button>
            <button class=btn prop:disabled=at_end
                on:click=move |_| on_last.run(())>"»"</button>
        </div>
    }
}

// ── Home page ─────────────────────────────────────────────────────────────────
#[component]
pub fn HomePage() -> impl IntoView {
    let query = use_query_map();
    let navigate = use_navigate();
    let watch = use_watch();

    let page = move || query.with(|q| q.get("page").and_then(|v| v.parse().ok()).unwrap_or(1i32));
    let year = move || query.with(|q| q.get("year").and_then(|v| v.parse().ok()));
    let genres = move || {
        query.with(|q| {
            q.get("genres")
                .map(|v| {
                    v.split(',')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    };
    let search = move || query.with(|q| q.get("q").filter(|v| !v.is_empty()));
    let sort = move || query.with(|q| q.get("sort").unwrap_or_else(|| "score".to_string()));
    let sort_dir = move || query.with(|q| q.get("sort_dir").unwrap_or_else(|| "desc".to_string()));
    let gstr = move || {
        let g = genres();
        if g.is_empty() {
            None
        } else {
            Some(g.join(","))
        }
    };

    let (movies, set_movies) = signal(Vec::<MovieSummary>::new());
    let (total, set_total) = signal(0i64);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);
    let (retry, set_retry) = signal(0u32);

    // section_view fires once on mount (no reactive reads → no re-runs)
    Effect::new(move |prev: Option<()>| {
        if prev.is_none() {
            spawn_local(async {
                api::track_event(api::TrackEventPayload {
                    event_type: "section_view",
                    movie_id: None,
                    genre: None,
                    era: None,
                    section: Some("gems"),
                    page_num: None,
                })
                .await;
            });
            api::track_umami("section_view", r#"{"section":"gems"}"#);
        }
    });

    // Refetch whenever the URL query (page/filters/search/sort) changes.
    Effect::new(move |_| {
        let _ = retry.get(); // bumped by "Try again"
        let p = page();
        let y = year();
        let g = gstr();
        let s = search();
        let sf = sort();
        let sd = sort_dir();
        let wq = watch.with(watch_query);
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::fetch_gems(p, PER_PAGE, y, g, s, Some(sf), Some(sd), wq).await {
                Ok(r) => {
                    set_movies.set(r.data);
                    set_total.set(r.total);
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

    let n1 = navigate.clone();
    let on_genres_cb = Callback::new(move |gs: Vec<String>| {
        let s = if gs.is_empty() {
            None
        } else {
            Some(gs.join(","))
        };
        if let Some(ref genre_str) = s {
            let g = genre_str.clone();
            api::track_umami(
                "filter_genre",
                &format!(r#"{{"genre":"{}","section":"gems"}}"#, g),
            );
            spawn_local(async move {
                api::track_event(api::TrackEventPayload {
                    event_type: "filter_genre",
                    movie_id: None,
                    genre: Some(g),
                    era: None,
                    section: Some("gems"),
                    page_num: None,
                })
                .await;
            });
        }
        n1(
            &build_url("/", 1, &s, &year(), &search(), &sort(), &sort_dir()),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n2 = navigate.clone();
    let on_year_cb = Callback::new(move |y: Option<i32>| {
        if let Some(era) = y {
            api::track_umami(
                "filter_era",
                &format!(r#"{{"era":{},"section":"gems"}}"#, era),
            );
            spawn_local(async move {
                api::track_event(api::TrackEventPayload {
                    event_type: "filter_era",
                    movie_id: None,
                    genre: None,
                    era: Some(era),
                    section: Some("gems"),
                    page_num: None,
                })
                .await;
            });
        }
        n2(
            &build_url("/", 1, &gstr(), &y, &search(), &sort(), &sort_dir()),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n3 = navigate.clone();
    let on_search_cb = Callback::new(move |s: Option<String>| {
        if s.is_some() {
            api::track_umami("search_used", r#"{"section":"gems"}"#);
            spawn_local(async {
                api::track_event(api::TrackEventPayload {
                    event_type: "search_used",
                    movie_id: None,
                    genre: None,
                    era: None,
                    section: Some("gems"),
                    page_num: None,
                })
                .await;
            });
        }
        n3(
            &build_url("/", 1, &gstr(), &year(), &s, &sort(), &sort_dir()),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n_sort = navigate.clone();
    let on_sort_cb = Callback::new(move |(field, dir): (String, String)| {
        n_sort(
            &build_url("/", 1, &gstr(), &year(), &search(), &field, &dir),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n_clear = navigate.clone();
    let on_clear_cb = Callback::new(move |_| {
        n_clear(
            &build_url("/", 1, &None, &None, &None, &sort(), &sort_dir()),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let on_retry_cb = Callback::new(move |_| set_retry.update(|v| *v += 1));
    let nav_pg = navigate;
    let lang = use_lang();
    let d = move || dict(lang.get());

    view! {
        <Title text=move || d().title_home />
        <div class="max-w-7xl mx-auto px-4 py-8">
            <div class="mb-6">
                <h1 class="text-4xl font-bold text-stone-100 mb-1">{move || d().home_h1}</h1>
                <p class="text-stone-400">{move || d().home_desc}</p>
            </div>
            <FilterBar
                genres=Signal::derive(genres) year=Signal::derive(year)
                search=Signal::derive(search) total=Signal::derive(move || total.get())
                on_genres=on_genres_cb on_year=on_year_cb on_search=on_search_cb
                sort=Signal::derive(sort) sort_dir=Signal::derive(sort_dir) on_sort=on_sort_cb
            />
            <WatchFilterPanel />
            {move || render_movie_grid(loading.get(), error.get(), movies.get(), on_clear_cb, on_retry_cb)}
            {move || {
                let tp = total_pages(); let p = page();
                let n1 = nav_pg.clone(); let n2 = nav_pg.clone();
                let n3 = nav_pg.clone(); let n4 = nav_pg.clone(); let n5 = nav_pg.clone();
                view!{ <PaginationBar page=p total_pages=tp
                    on_first=Callback::new(move |_| {
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("gems"), page_num: Some(1) }).await; });
                        n1(&build_url("/", 1, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_prev=Callback::new(move |_| {
                        let prev = p - 1;
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("gems"), page_num: Some(prev) }).await; });
                        n2(&build_url("/", prev, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_next=Callback::new(move |_| {
                        let next = p + 1;
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("gems"), page_num: Some(next) }).await; });
                        n3(&build_url("/", next, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_last=Callback::new(move |_| {
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("gems"), page_num: Some(tp) }).await; });
                        n4(&build_url("/", tp, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_page=Callback::new(move |pg: i32| {
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("gems"), page_num: Some(pg) }).await; });
                        n5(&build_url("/", pg, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                /> }
            }}
        </div>
    }
}

// ── Acclaimed page ────────────────────────────────────────────────────────────
#[component]
pub fn AcclaimedPage() -> impl IntoView {
    let query = use_query_map();
    let navigate = use_navigate();
    let watch = use_watch();

    let page = move || query.with(|q| q.get("page").and_then(|v| v.parse().ok()).unwrap_or(1i32));
    let year = move || query.with(|q| q.get("year").and_then(|v| v.parse().ok()));
    let genres = move || {
        query.with(|q| {
            q.get("genres")
                .map(|v| {
                    v.split(',')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    };
    let search = move || query.with(|q| q.get("q").filter(|v| !v.is_empty()));
    let sort = move || query.with(|q| q.get("sort").unwrap_or_else(|| "rating".to_string()));
    let sort_dir = move || query.with(|q| q.get("sort_dir").unwrap_or_else(|| "desc".to_string()));
    let gstr = move || {
        let g = genres();
        if g.is_empty() {
            None
        } else {
            Some(g.join(","))
        }
    };

    let (movies, set_movies) = signal(Vec::<MovieSummary>::new());
    let (total, set_total) = signal(0i64);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);
    let (retry, set_retry) = signal(0u32);

    // section_view fires once on mount (no reactive reads → no re-runs)
    Effect::new(move |prev: Option<()>| {
        if prev.is_none() {
            spawn_local(async {
                api::track_event(api::TrackEventPayload {
                    event_type: "section_view",
                    movie_id: None,
                    genre: None,
                    era: None,
                    section: Some("acclaimed"),
                    page_num: None,
                })
                .await;
            });
            api::track_umami("section_view", r#"{"section":"acclaimed"}"#);
        }
    });

    // Refetch whenever the URL query (page/filters/search/sort) changes.
    Effect::new(move |_| {
        let _ = retry.get(); // bumped by "Try again"
        let p = page();
        let y = year();
        let g = gstr();
        let s = search();
        let sf = sort();
        let sd = sort_dir();
        let wq = watch.with(watch_query);
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::fetch_acclaimed(p, PER_PAGE, y, g, s, Some(sf), Some(sd), wq).await {
                Ok(r) => {
                    set_movies.set(r.data);
                    set_total.set(r.total);
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

    let n1 = navigate.clone();
    let on_genres_cb = Callback::new(move |gs: Vec<String>| {
        let s = if gs.is_empty() {
            None
        } else {
            Some(gs.join(","))
        };
        if let Some(ref genre_str) = s {
            let g = genre_str.clone();
            api::track_umami(
                "filter_genre",
                &format!(r#"{{"genre":"{}","section":"acclaimed"}}"#, g),
            );
            spawn_local(async move {
                api::track_event(api::TrackEventPayload {
                    event_type: "filter_genre",
                    movie_id: None,
                    genre: Some(g),
                    era: None,
                    section: Some("acclaimed"),
                    page_num: None,
                })
                .await;
            });
        }
        n1(
            &build_url(
                "/acclaimed",
                1,
                &s,
                &year(),
                &search(),
                &sort(),
                &sort_dir(),
            ),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n2 = navigate.clone();
    let on_year_cb = Callback::new(move |y: Option<i32>| {
        if let Some(era) = y {
            api::track_umami(
                "filter_era",
                &format!(r#"{{"era":{},"section":"acclaimed"}}"#, era),
            );
            spawn_local(async move {
                api::track_event(api::TrackEventPayload {
                    event_type: "filter_era",
                    movie_id: None,
                    genre: None,
                    era: Some(era),
                    section: Some("acclaimed"),
                    page_num: None,
                })
                .await;
            });
        }
        n2(
            &build_url(
                "/acclaimed",
                1,
                &gstr(),
                &y,
                &search(),
                &sort(),
                &sort_dir(),
            ),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n3 = navigate.clone();
    let on_search_cb = Callback::new(move |s: Option<String>| {
        if s.is_some() {
            api::track_umami("search_used", r#"{"section":"acclaimed"}"#);
            spawn_local(async {
                api::track_event(api::TrackEventPayload {
                    event_type: "search_used",
                    movie_id: None,
                    genre: None,
                    era: None,
                    section: Some("acclaimed"),
                    page_num: None,
                })
                .await;
            });
        }
        n3(
            &build_url("/acclaimed", 1, &gstr(), &year(), &s, &sort(), &sort_dir()),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n_sort = navigate.clone();
    let on_sort_cb = Callback::new(move |(field, dir): (String, String)| {
        n_sort(
            &build_url("/acclaimed", 1, &gstr(), &year(), &search(), &field, &dir),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n_clear = navigate.clone();
    let on_clear_cb = Callback::new(move |_| {
        n_clear(
            &build_url("/acclaimed", 1, &None, &None, &None, &sort(), &sort_dir()),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let on_retry_cb = Callback::new(move |_| set_retry.update(|v| *v += 1));
    let nav_pg = navigate;
    let lang = use_lang();
    let d = move || dict(lang.get());

    view! {
        <Title text=move || d().title_acclaimed />
        <div class="max-w-7xl mx-auto px-4 py-8">
            <div class="mb-6">
                <h1 class="text-4xl font-bold text-stone-100 mb-1">{move || d().acclaimed_h1}</h1>
                <p class="text-stone-400">{move || d().acclaimed_desc}</p>
            </div>
            <FilterBar
                genres=Signal::derive(genres) year=Signal::derive(year)
                search=Signal::derive(search) total=Signal::derive(move || total.get())
                on_genres=on_genres_cb on_year=on_year_cb on_search=on_search_cb
                sort=Signal::derive(sort) sort_dir=Signal::derive(sort_dir) on_sort=on_sort_cb
            />
            <WatchFilterPanel />
            {move || render_movie_grid(loading.get(), error.get(), movies.get(), on_clear_cb, on_retry_cb)}
            {move || {
                let tp = total_pages(); let p = page();
                let n1 = nav_pg.clone(); let n2 = nav_pg.clone();
                let n3 = nav_pg.clone(); let n4 = nav_pg.clone(); let n5 = nav_pg.clone();
                view!{ <PaginationBar page=p total_pages=tp
                    on_first=Callback::new(move |_| {
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("acclaimed"), page_num: Some(1) }).await; });
                        n1(&build_url("/acclaimed", 1, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_prev=Callback::new(move |_| {
                        let prev = p - 1;
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("acclaimed"), page_num: Some(prev) }).await; });
                        n2(&build_url("/acclaimed", prev, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_next=Callback::new(move |_| {
                        let next = p + 1;
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("acclaimed"), page_num: Some(next) }).await; });
                        n3(&build_url("/acclaimed", next, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_last=Callback::new(move |_| {
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("acclaimed"), page_num: Some(tp) }).await; });
                        n4(&build_url("/acclaimed", tp, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_page=Callback::new(move |pg: i32| {
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("acclaimed"), page_num: Some(pg) }).await; });
                        n5(&build_url("/acclaimed", pg, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                /> }
            }}
        </div>
    }
}

// ── Wildcards page ────────────────────────────────────────────────────────────
#[component]
pub fn WildcardsPage() -> impl IntoView {
    let query = use_query_map();
    let navigate = use_navigate();
    let watch = use_watch();

    let page = move || query.with(|q| q.get("page").and_then(|v| v.parse().ok()).unwrap_or(1i32));
    let year = move || query.with(|q| q.get("year").and_then(|v| v.parse().ok()));
    let genres = move || {
        query.with(|q| {
            q.get("genres")
                .map(|v| {
                    v.split(',')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    };
    let search = move || query.with(|q| q.get("q").filter(|v| !v.is_empty()));
    let sort = move || query.with(|q| q.get("sort").unwrap_or_else(|| "score".to_string()));
    let sort_dir = move || query.with(|q| q.get("sort_dir").unwrap_or_else(|| "desc".to_string()));
    let gstr = move || {
        let g = genres();
        if g.is_empty() {
            None
        } else {
            Some(g.join(","))
        }
    };

    let (movies, set_movies) = signal(Vec::<MovieSummary>::new());
    let (total, set_total) = signal(0i64);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);
    let (retry, set_retry) = signal(0u32);

    // section_view fires once on mount (no reactive reads → no re-runs)
    Effect::new(move |prev: Option<()>| {
        if prev.is_none() {
            spawn_local(async {
                api::track_event(api::TrackEventPayload {
                    event_type: "section_view",
                    movie_id: None,
                    genre: None,
                    era: None,
                    section: Some("wildcards"),
                    page_num: None,
                })
                .await;
            });
            api::track_umami("section_view", r#"{"section":"wildcards"}"#);
        }
    });

    // Refetch whenever the URL query (page/filters/search/sort) changes.
    Effect::new(move |_| {
        let _ = retry.get(); // bumped by "Try again"
        let p = page();
        let y = year();
        let g = gstr();
        let s = search();
        let sf = sort();
        let sd = sort_dir();
        let wq = watch.with(watch_query);
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::fetch_wildcards(p, PER_PAGE, y, g, s, Some(sf), Some(sd), wq).await {
                Ok(r) => {
                    set_movies.set(r.data);
                    set_total.set(r.total);
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

    let n1 = navigate.clone();
    let on_genres_cb = Callback::new(move |gs: Vec<String>| {
        let s = if gs.is_empty() {
            None
        } else {
            Some(gs.join(","))
        };
        if let Some(ref genre_str) = s {
            let g = genre_str.clone();
            api::track_umami(
                "filter_genre",
                &format!(r#"{{"genre":"{}","section":"wildcards"}}"#, g),
            );
            spawn_local(async move {
                api::track_event(api::TrackEventPayload {
                    event_type: "filter_genre",
                    movie_id: None,
                    genre: Some(g),
                    era: None,
                    section: Some("wildcards"),
                    page_num: None,
                })
                .await;
            });
        }
        n1(
            &build_url(
                "/wildcards",
                1,
                &s,
                &year(),
                &search(),
                &sort(),
                &sort_dir(),
            ),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n2 = navigate.clone();
    let on_year_cb = Callback::new(move |y: Option<i32>| {
        if let Some(era) = y {
            api::track_umami(
                "filter_era",
                &format!(r#"{{"era":{},"section":"wildcards"}}"#, era),
            );
            spawn_local(async move {
                api::track_event(api::TrackEventPayload {
                    event_type: "filter_era",
                    movie_id: None,
                    genre: None,
                    era: Some(era),
                    section: Some("wildcards"),
                    page_num: None,
                })
                .await;
            });
        }
        n2(
            &build_url(
                "/wildcards",
                1,
                &gstr(),
                &y,
                &search(),
                &sort(),
                &sort_dir(),
            ),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n3 = navigate.clone();
    let on_search_cb = Callback::new(move |s: Option<String>| {
        if s.is_some() {
            api::track_umami("search_used", r#"{"section":"wildcards"}"#);
            spawn_local(async {
                api::track_event(api::TrackEventPayload {
                    event_type: "search_used",
                    movie_id: None,
                    genre: None,
                    era: None,
                    section: Some("wildcards"),
                    page_num: None,
                })
                .await;
            });
        }
        n3(
            &build_url("/wildcards", 1, &gstr(), &year(), &s, &sort(), &sort_dir()),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n_sort = navigate.clone();
    let on_sort_cb = Callback::new(move |(field, dir): (String, String)| {
        n_sort(
            &build_url("/wildcards", 1, &gstr(), &year(), &search(), &field, &dir),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let n_clear = navigate.clone();
    let on_clear_cb = Callback::new(move |_| {
        n_clear(
            &build_url("/wildcards", 1, &None, &None, &None, &sort(), &sort_dir()),
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });
    let on_retry_cb = Callback::new(move |_| set_retry.update(|v| *v += 1));
    let nav_pg = navigate;
    let lang = use_lang();
    let d = move || dict(lang.get());

    view! {
        <Title text=move || d().title_wildcards />
        <div class="max-w-7xl mx-auto px-4 py-8">
            <div class="mb-6">
                <h1 class="text-4xl font-bold text-stone-100 mb-1">{move || d().wildcards_h1}</h1>
                <p class="text-stone-400">{move || d().wildcards_desc}</p>
                <p class="text-stone-600 text-sm mt-1">{move || d().wildcards_desc2}</p>
            </div>
            <FilterBar
                genres=Signal::derive(genres) year=Signal::derive(year)
                search=Signal::derive(search) total=Signal::derive(move || total.get())
                on_genres=on_genres_cb on_year=on_year_cb on_search=on_search_cb
                sort=Signal::derive(sort) sort_dir=Signal::derive(sort_dir) on_sort=on_sort_cb
            />
            <WatchFilterPanel />
            {move || render_movie_grid(loading.get(), error.get(), movies.get(), on_clear_cb, on_retry_cb)}
            {move || {
                let tp = total_pages(); let p = page();
                let n1 = nav_pg.clone(); let n2 = nav_pg.clone();
                let n3 = nav_pg.clone(); let n4 = nav_pg.clone(); let n5 = nav_pg.clone();
                view!{ <PaginationBar page=p total_pages=tp
                    on_first=Callback::new(move |_| {
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("wildcards"), page_num: Some(1) }).await; });
                        n1(&build_url("/wildcards", 1, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_prev=Callback::new(move |_| {
                        let prev = p - 1;
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("wildcards"), page_num: Some(prev) }).await; });
                        n2(&build_url("/wildcards", prev, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_next=Callback::new(move |_| {
                        let next = p + 1;
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("wildcards"), page_num: Some(next) }).await; });
                        n3(&build_url("/wildcards", next, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_last=Callback::new(move |_| {
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("wildcards"), page_num: Some(tp) }).await; });
                        n4(&build_url("/wildcards", tp, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                    on_page=Callback::new(move |pg: i32| {
                        spawn_local(async move { api::track_event(api::TrackEventPayload { event_type: "pagination", movie_id: None, genre: None, era: None, section: Some("wildcards"), page_num: Some(pg) }).await; });
                        n5(&build_url("/wildcards", pg, &gstr(), &year(), &search(), &sort(), &sort_dir()), NavigateOptions { replace: false, ..Default::default() });
                    })
                /> }
            }}
        </div>
    }
}

// ── Sign-in page ──────────────────────────────────────────────────────────────
#[component]
pub fn SignInPage() -> impl IntoView {
    let auth = use_context::<RwSignal<Option<AuthState>>>().expect("auth context missing");
    let navigate = use_navigate();

    let (email, set_email) = signal(String::new());
    let (sent, set_sent) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);
    let lang = use_lang();
    let d = move || dict(lang.get());

    Effect::new(move |_| {
        if auth.get().is_some() {
            navigate("/", NavigateOptions::default());
        }
    });

    let submit = move || {
        let raw = email.get_untracked();
        let e = raw.trim().to_lowercase();
        if e.is_empty() {
            set_error.set(Some(d().signin_err_empty.into()));
            return;
        }
        if !is_valid_email_client(&e) {
            set_error.set(Some(d().signin_err_invalid.into()));
            return;
        }
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match api::send_magic_link(&e).await {
                Ok(_) => set_sent.set(true),
                Err(msg) => {
                    let clean = if msg.to_lowercase().contains("invalid email") {
                        d().signin_err_invalid.to_string()
                    } else {
                        d().signin_err_generic.to_string()
                    };
                    set_error.set(Some(clean));
                    set_loading.set(false);
                }
            }
            set_loading.set(false);
        });
    };

    view! {
        <Title text=move || d().title_signin />
        <div class="min-h-96 flex items-start justify-center pt-16 px-4">
            <div class="w-full max-w-md">
                {move || if sent.get() {
                    view! {
                        <div class="text-center py-8">
                            <p class="text-5xl mb-6">"📬"</p>
                            <h1 class="text-2xl font-bold text-stone-100 mb-3">{move || d().signin_check_email}</h1>
                            <p class="text-stone-400 mb-2">
                                {move || d().signin_sent_prefix}
                                <span class="text-stone-200">{email.get()}</span>
                                "."
                            </p>
                            <p class="text-stone-500 text-sm mt-4 mb-8">
                                {move || d().signin_expires}
                            </p>
                            <button
                                class="text-sm text-stone-500 hover:text-stone-300 transition-colors underline"
                                on:click=move |_| { set_sent.set(false); set_email.set(String::new()); set_error.set(None); }
                            >{move || d().signin_use_different}</button>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div>
                            <div class="text-center mb-8">
                                <A href="/" attr:class="font-display text-4xl tracking-widest text-stone-100 hover:text-sc-accent transition-colors">
                                    "PEDRILHA"
                                </A>
                                <p class="text-stone-500 mt-2 text-sm">{move || d().signin_tagline}</p>
                            </div>

                            <div class="bg-sc-panel border border-sc-border rounded-lg p-8">
                                <h2 class="text-xl font-bold text-stone-100 mb-1">{move || d().signin_heading}</h2>
                                <p class="text-stone-500 text-sm mb-6">
                                    {move || d().signin_blurb}
                                </p>

                                <label class="block text-xs text-stone-500 uppercase tracking-widest mb-1.5">{move || d().signin_email_label}</label>
                                <input
                                    type="email"
                                    placeholder="your@email.com"
                                    autocomplete="email"
                                    class="w-full bg-sc-card text-stone-200 border border-sc-border-input rounded-md px-3 py-2.5 text-sm mb-1 focus:outline-none focus:border-sc-accent-border"
                                    prop:value=move || email.get()
                                    on:input=move |ev| {
                                        set_error.set(None);
                                        set_email.set(event_target_value(&ev));
                                    }
                                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                                        if ev.key() == "Enter" { submit(); }
                                    }
                                />

                                {move || error.get().map(|e| view! {
                                    <p class="text-red-400 text-xs mb-3 mt-1">{e}</p>
                                })}

                                <button
                                    class="w-full mt-3 bg-sc-accent-bg text-stone-100 rounded-md px-4 py-2.5 text-sm font-medium hover:bg-sc-accent-bg-hover transition-colors disabled:opacity-50"
                                    on:click=move |_| submit()
                                    prop:disabled=move || loading.get()
                                >
                                    {move || if loading.get() { d().signin_sending } else { d().signin_send_link }}
                                </button>
                            </div>

                            <p class="text-center text-stone-600 text-xs mt-6">
                                <A href="/" attr:class="hover:text-stone-400 transition-colors">{move || d().signin_back}</A>
                            </p>
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
}

// ── Movie detail page ─────────────────────────────────────────────────────────
#[component]
pub fn MovieDetail() -> impl IntoView {
    let params = use_params_map();
    let _navigate = use_navigate();
    let (movie, set_movie) = signal(Option::<gem_finder_shared::types::Movie>::None);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);

    let (wl_state, set_wl_state) = signal(Option::<WatchState>::None);
    let (wl_loading, set_wl_loading) = signal(false);
    let (wl_error, set_wl_error) = signal(Option::<String>::None);
    let (wl_saved, set_wl_saved) = signal(false);

    let auth = use_context::<RwSignal<Option<AuthState>>>().unwrap_or_else(|| RwSignal::new(None));
    let lang = use_lang();
    let d = move || dict(lang.get());
    let watch = use_watch();

    let movie_id = move || params.with_untracked(|p| p.get("id").map(|v| v.to_string()));
    let (retry, set_retry) = signal(0u32);

    // Refetch when "Try again" bumps the retry signal (first run = initial load).
    Effect::new(move |_| {
        let _ = retry.get();
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match movie_id() {
                None => {
                    set_error.set(Some("Invalid movie ID".into()));
                    set_loading.set(false);
                }
                Some(id) => match api::fetch_movie(&id).await {
                    Ok(m) => {
                        if let Some(a) = auth.get_untracked() {
                            if let Some(db_id) = m.id {
                                if let Ok(entry) = api::get_watchlist_entry(db_id, &a.token).await {
                                    set_wl_state.set(entry.map(|e| e.state));
                                }
                            }
                        }
                        // Track movie detail view — fire-and-forget, swallow errors.
                        if let Some(ref enc_id) = movie_id() {
                            let mid = enc_id.clone();
                            let title_str = m.title.clone();
                            api::track_umami(
                                "movie_view",
                                &format!(r#"{{"id":"{}","title":"{}"}}"#, mid, title_str),
                            );
                            api::track_event(api::TrackEventPayload {
                                event_type: "movie_view",
                                movie_id: Some(mid),
                                genre: None,
                                era: None,
                                section: None,
                                page_num: None,
                            })
                            .await;
                        }
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
    });

    view! {
        <Title text=move || {
            movie.get().map(|m| {
                let year = m.year.map(|y| y.to_string()).unwrap_or_default();
                if year.is_empty() {
                    format!("{} — Pedrilha", m.title)
                } else {
                    format!("{} ({}) — Pedrilha", m.title, year)
                }
            }).unwrap_or_else(|| "Pedrilha".to_string())
        } />
        <div class="max-w-4xl mx-auto px-4 py-8">
            <button
                class="text-sc-accent hover:text-sc-accent-hover text-sm mb-6 inline-block bg-transparent border-none cursor-pointer p-0"
                on:click=|_| {
                    if let Some(w) = web_sys::window() {
                        if let Ok(h) = w.history() { let _ = h.back(); }
                    }
                }
            >{move || d().detail_back}</button>

            {move || if loading.get() {
                view!{ <div class="animate-pulse mt-6 space-y-4">
                    <div class="h-8 bg-sc-border rounded w-2/3" />
                    <div class="h-4 bg-sc-border rounded w-1/3" />
                    <div class="flex gap-8 mt-8">
                        <div class="w-48 h-72 bg-sc-border rounded flex-shrink-0" />
                        <div class="flex-1 space-y-3">
                            <div class="h-4 bg-sc-border rounded" />
                            <div class="h-4 bg-sc-border rounded w-5/6" />
                        </div>
                    </div>
                </div> }.into_any()
            } else if let Some(err) = error.get() {
                view!{ <div class="py-16 text-center">
                    <p class="text-stone-300 mb-2">{move || d().detail_err_load}</p>
                    <p class="text-xs text-stone-600 mb-6">{err}</p>
                    <div class="flex items-center justify-center gap-4">
                        <button
                            class="text-sm text-sc-accent hover:text-sc-accent-hover border border-sc-accent-border rounded px-4 py-2"
                            on:click=move |_| set_retry.update(|v| *v += 1)
                        >{move || d().grid_try_again}</button>
                        <A href="/" attr:class="text-sm text-stone-400 hover:text-stone-200">{move || d().detail_back_gems}</A>
                    </div>
                </div> }.into_any()
            } else if let Some(m) = movie.get() {
                let poster   = m.poster_url.clone().unwrap_or_default();
                let title    = m.title.clone();
                let year     = m.year.map(|y| y.to_string()).unwrap_or_default();
                let director = m.director.clone();
                let genre    = m.genre.clone().unwrap_or_default();
                let overview = m.overview.clone().unwrap_or_default();
                let imdb_str = m.imdb_rating.map(|r| format!("{:.1}", r));
                let rt_str   = m.rt_critic_score.map(|r| format!("{}%", r));
                let gem_str  = m.gem_score.map(|s| format!("{:.0}%", s * 100.0));
                let gem_rank = m.gem_rank.filter(|r| *r >= 1);
                let audience_str = m.rt_audience_score.map(|r| format!("{}%", r));
                let release_day = m.release_date.as_deref().and_then(fmt_release_day);
                let first_genre = genre.split(", ").next().unwrap_or("").to_string();
                let decade = m.year.map(|y| (y / 10) * 10).filter(|d| *d >= 1900);
                let poster_bg = poster.clone();
                let yt_url = format!(
                    "https://www.youtube.com/results?search_query={}+{}+trailer",
                    urlenc(&title), year
                );
                let jw_url = format!("https://www.justwatch.com/us/search?q={}", urlenc(&title));
                let imdb_id  = m.imdb_id.clone();
                let movie_db_id = m.id.unwrap_or(0);
                // Stremio deep link: IMDb ids are Stremio's movie ids via Cinemeta.
                let title_enc = urlenc(&title);
                let stremio_app = match m.imdb_id.clone() {
                    Some(id) if !id.is_empty() => format!("stremio:///detail/movie/{}/{}", id, id),
                    _ => format!("stremio:///search?search={}", title_enc),
                };
                // Streaming availability grouped per region (Phase 10).
                let providers_data = m.watch_providers.clone().unwrap_or_default();

                view!{
                    <div class="mt-6 relative isolate">
                        // De-focused poster as an ambient backdrop behind the header.
                        {(!poster_bg.is_empty()).then(|| view!{
                            <div class="absolute -inset-x-4 -top-8 h-64 overflow-hidden pointer-events-none -z-10" aria-hidden="true">
                                <img src=poster_bg.clone() alt="" class="w-full h-full object-cover blur-2xl opacity-20 saturate-50" />
                            </div>
                        })}
                        <h1 class="text-3xl font-bold text-stone-100 mb-1">{title.clone()}</h1>
                        <p class="text-stone-400 mb-6">
                            {year.clone()} " · " {move || director.clone().unwrap_or_else(|| d().detail_unknown_director.to_string())}
                            {release_day.map(|rd| view!{ <span>" · " {rd}</span> })}
                        </p>
                        <div class="flex gap-8 flex-wrap">
                            <div class="flex-shrink-0 w-48 sm:w-64">
                                {if poster.is_empty() {
                                    view!{ <div class="w-full h-72 sm:h-96 bg-sc-card rounded flex items-center justify-center">
                                        <span class="text-5xl">"🎬"</span></div> }.into_any()
                                } else {
                                    view!{ <img src=poster alt=d().detail_poster_alt.replace("{}", &title) class="w-full rounded shadow-xl" /> }.into_any()
                                }}
                            </div>
                            <div class="flex-1 min-w-0">
                                <div class="flex flex-wrap gap-3 mb-6">
                                    {gem_str.map(|s| view!{
                                        <A href="/about"
                                           attr:title=move || d().detail_gem_tooltip
                                           attr:class="block">
                                            <div class="flex flex-col items-center bg-sc-accent-deep border border-sc-accent-border rounded px-4 py-2 hover:border-sc-accent transition-colors">
                                                <span class="text-xs text-sc-accent uppercase tracking-wide">{move || d().detail_gem_score}</span>
                                                <span class="text-2xl font-bold text-sc-accent-hover">{s}</span>
                                                {gem_rank.map(|r| view!{
                                                    <span class="text-[11px] text-sc-accent">{move || d().detail_rank.replace("{}", &r.to_string())}</span>
                                                })}
                                            </div>
                                        </A>
                                    })}
                                    {imdb_str.map(|r| view!{
                                        <div class="flex flex-col items-center bg-yellow-900 border border-yellow-700 rounded px-4 py-2">
                                            <span class="text-xs text-yellow-400 uppercase tracking-wide">{move || d().detail_rating}</span>
                                            <span class="text-2xl font-bold text-yellow-300">{r}</span>
                                        </div>
                                    })}
                                    {rt_str.map(|r| view!{
                                        <div class="flex flex-col items-center bg-red-900 border border-red-700 rounded px-4 py-2">
                                            <span class="text-xs text-red-400 uppercase tracking-wide">{move || d().detail_critics}</span>
                                            <span class="text-2xl font-bold text-red-300">{r}</span>
                                        </div>
                                    })}
                                    {audience_str.map(|r| view!{
                                        <div class="flex flex-col items-center bg-red-900 border border-red-700 rounded px-4 py-2">
                                            <span class="text-xs text-red-400 uppercase tracking-wide">{move || d().detail_audience}</span>
                                            <span class="text-2xl font-bold text-red-300">{r}</span>
                                        </div>
                                    })}
                                </div>
                                {{
                                    let gtags: Vec<_> = genre.split(", ").filter(|g| !g.is_empty()).map(|g| {
                                        let g = g.to_string();
                                        view!{ <span class="px-2 py-1 bg-sc-card border border-sc-border rounded text-xs text-stone-300">{g}</span> }.into_any()
                                    }).collect();
                                    if gtags.is_empty() {
                                        view!{ <div /> }.into_any()
                                    } else {
                                        view!{ <div class="flex flex-wrap gap-2 mb-4">{gtags}</div> }.into_any()
                                    }
                                }}
                                {if !overview.is_empty() {
                                    view!{ <p class="text-stone-300 leading-relaxed mb-6">{overview}</p> }.into_any()
                                } else { view!{ <div /> }.into_any() }}
                                <div class="flex flex-wrap gap-2">
                                    {imdb_id.map(|id| view!{
                                        <a href={format!("https://www.imdb.com/title/{}", id)}
                                            target="_blank" rel="noopener noreferrer"
                                            class="text-sm text-yellow-400 hover:text-yellow-300 border border-yellow-700 rounded px-3 py-1.5">
                                            {move || d().detail_imdb_link}
                                        </a>
                                    })}
                                    <a href=yt_url
                                        target="_blank" rel="noopener noreferrer"
                                        class="text-sm text-stone-300 hover:text-stone-100 border border-sc-border hover:border-stone-600 rounded px-3 py-1.5">
                                        {move || d().detail_trailer}
                                    </a>
                                    <a href=jw_url
                                        target="_blank" rel="noopener noreferrer"
                                        class="text-sm text-stone-300 hover:text-stone-100 border border-sc-border hover:border-stone-600 rounded px-3 py-1.5">
                                        {move || d().detail_where_watch}
                                    </a>
                                    <a href=stremio_app
                                        class="text-sm text-purple-300 hover:text-purple-200 border border-purple-800 hover:border-purple-600 rounded px-3 py-1.5">
                                        {move || d().detail_open_stremio}
                                    </a>
                                </div>

                                // ── Streaming availability (Phase 10) ─────────────────────────────
                                {
                                    let providers_data = providers_data.clone();
                                    move || {
                                        let region = watch.with(|w| w.region.clone());
                                        let rp = providers_data.iter().find(|r| r.region == region).cloned();
                                        match rp {
                                            Some(rp) if !rp.providers.is_empty() => {
                                                let group = |accesses: &[&str]| -> Vec<gem_finder_shared::types::MovieProvider> {
                                                    rp.providers.iter()
                                                        .filter(|p| accesses.contains(&p.access.as_str()))
                                                        .cloned().collect()
                                                };
                                                let included = group(&["flatrate"]);
                                                let free_ads = group(&["free", "ads"]);
                                                let rent_buy = group(&["rent", "buy"]);
                                                let tmdb_link = rp.tmdb_link.clone().unwrap_or_default();
                                                let render_group = |label: String, items: Vec<gem_finder_shared::types::MovieProvider>, link: String| {
                                                    if items.is_empty() { return view!{ <div /> }.into_any(); }
                                                    view!{
                                                        <div class="mb-3">
                                                            <p class="text-[0.65rem] uppercase tracking-wide text-stone-500 font-semibold mb-1.5">{label}</p>
                                                            <div class="flex flex-wrap gap-2">
                                                                {items.into_iter().map(|p| {
                                                                    let logo = p.logo_path.as_deref().map(|x| format!("https://image.tmdb.org/t/p/w45{}", x)).unwrap_or_default();
                                                                    let has_logo = !logo.is_empty();
                                                                    let name = p.provider_name.clone();
                                                                    let link = link.clone();
                                                                    view!{
                                                                        <a href=link target="_blank" rel="noopener noreferrer"
                                                                            class="flex items-center gap-1.5 px-2 py-1 bg-sc-card border border-sc-border rounded text-xs text-stone-300 hover:border-stone-600">
                                                                            {has_logo.then(|| view!{ <img src=logo alt=name.clone() loading="lazy" class="w-4 h-4 rounded-sm" /> })}
                                                                            <span>{p.provider_name.clone()}</span>
                                                                        </a>
                                                                    }
                                                                }).collect::<Vec<_>>()}
                                                            </div>
                                                        </div>
                                                    }.into_any()
                                                };
                                                view!{
                                                    <div class="mt-6 pt-6 border-t border-sc-border">
                                                        {render_group(d().providers_included_with.to_string(), included, tmdb_link.clone())}
                                                        {render_group(d().providers_free_ads.to_string(), free_ads, tmdb_link.clone())}
                                                        {render_group(d().providers_rent_buy.to_string(), rent_buy, tmdb_link.clone())}
                                                        <p class="text-[0.6rem] text-stone-600 mt-2">{move || d().providers_attribution}</p>
                                                    </div>
                                                }.into_any()
                                            }
                                            _ => {
                                                // No synced provider data for this region — hide the section
                                                // entirely (the JustWatch link above already covers this case).
                                                view!{ <div /> }.into_any()
                                            }
                                        }
                                    }
                                }

                                // ── Watchlist ─────────────────────────────────────────────────────
                                <div class="mt-6 pt-6 border-t border-sc-border">
                                    {move || match auth.get() {
                                        None => view! {
                                            <div>
                                                <p class="text-stone-500 text-sm mb-2">{move || d().detail_track}</p>
                                                <A
                                                    href="/signin"
                                                    attr:class="inline-block text-sm text-sc-accent hover:text-sc-accent-hover border border-sc-accent-border rounded px-3 py-1.5"
                                                >{move || d().detail_signin_add}</A>
                                            </div>
                                        }.into_any(),
                                        Some(a) => {
                                            let t_want  = a.token.clone();
                                            let t_watch = a.token.clone();
                                            let t_nope  = a.token.clone();
                                            let t_del   = a.token.clone();

                                            let on_want = move |_: web_sys::MouseEvent| {
                                                let t = t_want.clone();
                                                set_wl_loading.set(true); set_wl_error.set(None);
                                                spawn_local(async move {
                                                    match api::upsert_watchlist(movie_db_id, WatchState::WantToWatch, None, &t).await {
                                                        Ok(_)  => { set_wl_state.set(Some(WatchState::WantToWatch)); set_wl_saved.set(true); },
                                                        Err(e) => set_wl_error.set(Some(e)),
                                                    }
                                                    set_wl_loading.set(false);
                                                });
                                            };
                                            let on_watched = move |_: web_sys::MouseEvent| {
                                                let t = t_watch.clone();
                                                set_wl_loading.set(true); set_wl_error.set(None);
                                                spawn_local(async move {
                                                    match api::upsert_watchlist(movie_db_id, WatchState::Watched, None, &t).await {
                                                        Ok(_)  => { set_wl_state.set(Some(WatchState::Watched)); set_wl_saved.set(true); },
                                                        Err(e) => set_wl_error.set(Some(e)),
                                                    }
                                                    set_wl_loading.set(false);
                                                });
                                            };
                                            let on_nope = move |_: web_sys::MouseEvent| {
                                                let t = t_nope.clone();
                                                set_wl_loading.set(true); set_wl_error.set(None);
                                                spawn_local(async move {
                                                    match api::upsert_watchlist(movie_db_id, WatchState::NotInterested, None, &t).await {
                                                        Ok(_)  => { set_wl_state.set(Some(WatchState::NotInterested)); set_wl_saved.set(true); },
                                                        Err(e) => set_wl_error.set(Some(e)),
                                                    }
                                                    set_wl_loading.set(false);
                                                });
                                            };
                                            let on_remove = move |_: web_sys::MouseEvent| {
                                                let t = t_del.clone();
                                                set_wl_loading.set(true); set_wl_error.set(None);
                                                spawn_local(async move {
                                                    match api::delete_watchlist(movie_db_id, &t).await {
                                                        Ok(_)  => { set_wl_state.set(None); set_wl_saved.set(false); },
                                                        Err(e) => set_wl_error.set(Some(e)),
                                                    }
                                                    set_wl_loading.set(false);
                                                });
                                            };

                                            view! {
                                                <div>
                                                    <p class="text-xs text-stone-600 uppercase tracking-widest mb-3">{move || d().detail_your_list}</p>
                                                    <div class="flex flex-wrap gap-2">
                                                        <button
                                                            class=move || if wl_state.get() == Some(WatchState::WantToWatch) {
                                                                "px-3 py-1.5 rounded text-sm bg-sc-accent-bg text-stone-100 border border-sc-accent-border disabled:opacity-50"
                                                            } else {
                                                                "px-3 py-1.5 rounded text-sm text-stone-400 border border-sc-border hover:border-sc-accent-border hover:text-stone-200 disabled:opacity-50"
                                                            }
                                                            on:click=on_want
                                                            prop:disabled=move || wl_loading.get()
                                                        >{move || d().wl_want}</button>
                                                        <button
                                                            class=move || if wl_state.get() == Some(WatchState::Watched) {
                                                                "px-3 py-1.5 rounded text-sm bg-sc-accent-bg text-stone-100 border border-sc-accent-border disabled:opacity-50"
                                                            } else {
                                                                "px-3 py-1.5 rounded text-sm text-stone-400 border border-sc-border hover:border-sc-accent-border hover:text-stone-200 disabled:opacity-50"
                                                            }
                                                            on:click=on_watched
                                                            prop:disabled=move || wl_loading.get()
                                                        >{move || d().wl_watched}</button>
                                                        <button
                                                            class=move || if wl_state.get() == Some(WatchState::NotInterested) {
                                                                "px-3 py-1.5 rounded text-sm bg-sc-accent-bg text-stone-100 border border-sc-accent-border disabled:opacity-50"
                                                            } else {
                                                                "px-3 py-1.5 rounded text-sm text-stone-400 border border-sc-border hover:border-sc-accent-border hover:text-stone-200 disabled:opacity-50"
                                                            }
                                                            on:click=on_nope
                                                            prop:disabled=move || wl_loading.get()
                                                        >{move || d().wl_not_interested}</button>
                                                        <button
                                                            class=move || if wl_state.get().is_some() {
                                                                "px-3 py-1.5 rounded text-sm text-stone-600 hover:text-stone-400 border border-sc-border"
                                                            } else { "hidden" }
                                                            on:click=on_remove
                                                            prop:disabled=move || wl_loading.get()
                                                        >{move || d().wl_remove}</button>
                                                    </div>
                                                    {move || wl_saved.get().then(|| view! {
                                                        <p class="text-sc-accent text-xs mt-2">
                                                            {move || d().detail_saved_prefix}
                                                            <A href="/watchlist" attr:class="underline hover:text-sc-accent-hover">{move || d().detail_saved_link}</A>
                                                        </p>
                                                    })}
                                                    {move || wl_error.get().map(|e| view! {
                                                        <p class="text-red-400 text-xs mt-2">{e}</p>
                                                    })}
                                                </div>
                                            }.into_any()
                                        }
                                    }}
                                </div>

                                // ── More like this — pure filter links ────────────────
                                <div class="mt-6 flex flex-wrap gap-2">
                                    {(!first_genre.is_empty()).then(|| {
                                        let fg = first_genre.clone();
                                        view!{
                                            <A href=format!("/?genres={}", urlenc(&fg))
                                                attr:class="text-xs text-stone-400 hover:text-stone-200 bg-sc-panel border border-sc-border hover:border-stone-600 rounded-full px-4 py-1.5">
                                                {move || d().detail_more_genre.replace("{}", genre_label(lang.get(), &fg))}
                                            </A>
                                        }
                                    })}
                                    {decade.map(|dec| view!{
                                        <A href=format!("/?year={}", dec)
                                            attr:class="text-xs text-stone-400 hover:text-stone-200 bg-sc-panel border border-sc-border hover:border-stone-600 rounded-full px-4 py-1.5">
                                            {move || d().detail_more_decade.replace("{}", &dec.to_string())}
                                        </A>
                                    })}
                                </div>
                            </div>
                        </div>
                    </div>
                }.into_any()
            } else { view!{ <div /> }.into_any() }}
        </div>
    }
}

// ── Shared movie grid renderer ────────────────────────────────────────────────
/// Build the active `WatchQuery` from prefs, or `None` when no filter is applied.
fn watch_query(w: &WatchPrefs) -> Option<api::WatchQuery> {
    if w.active() {
        Some(api::WatchQuery {
            region: w.region.clone(),
            providers: w.selected_csv(),
            rentals: w.rentals,
        })
    } else {
        None
    }
}

// ── "What can I watch?" filter panel (Phase 10) ─────────────────────────────
/// Self-contained provider picker. Reads/writes the app-wide `WatchPrefs`
/// context; list pages refetch reactively when it changes.
#[component]
fn WatchFilterPanel() -> impl IntoView {
    let watch = use_watch();
    let lang = use_lang();
    let d = move || dict(lang.get());
    let (open, set_open) = signal(false);
    let (providers, set_providers) = signal(Vec::<ProviderInfo>::new());

    // Refetch the picker whenever the region changes (only — not on every toggle).
    let region = Memo::new(move |_| watch.with(|w| w.region.clone()));
    Effect::new(move |_| {
        let r = region.get();
        spawn_local(async move {
            match api::fetch_providers(&r).await {
                Ok(list) => set_providers.set(list),
                Err(_) => set_providers.set(Vec::new()),
            }
        });
    });

    let active_count =
        move || watch.with(|w| w.selected().len() as i32 + if w.rentals { 1 } else { 0 });
    let rentals_on = move || watch.with(|w| w.rentals);

    // Changing the watch filter resets to page 1 (mirrors genre/year/search) so a
    // shrunk result set never strands the user on a now-empty page.
    let navigate = use_navigate();
    let query = use_query_map();
    let location = use_location();
    let reset_page = Callback::new(move |_: ()| {
        let path = location.pathname.get_untracked();
        let params = query.get_untracked();
        let mut url = format!("{}?page=1", path);
        for k in ["genres", "year", "q", "sort", "sort_dir"] {
            if let Some(v) = params.get(k).filter(|v| !v.is_empty()) {
                url.push_str(&format!("&{}={}", k, v));
            }
        }
        navigate(
            &url,
            NavigateOptions {
                replace: true,
                ..Default::default()
            },
        );
    });

    let btn_class = move || {
        let base = "flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md border transition-colors cursor-pointer";
        if open.get() || active_count() > 0 {
            format!("{} border-sc-accent text-sc-accent bg-sc-accent-deep", base)
        } else {
            format!("{} border-sc-border text-stone-400 bg-sc-card hover:border-stone-600 hover:text-stone-200", base)
        }
    };
    let seg = |active: bool| -> &'static str {
        if active {
            "px-3 py-1 text-xs rounded border border-sc-accent text-sc-accent bg-sc-accent-deep font-medium cursor-pointer"
        } else {
            "px-3 py-1 text-xs rounded border border-sc-border text-stone-400 bg-sc-card hover:border-stone-600 hover:text-stone-200 cursor-pointer"
        }
    };

    view! {
        <div class="relative mb-4">
            {move || open.get().then(|| view! {
                <div style="position:fixed;inset:0;z-index:40" on:click=move |_| set_open.set(false) />
            })}

            <div class="flex items-center gap-2 flex-wrap">
                <button class=btn_class on:click=move |_| set_open.update(|v| *v = !*v)>
                    <span>{move || d().filter_watchable}</span>
                    {move || (active_count() > 0).then(|| view! {
                        <span class="ml-1 px-1.5 rounded-full bg-sc-accent text-stone-900 text-[10px] font-bold">{move || active_count()}</span>
                    })}
                </button>
                {move || (active_count() > 0).then(|| {
                    let chip = d().filter_active_chip.replace("{}", &active_count().to_string());
                    view! {
                        <span class="flex items-center gap-1.5 px-2 py-1 text-xs rounded-full bg-sc-accent-deep text-sc-accent border border-sc-accent">
                            <span>{chip}</span>
                            <button class="hover:text-stone-100 font-bold" on:click=move |_| { watch.update(|w| w.clear()); reset_page.run(()); }>"×"</button>
                        </span>
                    }
                })}
            </div>

            {move || open.get().then(|| view! {
                <div style="position:absolute;top:calc(100% + 6px);left:0;min-width:300px;max-width:360px;background-color:var(--sc-panel,#17100a);border:1px solid var(--sc-border);border-radius:10px;box-shadow:0 24px 48px rgba(0,0,0,0.7);padding:14px;z-index:200">
                    <p class="text-[0.65rem] uppercase tracking-wide text-sc-accent-border font-semibold mb-2">{move || d().filter_region}</p>
                    <div class="flex gap-2 mb-3">
                        <button class=move || seg(watch.with(|w| w.region == "US")) on:click=move |_| { watch.update(|w| w.region = "US".to_string()); reset_page.run(()); }>"US"</button>
                        <button class=move || seg(watch.with(|w| w.region == "BR")) on:click=move |_| { watch.update(|w| w.region = "BR".to_string()); reset_page.run(()); }>"BR"</button>
                    </div>

                    <div class="grid grid-cols-2 gap-1.5 max-h-60 overflow-y-auto mb-3">
                        {move || {
                            let sel: Vec<i32> = watch.with(|w| w.selected().clone());
                            providers.get().into_iter().map(|p| {
                                let id = p.provider_id;
                                let is_sel = sel.contains(&id);
                                let logo = p.logo_path.as_deref()
                                    .map(|x| format!("https://image.tmdb.org/t/p/w45{}", x))
                                    .unwrap_or_default();
                                let has_logo = !logo.is_empty();
                                let name = p.name.clone();
                                let cls = if is_sel {
                                    "flex items-center gap-1.5 px-2 py-1 text-xs rounded border border-sc-accent text-sc-accent bg-sc-accent-deep cursor-pointer"
                                } else {
                                    "flex items-center gap-1.5 px-2 py-1 text-xs rounded border border-sc-border text-stone-300 bg-sc-card hover:border-stone-600 cursor-pointer"
                                };
                                view! {
                                    <button class=cls on:click=move |_| { watch.update(|w| w.toggle(id)); reset_page.run(()); }>
                                        {has_logo.then(|| view! { <img src=logo alt=name.clone() loading="lazy" class="w-4 h-4 rounded-sm flex-shrink-0" /> })}
                                        <span class="truncate">{p.name.clone()}</span>
                                    </button>
                                }
                            }).collect::<Vec<_>>()
                        }}
                    </div>
                    {move || providers.get().is_empty().then(|| view! {
                        <p class="text-xs text-stone-500 mb-3">{move || d().filter_empty_hint}</p>
                    })}

                    <label class="flex items-start gap-2 mb-3 cursor-pointer">
                        <input type="checkbox" class="mt-0.5" prop:checked=rentals_on
                            on:change=move |_| { watch.update(|w| w.rentals = !w.rentals); reset_page.run(()); } />
                        <span>
                            <span class="text-xs text-stone-200 block">{move || d().filter_include_rentals}</span>
                            <span class="text-[0.65rem] text-stone-500 block">{move || d().filter_rentals_hint}</span>
                        </span>
                    </label>

                    <button class="text-xs text-stone-400 hover:text-stone-200" on:click=move |_| { watch.update(|w| w.clear()); reset_page.run(()); }>{move || d().filter_clear}</button>
                    <p class="text-[0.6rem] text-stone-600 mt-3 pt-2 border-t border-sc-border">{move || d().providers_attribution}</p>
                </div>
            })}
        </div>
    }
}

fn render_movie_grid(
    loading: bool,
    error: Option<String>,
    movies: Vec<MovieSummary>,
    on_clear: Callback<()>,
    on_retry: Callback<()>,
) -> impl IntoView {
    let lang = use_lang();
    let d = move || dict(lang.get());
    let inner = if loading {
        view! { <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
            {(0..PER_PAGE).map(|_| view!{ <SkeletonCard /> }).collect::<Vec<_>>()}
        </div> }
        .into_any()
    } else if let Some(err) = error {
        view! { <div class="py-16 text-center">
            <p class="text-stone-300 mb-2">{move || d().grid_err_load}</p>
            <p class="text-xs text-stone-600 mb-6">{err}</p>
            <button
                class="text-sm text-sc-accent hover:text-sc-accent-hover border border-sc-accent-border rounded px-4 py-2"
                on:click=move |_| on_retry.run(())
            >{move || d().grid_try_again}</button>
        </div> }.into_any()
    } else if movies.is_empty() {
        // When a watch filter is what emptied the list, explain that and offer to
        // clear it directly (the generic "clear filters" only clears URL filters).
        let watch = use_watch();
        if watch.with(|w| w.active()) {
            view! { <div class="py-16 text-center">
                <p class="text-stone-500 mb-6">{move || d().grid_no_watch_matches}</p>
                <button
                    class="text-sm text-stone-300 hover:text-stone-100 border border-sc-border hover:border-stone-600 rounded px-4 py-2"
                    on:click=move |_| watch.update(|w| w.clear())
                >{move || d().filter_clear}</button>
            </div> }.into_any()
        } else {
            view! { <div class="py-16 text-center">
                <p class="text-stone-500 mb-6">{move || d().grid_no_gems}</p>
                <button
                    class="text-sm text-stone-300 hover:text-stone-100 border border-sc-border hover:border-stone-600 rounded px-4 py-2"
                    on:click=move |_| on_clear.run(())
                >{move || d().grid_clear_filters}</button>
            </div> }.into_any()
        }
    } else {
        view! { <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4" style="isolation:isolate">
            {movies.into_iter().map(|m| view!{ <MovieCard movie=m /> }).collect::<Vec<_>>()}
        </div> }
        .into_any()
    };
    // Stable minimum height so loading → error/empty transitions don't collapse the page.
    view! { <div class="min-h-[400px]">{inner}</div> }
}

// ── Skeleton card ─────────────────────────────────────────────────────────────
#[component]
fn SkeletonCard() -> impl IntoView {
    view! {
        <div class="bg-sc-card rounded overflow-hidden animate-pulse">
            <div class="bg-sc-border" style="aspect-ratio:2/3" />
            <div class="p-3 space-y-2">
                <div class="h-3 bg-sc-border rounded w-3/4" />
                <div class="h-3 bg-sc-border rounded w-1/2" />
            </div>
        </div>
    }
}

// ── Movie card ────────────────────────────────────────────────────────────────
#[component]
fn MovieCard(movie: MovieSummary) -> impl IntoView {
    let navigate = use_navigate();
    let lang = use_lang();
    let d = move || dict(lang.get());
    let href = format!("/movie/{}", encode_movie_id(movie.id));
    let href_nav = href.clone();
    let encoded_id = encode_movie_id(movie.id);
    let poster = movie.poster_url.clone().unwrap_or_default();
    let has_poster = !poster.is_empty();
    // Card numerals drop the % sign — the glyph carries the unit, tooltips spell it out.
    let gem_score = movie.gem_score.map(|s| format!("{:.0}", s * 100.0));
    let year = movie.year.map(|y| y.to_string()).unwrap_or_default();
    let title = movie.title.clone();
    let director = movie.director.clone().unwrap_or_default();
    let imdb = movie.imdb_rating.map(|r| format!("{:.1}", r));
    let rt = movie.rt_critic_score.map(|r| r.to_string());
    let watch_badge = movie.watch_badge.clone();

    view! {
        <a
            href=href
            class="group block bg-sc-card rounded overflow-hidden hover:ring-1 hover:ring-sc-accent-bg transition-all duration-200 cursor-pointer"
            on:click=move |ev: web_sys::MouseEvent| {
                if !ev.meta_key() && !ev.ctrl_key() && !ev.shift_key() && ev.button() == 0 {
                    ev.prevent_default();
                    let mid = encoded_id.clone();
                    api::track_umami("movie_view", &format!(r#"{{"id":"{}"}}"#, mid));
                    spawn_local(async move {
                        api::track_event(api::TrackEventPayload {
                            event_type: "movie_view",
                            movie_id: Some(mid),
                            genre: None, era: None, section: None, page_num: None,
                        }).await;
                    });
                    navigate(&href_nav, Default::default());
                }
            }
        >
            <div class="overflow-hidden relative" style="aspect-ratio:2/3">
                {if has_poster {
                    view!{ <img src=poster alt=title.clone() loading="lazy"
                        class="w-full h-full object-cover motion-safe:group-hover:scale-105 transition-transform duration-300" /> }.into_any()
                } else {
                    view!{ <div class="w-full h-full bg-sc-border flex items-center justify-center">
                        <span class="text-5xl" aria-hidden="true">"🎬"</span></div> }.into_any()
                }}
            </div>
            <div class="p-3">
                <h3 class="text-stone-100 font-medium text-sm leading-snug line-clamp-2 mb-1">{title}</h3>
                <div class="flex items-center justify-between text-xs mb-0.5">
                    <span class="text-stone-400 tabular-nums">{year}</span>
                    <div class="flex flex-wrap justify-end gap-x-1.5 gap-y-0.5 items-center tabular-nums">
                        {gem_score.map(|s| { let sa = s.clone(); view!{ <span class="text-sc-accent font-semibold whitespace-nowrap" title=move || d().card_gem_tooltip aria-label=move || d().card_gem_aria.replace("{}", &sa)>"✦ "{s}</span> } })}
                        {imdb.map(|r| { let ra = r.clone(); view!{ <span class="text-yellow-400 whitespace-nowrap" title=move || d().card_rating_tooltip aria-label=move || d().card_rating_aria.replace("{}", &ra)>"★ "{r}</span> } })}
                        {rt.map(|r|  { let ra = r.clone(); view!{ <span class="text-red-400 whitespace-nowrap" title=move || d().card_critic_tooltip aria-label=move || d().card_critic_aria.replace("{}", &ra)>"🍅 "{r}</span> } })}
                    </div>
                </div>
                {if !director.is_empty() {
                    view!{ <p class="text-xs text-stone-500 mt-0.5 truncate">{director}</p> }.into_any()
                } else { view!{ <span /> }.into_any() }}
                {watch_badge.map(|b| {
                    let logo = b.logo_path.as_deref()
                        .map(|p| format!("https://image.tmdb.org/t/p/w45{}", p))
                        .unwrap_or_default();
                    let has_logo = !logo.is_empty();
                    let included = b.tier == WatchTier::Included;
                    let name = b.provider_name.clone();
                    let name_title = name.clone();
                    view!{
                        <div class="flex items-center gap-1 mt-1.5">
                            {has_logo.then(|| view!{
                                <img src=logo alt=name.clone() loading="lazy"
                                    class="w-4 h-4 rounded-sm flex-shrink-0" />
                            })}
                            <span
                                class=if included {
                                    "text-[10px] font-medium truncate text-emerald-400"
                                } else {
                                    "text-[10px] font-medium truncate text-stone-400"
                                }
                                title=name_title
                            >
                                {move || if included { d().badge_included } else { d().badge_rent }}
                            </span>
                        </div>
                    }
                })}
            </div>
        </a>
    }
}

// ── Admin page ────────────────────────────────────────────────────────────────
#[derive(Clone, PartialEq)]
enum ActionState {
    Idle,
    Running,
    Done(String),
    Failed(String),
}

// ── Auth verify page ──────────────────────────────────────────────────────────
#[component]
pub fn VerifyPage() -> impl IntoView {
    let query = use_query_map();
    let navigate = use_navigate();
    let auth = use_context::<RwSignal<Option<AuthState>>>().expect("auth context missing");
    let lang = use_lang();
    let d = move || dict(lang.get());
    let (status, set_status) = signal(d().verify_verifying.to_string());
    let (failed, set_failed) = signal(false);

    let token_val = query.with_untracked(|q| q.get("token").unwrap_or_default().to_string());

    if token_val.is_empty() {
        set_status.set(d().verify_incomplete.to_string());
        set_failed.set(true);
    } else {
        spawn_local(async move {
            match api::verify_token(&token_val).await {
                Ok(resp) => {
                    save_auth_to_storage(&resp.token, &resp.user_id, &resp.email, None);
                    let is_admin = jwt_is_admin(&resp.token);
                    auth.set(Some(AuthState {
                        token: resp.token,
                        user_id: resp.user_id,
                        email: resp.email,
                        username: None,
                        is_admin,
                    }));
                    navigate("/", NavigateOptions::default());
                }
                Err(_) => {
                    set_status.set(d().verify_invalid.to_string());
                    set_failed.set(true);
                }
            }
        });
    }

    view! {
        <Title text=move || d().title_signing_in />
        <div class="max-w-md mx-auto px-4 py-16 text-center">
            <p class="text-2xl mb-4" aria-hidden="true">"🔑"</p>
            <p class="text-stone-300">{move || status.get()}</p>
            {move || failed.get().then(|| view!{
                <A href="/signin"
                    attr:class="inline-block mt-6 text-sm text-sc-accent hover:text-sc-accent-hover border border-sc-accent-border rounded px-4 py-2">
                    {move || d().verify_request_new}
                </A>
            })}
        </div>
    }
}

#[component]
pub fn AdminPage() -> impl IntoView {
    let auth = use_context::<RwSignal<Option<AuthState>>>().expect("auth context missing");
    let navigate = use_navigate();

    // Redirect non-admins immediately
    Effect::new(move |_| {
        if auth.get().is_none_or(|a| !a.is_admin) {
            navigate("/", NavigateOptions::default());
        }
    });

    let lang = use_lang();
    let d = move || dict(lang.get());

    // JWT from auth state — used as the bearer token for all admin API calls
    let get_token = move || auth.get_untracked().map(|a| a.token).unwrap_or_default();

    let (seed_state, set_seed_state) = signal(ActionState::Idle);
    let (sync_state, set_sync_state) = signal(ActionState::Idle);
    let (enrich_state, set_enrich_state) = signal(ActionState::Idle);
    let (score_state, set_score_state) = signal(ActionState::Idle);
    let (provider_state, set_provider_state) = signal(ActionState::Idle);
    let (enrich_limit, set_enrich_limit) = signal(10_000i64);
    let (logs, set_logs) = signal(Vec::<serde_json::Value>::new());
    let (logs_loading, set_logs_loading) = signal(false);

    let (smtp_warn, set_smtp_warn) = signal(false);
    let (tmdb_warn, set_tmdb_warn) = signal(false);
    let (omdb_warn, set_omdb_warn) = signal(false);
    let (log_rotation, set_log_rotation) = signal("never".to_string());

    let fetch_logs = move || {
        let tok = get_token();
        set_logs_loading.set(true);
        spawn_local(async move {
            if let Ok(resp) = api::admin_logs(&tok).await {
                if let Some(arr) = resp.get("logs").and_then(|v| v.as_array()) {
                    set_logs.set(arr.clone());
                }
            }
            set_logs_loading.set(false);
        });
    };

    Effect::new(move |_| {
        fetch_logs();
        spawn_local(async move {
            if let Ok(status) = api::fetch_admin_status().await {
                let smtp = status
                    .get("smtp_configured")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let tmdb = status
                    .get("tmdb_configured")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let omdb = status
                    .get("omdb_configured")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                set_smtp_warn.set(!smtp);
                set_tmdb_warn.set(!tmdb);
                set_omdb_warn.set(!omdb);
                let rot = status
                    .get("log_rotation")
                    .and_then(|v| v.as_str())
                    .unwrap_or("never")
                    .to_string();
                set_log_rotation.set(rot);
            }
        });
    });

    let run_seed = move |_| {
        let tok = get_token();
        set_seed_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_seed(&tok).await {
                Ok(_) => set_seed_state.set(ActionState::Done(d().admin_started.into())),
                Err(e) => set_seed_state.set(ActionState::Failed(e)),
            }
        });
    };
    let run_sync = move |_| {
        let tok = get_token();
        set_sync_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_sync(&tok).await {
                Ok(_) => set_sync_state.set(ActionState::Done(d().admin_started.into())),
                Err(e) => set_sync_state.set(ActionState::Failed(e)),
            }
        });
    };
    let run_enrich = move |_| {
        let limit = enrich_limit.get();
        let tok = get_token();
        set_enrich_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_enrich(limit, &tok).await {
                Ok(_) => set_enrich_state.set(ActionState::Done(d().admin_started.into())),
                Err(e) => set_enrich_state.set(ActionState::Failed(e)),
            }
        });
    };
    let run_score = move |_| {
        let tok = get_token();
        set_score_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_score(&tok).await {
                Ok(_) => set_score_state.set(ActionState::Done(d().admin_started.into())),
                Err(e) => set_score_state.set(ActionState::Failed(e)),
            }
        });
    };
    let run_provider = move |_| {
        let tok = get_token();
        set_provider_state.set(ActionState::Running);
        spawn_local(async move {
            match api::admin_provider_sync(i64::MAX, &tok).await {
                Ok(_) => set_provider_state.set(ActionState::Done(d().admin_started.into())),
                Err(e) => set_provider_state.set(ActionState::Failed(e)),
            }
        });
    };

    view! {
        <Title text=move || d().title_admin />
        <div class="max-w-3xl mx-auto px-4 py-8">
            <h1 class="text-3xl font-bold text-stone-100 mb-2">{move || d().admin_h1}</h1>
            <p class="text-stone-400 mb-3">{move || d().admin_intro}</p>

            {move || {
                let has_warn = smtp_warn.get() || tmdb_warn.get() || omdb_warn.get();
                if !has_warn { return view!{ <div /> }.into_any(); }
                let items: Vec<(&str, &str)> = vec![
                    ("SMTP_HOST / SMTP_USER", d().admin_warn_smtp),
                    ("TMDB_API_KEY", d().admin_warn_tmdb),
                    ("OMDB_API_KEY", d().admin_warn_omdb),
                ];
                let warnings: Vec<_> = [
                    (smtp_warn.get(), items[0]),
                    (tmdb_warn.get(), items[1]),
                    (omdb_warn.get(), items[2]),
                ]
                .into_iter()
                .filter(|(active, _)| *active)
                .map(|(_, (var, msg))| view! {
                    <div class="flex gap-3 text-sm">
                        <span class="text-yellow-500 flex-shrink-0">"⚠"</span>
                        <div>
                            <span class="font-mono text-yellow-400 text-xs">{var}</span>
                            <span class="text-stone-400 text-xs ml-2">{msg}</span>
                        </div>
                    </div>
                })
                .collect();
                view! {
                    <div class="mb-6 p-4 bg-yellow-950 border border-yellow-800 rounded-lg space-y-2">
                        {warnings}
                    </div>
                }.into_any()
            }}

                <div class="mb-4 p-3 bg-sc-panel rounded border border-sc-border text-xs text-stone-400 flex gap-4 items-center"><span class="uppercase tracking-widest">{move || d().admin_log_rotation}</span><span class="text-stone-200">{move || log_rotation.get()}</span><span class="text-stone-600">{move || d().admin_log_rotation_hint}</span></div>
            <div class="mb-6 p-4 bg-sc-panel rounded border border-sc-accent-border">
                <div class="flex items-start justify-between gap-4">
                    <div class="flex-1 min-w-0">
                        <p class="text-sc-accent-hover font-semibold text-sm">{move || d().admin_seed_title}</p>
                        <p class="text-xs text-stone-400 mt-0.5">{move || d().admin_seed_desc}</p>
                    </div>
                    <div class="flex items-center gap-3 flex-shrink-0">
                        <AdminStatus state=seed_state.into() />
                        <button class="px-3 py-1.5 bg-sc-accent-bg hover:bg-sc-accent-bg-hover text-stone-100 text-xs rounded disabled:opacity-50"
                            disabled=move || seed_state.get() == ActionState::Running
                            on:click=run_seed>{move || d().admin_seed_btn}</button>
                    </div>
                </div>
            </div>

            <div class="mb-4 p-4 bg-sc-panel rounded border border-sc-border">
                <div class="flex items-start justify-between gap-4">
                    <div class="flex-1 min-w-0">
                        <p class="text-stone-200 font-semibold text-sm">{move || d().admin_sync_title}</p>
                        <p class="text-xs text-stone-400 mt-0.5">{move || d().admin_sync_desc}</p>
                    </div>
                    <div class="flex items-center gap-3 flex-shrink-0">
                        <AdminStatus state=sync_state.into() />
                        <button class="px-3 py-1.5 bg-sc-accent-bg hover:bg-sc-accent-bg-hover text-stone-100 text-xs rounded disabled:opacity-50"
                            disabled=move || sync_state.get() == ActionState::Running
                            on:click=run_sync>{move || d().admin_sync_btn}</button>
                    </div>
                </div>
            </div>

            <div class="mb-4 p-4 bg-sc-panel rounded border border-sc-border">
                <div class="flex items-start justify-between gap-4">
                    <div class="flex-1 min-w-0">
                        <p class="text-stone-200 font-semibold text-sm">{move || d().admin_enrich_title}</p>
                        <p class="text-xs text-stone-400 mt-0.5">{move || d().admin_enrich_desc}</p>
                        <div class="flex items-center gap-2 mt-2">
                            <label class="text-xs text-stone-500">{move || d().admin_enrich_limit}</label>
                            <input type="number" min="1" max="50000"
                                class="w-24 bg-sc-card border border-sc-border-input text-stone-200 text-xs rounded px-2 py-1"
                                prop:value=move || enrich_limit.get().to_string()
                                on:change=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<i64>() {
                                        set_enrich_limit.set(v.clamp(1, 50_000));
                                    }
                                }
                            />
                        </div>
                    </div>
                    <div class="flex items-center gap-3 flex-shrink-0">
                        <AdminStatus state=enrich_state.into() />
                        <button class="px-3 py-1.5 bg-sc-accent-bg hover:bg-sc-accent-bg-hover text-stone-100 text-xs rounded disabled:opacity-50"
                            disabled=move || enrich_state.get() == ActionState::Running
                            on:click=run_enrich>{move || d().admin_enrich_btn}</button>
                    </div>
                </div>
            </div>

            <div class="mb-8 p-4 bg-sc-panel rounded border border-sc-border">
                <div class="flex items-start justify-between gap-4">
                    <div class="flex-1 min-w-0">
                        <p class="text-stone-200 font-semibold text-sm">{move || d().admin_score_title}</p>
                        <p class="text-xs text-stone-400 mt-0.5">{move || d().admin_score_desc}</p>
                    </div>
                    <div class="flex items-center gap-3 flex-shrink-0">
                        <AdminStatus state=score_state.into() />
                        <button class="px-3 py-1.5 bg-sc-accent-bg hover:bg-sc-accent-bg-hover text-stone-100 text-xs rounded disabled:opacity-50"
                            disabled=move || score_state.get() == ActionState::Running
                            on:click=run_score>{move || d().admin_score_btn}</button>
                    </div>
                </div>
            </div>

            <div class="mb-8 p-4 bg-sc-panel rounded border border-sc-border">
                <div class="flex items-start justify-between gap-4">
                    <div class="flex-1 min-w-0">
                        <p class="text-stone-200 font-semibold text-sm">{move || d().admin_providers_title}</p>
                        <p class="text-xs text-stone-400 mt-0.5">{move || d().admin_providers_desc}</p>
                    </div>
                    <div class="flex items-center gap-3 flex-shrink-0">
                        <AdminStatus state=provider_state.into() />
                        <button class="px-3 py-1.5 bg-sc-accent-bg hover:bg-sc-accent-bg-hover text-stone-100 text-xs rounded disabled:opacity-50"
                            disabled=move || provider_state.get() == ActionState::Running
                            on:click=run_provider>{move || d().admin_providers_btn}</button>
                    </div>
                </div>
            </div>

            <div>
                <div class="flex items-center justify-between mb-3">
                    <h2 class="text-lg font-semibold text-stone-200">{move || d().admin_run_logs}</h2>
                    <button class="text-xs text-stone-400 hover:text-stone-200 px-2 py-1 bg-sc-card rounded border border-sc-border"
                        on:click=move |_| fetch_logs()>{move || d().admin_refresh}</button>
                </div>
                {move || if logs_loading.get() {
                    view!{ <p class="text-stone-500 text-sm">{move || d().admin_loading}</p> }.into_any()
                } else if logs.get().is_empty() {
                    view!{ <p class="text-stone-500 text-sm">{move || d().admin_no_logs}</p> }.into_any()
                } else {
                    view!{
                        <div class="space-y-1 font-mono text-xs max-h-96 overflow-y-auto">
                            {move || logs.get().into_iter().map(|entry| {
                                let level = entry.get("level").and_then(|v| v.as_str()).unwrap_or("info").to_string();
                                let event = entry.get("event").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let msg   = entry.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let ts    = entry.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let color = match level.as_str() {
                                    "error" => "text-red-400",
                                    "warn"  => "text-yellow-400",
                                    _       => "text-stone-400",
                                };
                                view!{
                                    <div class="flex gap-2 py-0.5 border-b border-sc-border">
                                        <span class="text-stone-600 shrink-0">{ts}</span>
                                        <span class={format!("shrink-0 {}", color)}>{level}</span>
                                        <span class="text-stone-500 shrink-0">{event}</span>
                                        <span class="text-stone-300 truncate">{msg}</span>
                                    </div>
                                }
                            }).collect::<Vec<_>>()}
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
}

// ── Admin status chip ─────────────────────────────────────────────────────────
#[component]
fn AdminStatus(state: Signal<ActionState>) -> impl IntoView {
    let lang = use_lang();
    let d = move || dict(lang.get());
    view! {
        <span class="text-xs max-w-xs truncate"
            class:text-stone-500={move || state.get() == ActionState::Idle}
            class:text-yellow-400={move || state.get() == ActionState::Running}
            class:animate-pulse={move || state.get() == ActionState::Running}
            class:text-sc-accent={move || matches!(state.get(), ActionState::Done(_))}
            class:text-red-400={move || matches!(state.get(), ActionState::Failed(_))}>
            {move || match state.get() {
                ActionState::Idle      => String::new(),
                ActionState::Running   => d().admin_running.to_string(),
                ActionState::Done(msg) => msg,
                ActionState::Failed(e) => format!("✗ {}", e),
            }}
        </span>
    }
}

// ── Watchlist page ────────────────────────────────────────────────────────────

#[derive(Clone)]
struct WatchlistItem {
    movie: Movie,
    state: WatchState,
}

#[component]
pub fn WatchlistPage() -> impl IntoView {
    let auth = use_context::<RwSignal<Option<AuthState>>>().expect("auth context missing");

    let (items, set_items) = signal(Vec::<WatchlistItem>::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (show_want, set_show_want) = signal(true);
    let (show_watched, set_show_watched) = signal(false);
    let lang = use_lang();
    let d = move || dict(lang.get());

    Effect::new(move |_| match auth.get() {
        None => {
            set_items.set(vec![]);
            set_loading.set(false);
        }
        Some(a) => {
            let token = a.token.clone();
            set_loading.set(true);
            set_error.set(None);
            spawn_local(async move {
                match api::get_watchlist(&token).await {
                    Err(e) => {
                        set_error.set(Some(e));
                        set_loading.set(false);
                    }
                    Ok(entries) => {
                        // Fetch all movies concurrently — a 30-film list costs one
                        // round-trip of latency instead of thirty.
                        let futs = entries
                            .iter()
                            .filter(|e| e.state != WatchState::NotInterested)
                            .map(|entry| {
                                let encoded = encode_movie_id(entry.movie_id);
                                let state = entry.state.clone();
                                async move {
                                    api::fetch_movie(&encoded)
                                        .await
                                        .ok()
                                        .map(|movie| WatchlistItem { movie, state })
                                }
                            })
                            .collect::<Vec<_>>();
                        let results: Vec<WatchlistItem> = futures::future::join_all(futs)
                            .await
                            .into_iter()
                            .flatten()
                            .collect();
                        set_items.set(results);
                        set_loading.set(false);
                    }
                }
            });
        }
    });

    view! {
        <Title text=move || d().title_watchlist />
        <div class="max-w-7xl mx-auto px-4 py-8">
            <div class="mb-6">
                <h1 class="text-4xl font-bold text-stone-100 mb-1">{move || d().watchlist_h1}</h1>
                <p class="text-stone-400">{move || d().watchlist_desc}</p>
            </div>

            // ── Status filter checkboxes ──────────────────────────────────────
            {move || (!loading.get() && auth.get().is_some() && !items.get().is_empty()).then(|| view! {
                <div class="flex items-center gap-4 mb-6">
                    <label class="flex items-center gap-2 cursor-pointer select-none">
                        <input
                            type="checkbox"
                            prop:checked=move || show_want.get()
                            on:change=move |_| set_show_want.update(|v| *v = !*v)
                            class="accent-sc-accent w-4 h-4 cursor-pointer"
                        />
                        <span class="text-sm text-stone-300">{move || d().watchlist_cb_want}</span>
                    </label>
                    <label class="flex items-center gap-2 cursor-pointer select-none">
                        <input
                            type="checkbox"
                            prop:checked=move || show_watched.get()
                            on:change=move |_| set_show_watched.update(|v| *v = !*v)
                            class="accent-sc-accent w-4 h-4 cursor-pointer"
                        />
                        <span class="text-sm text-stone-300">{move || d().watchlist_cb_watched}</span>
                    </label>
                </div>
            })}

            {move || {
                if auth.get().is_none() {
                    return view! {
                        <div class="py-16 text-center">
                            <p class="text-stone-400 mb-4">{move || d().watchlist_signin_prompt}</p>
                            <A href="/signin"
                                attr:class="text-sc-accent hover:text-sc-accent-hover border border-sc-accent-border rounded px-4 py-2 text-sm">
                                {move || d().watchlist_signin_btn}
                            </A>
                        </div>
                    }.into_any();
                }
                if loading.get() {
                    return view! {
                        <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                            {(0..10).map(|_| view!{ <SkeletonCard /> }).collect::<Vec<_>>()}
                        </div>
                    }.into_any();
                }
                if let Some(err) = error.get() {
                    return view! {
                        <div class="py-16 text-center"><p class="text-red-400">{err}</p></div>
                    }.into_any();
                }
                let its: Vec<WatchlistItem> = items.get()
                    .into_iter()
                    .filter(|item| match item.state {
                        WatchState::WantToWatch => show_want.get(),
                        WatchState::Watched => show_watched.get(),
                        WatchState::NotInterested => false,
                    })
                    .collect();
                if its.is_empty() {
                    return view! {
                        <div class="py-16 text-center text-stone-500">
                            {move || d().watchlist_empty}
                        </div>
                    }.into_any();
                }
                view! {
                    <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 gap-4">
                        {its.into_iter().map(|item| view!{ <WatchlistCard item=item /> }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

// ── Watchlist card ────────────────────────────────────────────────────────────
#[component]
fn WatchlistCard(item: WatchlistItem) -> impl IntoView {
    let navigate = use_navigate();
    let lang = use_lang();
    let d = move || dict(lang.get());
    let href = format!("/movie/{}", encode_movie_id(item.movie.id.unwrap_or(0)));
    let href_nav = href.clone();
    let poster = item.movie.poster_url.clone().unwrap_or_default();
    let has_poster = !poster.is_empty();
    let title = item.movie.title.clone();
    let year = item.movie.year.map(|y| y.to_string()).unwrap_or_default();
    let director = item.movie.director.clone().unwrap_or_default();
    let imdb = item.movie.imdb_rating.map(|r| format!("{:.1}", r));
    let gem_score = item.movie.gem_score.map(|s| format!("{:.0}", s * 100.0));

    let state = item.state.clone();
    let badge_class = match &state {
        WatchState::WantToWatch => "bg-sc-accent-deep border-sc-accent text-sc-accent",
        WatchState::Watched => "bg-green-950 border-green-700 text-green-400",
        WatchState::NotInterested => "bg-stone-800 border-stone-600 text-stone-400",
    };
    let badge_label = move || match &state {
        WatchState::WantToWatch => d().wl_want,
        WatchState::Watched => d().wl_watched,
        WatchState::NotInterested => d().wl_not_interested,
    };

    view! {
        <a
            href=href
            class="group block bg-sc-card rounded overflow-hidden hover:ring-1 hover:ring-sc-accent-bg transition-all duration-200 cursor-pointer relative"
            on:click=move |ev: web_sys::MouseEvent| {
                if !ev.meta_key() && !ev.ctrl_key() && !ev.shift_key() && ev.button() == 0 {
                    ev.prevent_default();
                    navigate(&href_nav, Default::default());
                }
            }
        >
            <div class="overflow-hidden relative" style="aspect-ratio:2/3">
                {if has_poster {
                    view!{ <img src=poster alt=title.clone() loading="lazy"
                        class="w-full h-full object-cover group-hover:scale-105 transition-transform duration-300" /> }.into_any()
                } else {
                    view!{ <div class="w-full h-full bg-sc-border flex items-center justify-center">
                        <span class="text-5xl">"🎬"</span></div> }.into_any()
                }}
                <div class={format!("absolute bottom-1.5 left-1.5 px-1.5 py-0.5 text-xs rounded border {}", badge_class)}>
                    {move || badge_label()}
                </div>
            </div>
            <div class="p-3">
                <h3 class="text-stone-100 font-medium text-sm leading-snug line-clamp-2 mb-1">{title}</h3>
                <div class="flex items-center justify-between text-xs mb-0.5">
                    <span class="text-stone-400 tabular-nums">{year}</span>
                    <div class="flex flex-wrap justify-end gap-x-1.5 gap-y-0.5 items-center tabular-nums">
                        {gem_score.map(|s| { let sa = s.clone(); view!{ <span class="text-sc-accent font-semibold whitespace-nowrap" title=move || d().card_gem_tooltip aria-label=move || d().card_gem_aria.replace("{}", &sa)>"✦ "{s}</span> } })}
                        {imdb.map(|r| { let ra = r.clone(); view!{ <span class="text-yellow-400 whitespace-nowrap" title=move || d().card_rating_tooltip aria-label=move || d().card_rating_aria.replace("{}", &ra)>"★ "{r}</span> } })}
                    </div>
                </div>
                {if !director.is_empty() {
                    view!{ <p class="text-xs text-stone-500 mt-0.5 truncate">{director}</p> }.into_any()
                } else { view!{ <span /> }.into_any() }}
            </div>
        </a>
    }
}

// ── Privacy Policy page ───────────────────────────────────────────────────────
#[component]
pub fn PrivacyPage() -> impl IntoView {
    let lang = use_lang();
    let d = move || dict(lang.get());
    view! {
        <Title text=move || d().title_privacy />
        <div class="max-w-3xl mx-auto px-4 py-12 text-stone-300">
            <h1 class="font-display text-4xl tracking-widest text-stone-100 mb-2">{move || d().privacy_h1}</h1>
            <p class="text-stone-500 text-sm mb-10">{move || d().privacy_dates}</p>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().privacy_who_h}</h2>
                <p class="text-stone-400 leading-relaxed">
                    {move || d().privacy_who_p1}
                    <a href="mailto:rk@rkzwei.dev" class="text-sc-accent hover:text-sc-accent-hover transition-colors">
                        "rk@rkzwei.dev"
                    </a>
                    "."
                </p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().privacy_collect_h}</h2>
                <p class="text-stone-400 leading-relaxed mb-4">{move || d().privacy_collect_p1}</p>
                <p class="text-stone-400 leading-relaxed mb-4">{move || d().privacy_collect_p2}</p>
                <p class="text-stone-400 leading-relaxed">{move || d().privacy_collect_p3}</p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().privacy_session_h}</h2>
                <p class="text-stone-400 leading-relaxed mb-4">
                    {move || d().privacy_session_p1a}
                    <em>{move || d().privacy_session_em}</em>
                    {move || d().privacy_session_p1b}
                </p>
                <p class="text-stone-400 leading-relaxed">{move || d().privacy_session_p2}</p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().privacy_providers_h}</h2>
                <p class="text-stone-400 leading-relaxed">{move || d().privacy_providers_p}</p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().privacy_where_h}</h2>
                <p class="text-stone-400 leading-relaxed">{move || d().privacy_where_p}</p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().privacy_retention_h}</h2>
                <p class="text-stone-400 leading-relaxed">{move || d().privacy_retention_p}</p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().privacy_legal_h}</h2>
                <p class="text-stone-400 leading-relaxed mb-4">{move || d().privacy_legal_p1}</p>
                <p class="text-stone-400 leading-relaxed">{move || d().privacy_legal_p2}</p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().privacy_rights_h}</h2>
                <p class="text-stone-400 leading-relaxed mb-4">
                    {move || d().privacy_rights_p1}
                    <a href="mailto:rk@rkzwei.dev" class="text-sc-accent hover:text-sc-accent-hover transition-colors">
                        "rk@rkzwei.dev"
                    </a>
                    "."
                </p>
                <p class="text-stone-400 leading-relaxed">{move || d().privacy_rights_p2}</p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().privacy_changes_h}</h2>
                <p class="text-stone-400 leading-relaxed">{move || d().privacy_changes_p}</p>
            </section>
        </div>
    }
}

// ── About page ────────────────────────────────────────────────────────────────
#[component]
pub fn AboutPage() -> impl IntoView {
    let lang = use_lang();
    let d = move || dict(lang.get());
    view! {
        <Title text=move || d().title_about />
        <div class="max-w-3xl mx-auto px-4 py-12 text-stone-300">
            <h1 class="font-display text-4xl tracking-widest text-stone-100 mb-2">{move || d().about_h1}</h1>
            <p class="text-stone-500 text-sm mb-10">{move || d().about_subtitle}</p>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().about_gem_h}</h2>
                <p class="text-stone-400 leading-relaxed mb-4">{move || d().about_gem_p1}</p>
                <p class="text-stone-400 leading-relaxed mb-4">{move || d().about_gem_p2}</p>
                <p class="text-stone-400 leading-relaxed">{move || d().about_gem_p3}</p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().about_acclaimed_h}</h2>
                <p class="text-stone-400 leading-relaxed">{move || d().about_acclaimed_p}</p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().about_wildcards_h}</h2>
                <p class="text-stone-400 leading-relaxed">{move || d().about_wildcards_p}</p>
            </section>

            <section class="mb-8">
                <h2 class="text-stone-100 font-semibold text-lg mb-3">{move || d().about_name_h}</h2>
                <p class="text-stone-400 leading-relaxed">
                    <em>"Pedrilha"</em>
                    {move || d().about_name_1}
                    <em>"pedra"</em>
                    {move || d().about_name_2}
                    <em>"Se Essa Rua Fosse Minha"</em>
                    {move || d().about_name_3}
                    <em>"com pedrinhas de brilhantes"</em>
                    {move || d().about_name_4}
                </p>
            </section>
        </div>
    }
}

// ── Changelog page ────────────────────────────────────────────────────────────
fn changelog_clean_item(s: &str) -> String {
    // strip trailing " ([hash](url))" commit reference
    let s = if let Some(idx) = s.rfind(" ([") {
        &s[..idx]
    } else {
        s
    };
    // strip ** bold markers (scope prefixes like **analytics:**)
    s.replace("**", "")
}

fn changelog_clean_header(s: &str) -> String {
    // "[0.2.0](url) (date)" or "[0.1.0] - date"  →  "v0.2.0 (date)" / "v0.1.0 - date"
    if !s.starts_with('[') {
        return s.to_string();
    }
    let close = match s.find(']') {
        Some(i) => i,
        None => return s.to_string(),
    };
    let version = &s[1..close];
    let after = &s[close + 1..];
    let tail = if after.starts_with('(') {
        match after.find(')') {
            Some(i) => after[i + 1..].trim_start(),
            None => after,
        }
    } else {
        after.trim_start()
    };
    format!("v{version} {tail}")
}

#[component]
pub fn ChangelogPage() -> impl IntoView {
    let lang = use_lang();
    let d = move || dict(lang.get());
    const RAW: &str = include_str!("../../../CHANGELOG.md");

    let nodes: Vec<_> = RAW
        .lines()
        .map(|line| {
            if let Some(rest) = line.strip_prefix("## ") {
                let text = changelog_clean_header(rest);
                view! {
                    <h2 class="text-lg font-bold text-stone-100 mt-8 mb-2 border-b border-sc-border pb-1">
                        {text}
                    </h2>
                }
                .into_any()
            } else if let Some(rest) = line.strip_prefix("### ") {
                view! {
                    <h3 class="text-xs font-semibold text-sc-accent uppercase tracking-widest mt-4 mb-1">
                        {rest.to_string()}
                    </h3>
                }
                .into_any()
            } else if line.starts_with("* ") || line.starts_with("- ") {
                let text = changelog_clean_item(&line[2..]);
                view! {
                    <li class="text-stone-400 text-sm ml-4 list-disc">
                        {text}
                    </li>
                }
                .into_any()
            } else if line.starts_with("# ") || line.is_empty() {
                view! { <span /> }.into_any()
            } else {
                view! { <p class="text-stone-500 text-sm mt-1">{line.to_string()}</p> }.into_any()
            }
        })
        .collect();

    view! {
        <Title text=move || d().title_changelog />
        <div class="max-w-2xl mx-auto px-4 py-12">
            <h1 class="text-3xl font-bold text-stone-100 mb-1">{move || d().changelog_h1}</h1>
            <p class="text-stone-500 text-sm mb-8">{move || d().changelog_desc}</p>
            <ul>{nodes}</ul>
        </div>
    }
}
