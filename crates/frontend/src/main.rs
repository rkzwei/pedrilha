use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes, A},
    path,
};

mod api;
mod components;
mod pages;

fn main() {
    mount_to_body(|| view! { <App /> })
}

#[component]
fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Router>
            <Title text="Gem Finder" />
            <Meta name="description" content="Discover hidden gem movies" />

            <div class="min-h-screen bg-[#0d0906] text-stone-200">
                <header class="bg-[#17100a] border-b border-[#2b1e14] sticky top-0 z-10">
                    <nav class="max-w-7xl mx-auto px-4 py-4 flex justify-between items-center">
                        <A href="/" attr:class="font-display text-3xl tracking-widest text-stone-100 hover:text-orange-600 transition-colors">
                            "GEM FINDER"
                        </A>
                        <div class="flex gap-6">
                            <A href="/" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-sm tracking-wide">
                                "GEMS"
                            </A>
                            <A href="/acclaimed" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-sm tracking-wide">
                                "ACCLAIMED"
                            </A>
                            <A href="/wildcards" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-sm tracking-wide">
                                "WILDCARDS"
                            </A>
                            <A href="/admin" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-sm tracking-wide">
                                "ADMIN"
                            </A>
                        </div>
                    </nav>
                </header>

                <main>
                    <Routes fallback=|| view! {
                        <div class="max-w-7xl mx-auto px-4 py-16 text-center">
                            <p class="text-4xl mb-4">"404"</p>
                            <p class="text-stone-400 mb-8">"Page not found"</p>
                            <A href="/" attr:class="text-orange-600 hover:text-orange-500">"← Back to Gems"</A>
                        </div>
                    }>
                        <Route path=path!("/") view=pages::HomePage />
                        <Route path=path!("/acclaimed") view=pages::AcclaimedPage />
                        <Route path=path!("/wildcards") view=pages::WildcardsPage />
                        <Route path=path!("/movie/:id") view=pages::MovieDetail />
                        <Route path=path!("/admin") view=pages::AdminPage />
                    </Routes>
                </main>

                <footer class="bg-[#17100a] border-t border-[#2b1e14] py-8 text-center text-stone-600 text-sm">
                    <p>"Gem Finder — Unearthing what the blockbusters buried."</p>
                    <p class="text-xs text-stone-700 mt-2 tracking-widest">
                        "// "
                        <a href="https://www.imdb.com/title/tt0076740/"
                           target="_blank" rel="noopener noreferrer"
                           class="hover:text-orange-700 transition-colors">
                            "SORCERER, 1977 — WILLIAM FRIEDKIN"
                        </a>
                    </p>
                </footer>
            </div>
        </Router>
    }
}
