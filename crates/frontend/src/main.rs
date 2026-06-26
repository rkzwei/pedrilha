use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes, A},
    path,
};

mod api;
mod components;
mod pages;

#[derive(Clone, Debug)]
pub struct AuthState {
    pub token: String,
    pub user_id: String,
    pub email: String,
    pub username: Option<String>,
}

const LS_TOKEN: &str = "gf_token";
const LS_USER_ID: &str = "gf_user_id";
const LS_EMAIL: &str = "gf_email";
const LS_USERNAME: &str = "gf_username";

pub fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .and_then(|s| s)
}

pub fn save_auth_to_storage(token: &str, user_id: &str, email: &str, username: Option<&str>) {
    if let Some(ls) = local_storage() {
        let _ = ls.set_item(LS_TOKEN, token);
        let _ = ls.set_item(LS_USER_ID, user_id);
        let _ = ls.set_item(LS_EMAIL, email);
        if let Some(u) = username {
            let _ = ls.set_item(LS_USERNAME, u);
        } else {
            let _ = ls.remove_item(LS_USERNAME);
        }
    }
}

pub fn load_auth_from_storage() -> Option<AuthState> {
    let ls = local_storage()?;
    let token = ls.get_item(LS_TOKEN).ok()??;
    let user_id = ls.get_item(LS_USER_ID).ok()??;
    let email = ls.get_item(LS_EMAIL).ok()??;
    let username = ls.get_item(LS_USERNAME).ok().flatten();
    if token.is_empty() {
        return None;
    }
    Some(AuthState {
        token,
        user_id,
        email,
        username,
    })
}

pub fn logout(auth: RwSignal<Option<AuthState>>) {
    if let Some(ls) = local_storage() {
        let _ = ls.remove_item(LS_TOKEN);
        let _ = ls.remove_item(LS_USER_ID);
        let _ = ls.remove_item(LS_EMAIL);
        let _ = ls.remove_item(LS_USERNAME);
    }
    auth.set(None);
}

fn main() {
    mount_to_body(|| view! { <App /> })
}

#[component]
fn App() -> impl IntoView {
    provide_meta_context();

    let auth: RwSignal<Option<AuthState>> = RwSignal::new(load_auth_from_storage());
    provide_context(auth);

    let smtp_ok: RwSignal<bool> = RwSignal::new(true);
    provide_context(smtp_ok);

    spawn_local(async move {
        if let Ok(status) = api::fetch_admin_status().await {
            let configured = status
                .get("smtp_configured")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            smtp_ok.set(configured);
        }
    });

    view! {
        <Router>
            <Title text="Gem Finder" />
            <Meta name="description" content="Discover hidden gem movies" />

            <div class="min-h-screen bg-sc-base text-stone-200">
                <header class="border-b border-sc-border sticky top-0 z-10" style="background-color: rgba(23,16,10,0.88); backdrop-filter: blur(10px); -webkit-backdrop-filter: blur(10px);">
                    <nav class="max-w-7xl mx-auto px-4 py-4 flex justify-between items-center">
                        <A href="/" attr:class="font-display text-3xl tracking-widest text-stone-100 hover:text-sc-accent transition-colors">
                            "GEM FINDER"
                        </A>
                        <div class="flex gap-6 items-center">
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
                            {move || match auth.get() {
                                Some(a) => {
                                    let display = a.username.clone()
                                        .unwrap_or_else(|| {
                                            a.email.split('@').next().unwrap_or("user").to_string()
                                        });
                                    view! {
                                        <A href="/watchlist" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-sm tracking-wide">
                                            "WATCHLIST"
                                        </A>
                                        <span class="text-stone-500 text-sm">{display}</span>
                                        <button
                                            class="text-stone-400 hover:text-stone-100 transition-colors text-sm tracking-wide"
                                            on:click=move |_| logout(auth)
                                        >
                                            "SIGN OUT"
                                        </button>
                                    }.into_any()
                                }
                                None => {
                                    if smtp_ok.get() {
                                        view! {
                                            <A href="/signin" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-sm tracking-wide">
                                                "SIGN IN"
                                            </A>
                                        }.into_any()
                                    } else {
                                        view! { <span /> }.into_any()
                                    }
                                }
                            }}
                        </div>
                    </nav>
                </header>

                <main>
                    <Routes fallback=|| view! {
                        <div class="max-w-7xl mx-auto px-4 py-16 text-center">
                            <p class="text-4xl mb-4">"404"</p>
                            <p class="text-stone-400 mb-8">"Page not found"</p>
                            <A href="/" attr:class="text-sc-accent hover:text-sc-accent-hover">"Back to Gems"</A>
                        </div>
                    }>
                        <Route path=path!("/") view=pages::HomePage />
                        <Route path=path!("/acclaimed") view=pages::AcclaimedPage />
                        <Route path=path!("/wildcards") view=pages::WildcardsPage />
                        <Route path=path!("/movie/:id") view=pages::MovieDetail />
                        <Route path=path!("/admin") view=pages::AdminPage />
                        <Route path=path!("/watchlist") view=pages::WatchlistPage />
                        <Route path=path!("/signin") view=pages::SignInPage />
                        <Route path=path!("/auth/verify") view=pages::VerifyPage />
                    </Routes>
                </main>

                <footer class="bg-sc-panel border-t border-sc-border py-8 text-center text-stone-600 text-sm">
                    <p>"Gem Finder — Unearthing what the blockbusters buried."</p>
                    <p class="text-xs text-stone-700 mt-2 tracking-widest">
                        "// "
                        <a href="https://www.imdb.com/title/tt0076740/"
                           target="_blank" rel="noopener noreferrer"
                           class="hover:text-sc-accent-dim transition-colors">
                            "SORCERER, 1977 — WILLIAM FRIEDKIN"
                        </a>
                    </p>
                </footer>
            </div>
        </Router>
    }
}
