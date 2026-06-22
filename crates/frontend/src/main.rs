use leptos::prelude::*;
use leptos_meta::*;

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
        <Title text="Gem Finder" />
        <Meta name="description" content="Discover hidden gem movies" />

        <div class="min-h-screen bg-gray-950 text-gray-100">
            <header class="bg-gray-900 border-b border-gray-800">
                <nav class="max-w-7xl mx-auto px-4 py-4 flex justify-between items-center">
                    <h1 class="text-2xl font-bold">"💎 Gem Finder"</h1>
                    <a href="/" class="text-gray-300 hover:text-white">"Gems"</a>
                </nav>
            </header>

            <main>
                <pages::HomePage />
            </main>

            <footer class="bg-gray-900 border-t border-gray-800 py-8 text-center text-gray-400">
                <p>"Gem Finder — Discovering hidden cinematic treasures"</p>
            </footer>
        </div>
    }
}
