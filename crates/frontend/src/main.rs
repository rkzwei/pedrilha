use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes, A},
    path,
};

mod api;
mod pages;

#[derive(Clone, Debug)]
pub struct AuthState {
    pub token: String,
    pub user_id: String,
    pub email: String,
    pub username: Option<String>,
    pub is_admin: bool,
}

/// Decode the `is_admin` claim from a JWT without a crypto library.
/// JWTs are header.payload.signature — the payload is base64url JSON.
pub fn jwt_is_admin(token: &str) -> bool {
    let payload = match token.split('.').nth(1) {
        Some(p) => p,
        None => return false,
    };
    // base64url → base64: replace URL-safe chars and add padding
    let b64 = payload.replace('-', "+").replace('_', "/");
    let pad = (4 - b64.len() % 4) % 4;
    let b64 = format!("{}{}", b64, "=".repeat(pad));

    let decoded = web_sys::window()
        .and_then(|w| w.atob(&b64).ok())
        .unwrap_or_default();

    serde_json::from_str::<serde_json::Value>(&decoded)
        .ok()
        .and_then(|v| v.get("is_admin").and_then(|b| b.as_bool()))
        .unwrap_or(false)
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
    let is_admin = jwt_is_admin(&token);
    Some(AuthState {
        token,
        user_id,
        email,
        username,
        is_admin,
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
    console_error_panic_hook::set_once();
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id("app-loading") {
            el.remove();
        }
    }
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
            <Title text="Pedrilha" />
            <Meta name="description" content="Discover hidden gem movies" />

            <div class="min-h-screen bg-sc-base text-stone-200">
                <header class="border-b border-sc-border sticky top-0 z-[60]" style="background-color: rgba(23,16,10,0.88); backdrop-filter: blur(10px); -webkit-backdrop-filter: blur(10px);">
                    <nav class="max-w-7xl mx-auto px-4 py-4 flex flex-col sm:flex-row sm:justify-between items-center gap-3 sm:gap-0">
                        <A href="/" attr:class="font-display text-2xl sm:text-3xl tracking-widest text-stone-100 hover:text-sc-accent transition-colors">
                            "PEDRILHA"
                        </A>
                        <div class="flex flex-wrap justify-center gap-x-4 gap-y-1 sm:gap-6 items-center">
                            <A href="/" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide aria-[current=page]:text-sc-accent aria-[current=page]:border-b-2 aria-[current=page]:border-sc-accent">
                                "GEMS"
                            </A>
                            <A href="/acclaimed" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide aria-[current=page]:text-sc-accent aria-[current=page]:border-b-2 aria-[current=page]:border-sc-accent">
                                "ACCLAIMED"
                            </A>
                            <A href="/wildcards" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide aria-[current=page]:text-sc-accent aria-[current=page]:border-b-2 aria-[current=page]:border-sc-accent">
                                "WILDCARDS"
                            </A>
                            {move || auth.get().filter(|a| a.is_admin).map(|_| view! {
                                <A href="/admin" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide aria-[current=page]:text-sc-accent aria-[current=page]:border-b-2 aria-[current=page]:border-sc-accent">
                                    "ADMIN"
                                </A>
                            })}
                            {move || match auth.get() {
                                Some(a) => {
                                    let display = a.username.clone()
                                        .unwrap_or_else(|| {
                                            a.email.split('@').next().unwrap_or("user").to_string()
                                        });
                                    view! {
                                        <A href="/watchlist" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide aria-[current=page]:text-sc-accent aria-[current=page]:border-b-2 aria-[current=page]:border-sc-accent">
                                            "WATCHLIST"
                                        </A>
                                        <span class="hidden md:inline text-stone-500 text-xs sm:text-sm">{display}</span>
                                        <button
                                            class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide"
                                            on:click=move |_| logout(auth)
                                        >
                                            "SIGN OUT"
                                        </button>
                                    }.into_any()
                                }
                                None => {
                                    if smtp_ok.get() {
                                        view! {
                                            <A href="/signin" attr:class="text-stone-400 hover:text-stone-100 transition-colors text-xs sm:text-sm tracking-wide">
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
                        <div class="max-w-7xl mx-auto px-4 py-24 text-center">
                            <Title text="Not found — Pedrilha" />
                            <h1 class="font-display text-4xl sm:text-6xl tracking-wide text-stone-100 mb-4">"404 — NOT IN THE VAULT"</h1>
                            <p class="text-stone-400 mb-8">"This reel doesn't exist."</p>
                            <A href="/" attr:class="text-sc-accent hover:text-sc-accent-hover">"← Back to the gems"</A>
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
                        <Route path=path!("/privacy") view=pages::PrivacyPage />
                        <Route path=path!("/changelog") view=pages::ChangelogPage />
                        <Route path=path!("/about") view=pages::AboutPage />
                    </Routes>
                </main>

                <footer class="bg-sc-panel border-t border-sc-border py-8 text-center text-stone-600 text-sm">
                    <p>"Pedrilha — Unearthing what the blockbusters buried."</p>
                    <p class="text-xs text-stone-700 mt-2 tracking-widest">
                        "// "
                        <a href="https://www.imdb.com/title/tt0076740/"
                           target="_blank" rel="noopener noreferrer"
                           class="hover:text-sc-accent-dim transition-colors">
                            "SORCERER, 1977 — WILLIAM FRIEDKIN"
                        </a>
                    </p>
                    <p class="text-xs text-stone-700 mt-3">
                        <A href="/about" attr:class="hover:text-stone-500 transition-colors">
                            "How it works"
                        </A>
                        " · "
                        <A href="/privacy" attr:class="hover:text-stone-500 transition-colors">
                            "Privacy Policy"
                        </A>
                        " · No cookies · No ads · "
                        <a href="mailto:rk@rkzwei.dev" class="hover:text-stone-500 transition-colors">
                            "rk@rkzwei.dev"
                        </a>
                        " · "
                        <A href="/changelog" attr:class="hover:text-stone-500 transition-colors">
                            {concat!("v", env!("CARGO_PKG_VERSION"))}
                        </A>
                    </p>
                </footer>
            </div>
        </Router>
    }
}
