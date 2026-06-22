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

            <div class="min-h-screen bg-gray-950 text-gray-100">
                <header class="bg-gray-900 border-b border-gray-800 sticky top-0 z-10">
                    <nav class="max-w-7xl mx-auto px-4 py-4 flex justify-between items-center">
                        <A href="/" attr:class="text-2xl font-bold hover:text-emerald-400 transition-colors">
                            "💎 Gem Finder"
                        </A>
                        <div class="flex gap-6">
                            <A href="/" attr:class="text-gray-300 hover:text-white transition-colors">
                                "Gems"
                            </A>
                            <A href="/acclaimed" attr:class="text-gray-300 hover:text-white transition-colors">
                                "Acclaimed"
                            </A>
                        </div>
                    </nav>
                </header>

                <main>
                    <Routes fallback=|| view! {
                        <div class="max-w-7xl mx-auto px-4 py-16 text-center">
                            <p class="text-4xl mb-4">"404"</p>
                            <p class="text-gray-400 mb-8">"Page not found"</p>
                            <A href="/" attr:class="text-emerald-400 hover:text-emerald-300">"← Back to Gems"</A>
                        </div>
                    }>
                        <Route path=path!("/") view=pages::HomePage />
                        <Route path=path!("/acclaimed") view=pages::AcclaimedPage />
                        <Route path=path!("/movie/:id") view=pages::MovieDetail />
                    </Routes>
                </main>

                <footer class="bg-gray-900 border-t border-gray-800 py-8 text-center text-gray-500 text-sm">
                    <p>"Gem Finder — Discovering hidden cinematic treasures"</p>
                </footer>
            </div>
        </Router>
    }
}
