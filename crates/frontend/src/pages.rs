use crate::api;
use gem_finder_shared::types::MovieSummary;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// Home page - displays a grid of hidden gem movies.
#[component]
pub fn HomePage() -> impl IntoView {
    // Resource::new requires Send in Leptos 0.7 but reqwest WASM futures are !Send.
    // Use a plain signal + spawn_local instead.
    let (movies, set_movies) = signal(Vec::<MovieSummary>::new());
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(Option::<String>::None);

    spawn_local(async move {
        match api::fetch_gems(1, 20, None, None).await {
            Ok(response) => {
                set_movies.set(response.data);
                set_loading.set(false);
            }
            Err(e) => {
                set_error.set(Some(e));
                set_loading.set(false);
            }
        }
    });

    view! {
        <div class="max-w-7xl mx-auto px-4 py-8">
            <div class="mb-8">
                <h1 class="text-4xl font-bold text-white mb-2">"Hidden Gems"</h1>
                <p class="text-gray-400 text-lg">
                    "Movies rated in the 6.5–7.9 sweet spot that deserve your attention."
                </p>
            </div>

            <div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-6">
                {move || {
                    if loading.get() {
                        view! { <p class="text-gray-400 col-span-full">"Loading..."</p> }.into_any()
                    } else if let Some(err) = error.get() {
                        view! {
                            <div class="col-span-full text-center py-12">
                                <p class="text-red-400">{err}</p>
                            </div>
                        }.into_any()
                    } else {
                        movies.get()
                            .into_iter()
                            .map(|movie| view! { <MovieCard movie /> })
                            .collect::<Vec<_>>()
                            .into_any()
                    }
                }}
            </div>
        </div>
    }
}

/// A single movie card in the grid.
#[component]
fn MovieCard(movie: MovieSummary) -> impl IntoView {
    // poster_url is already an absolute URL built by tmdb_sync (e.g. "https://image.tmdb.org/t/p/w500/...")
    // Do NOT prepend another base — that was producing double-URL garbage.
    let poster_url = movie.poster_url.unwrap_or_default();

    // gem_score is normalised [0, 1] — display as a percentage so it doesn't look like a star rating.
    let gem_score = movie
        .gem_score
        .map(|s| format!("{:.0}%", s * 100.0))
        .unwrap_or_default();
    let year_text = movie.year.map(|y| y.to_string()).unwrap_or_default();

    view! {
        <div class="bg-gray-800 rounded-lg overflow-hidden shadow-lg hover:shadow-xl transition-shadow duration-300">
            <img
                src=poster_url
                alt=movie.title.clone()
                class="w-full h-64 object-cover"
            />
            <div class="p-4">
                <h3 class="text-white font-semibold text-lg truncate">{movie.title}</h3>
                <div class="flex justify-between items-center mt-2">
                    <span class="text-gray-400 text-sm">{year_text}</span>
                    <div class="flex items-center space-x-1">
                        <span class="text-emerald-400 text-xs font-semibold">"GEM"</span>
                        <span class="text-white font-medium">{gem_score}</span>
                    </div>
                </div>
                {movie.imdb_rating.map(|r| {
                    view! {
                        <div class="mt-1 text-xs text-gray-500">
                            "IMDb: " {format!("{:.1}", r)}
                        </div>
                    }
                })}
            </div>
        </div>
    }
}
