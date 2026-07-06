//! Friend-recommendation UI (Ethos C1). All rec components live here —
//! pages.rs is already ~3k lines and must not grow with this feature.

use std::collections::BTreeMap;

use gem_finder_shared::id_encode::encode_movie_id;
use gem_finder_shared::types::{FriendInfo, RecPublic, ReceivedRec, SentRec, WatchState};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use leptos_router::NavigateOptions;
use wasm_bindgen::prelude::*;

use crate::i18n::{dict, use_lang};
use crate::{api, save_auth_to_storage, AuthState};

/// States of the recommend flow modal.
#[derive(Clone, PartialEq)]
enum RecFlow {
    Closed,
    Compose,
    NeedsUsername,
    LinkReady(String),
    Sent,
}

fn window_origin() -> String {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default()
}

/// Modal backdrop + centered panel, shared by every non-Closed flow state.
/// Clicking the backdrop (not the panel) closes; the panel stops propagation
/// so clicks inside it don't bubble to the backdrop's close handler.
fn modal_shell(
    on_close: impl Fn() + Copy + 'static,
    children: impl IntoView + 'static,
) -> impl IntoView {
    view! {
        <div
            style="position:fixed;inset:0;z-index:100;background:rgba(0,0,0,0.65);display:flex;align-items:center;justify-content:center;padding:16px"
            on:click=move |_| on_close()
        >
            <div
                style="background-color:var(--sc-panel,#17100a);border:1px solid var(--sc-border);border-radius:12px;max-width:420px;width:100%;padding:20px"
                on:click=move |ev: web_sys::MouseEvent| ev.stop_propagation()
            >
                {children}
            </div>
        </div>
    }
}

/// Username picker shown inline when a rec send hits `username_required`.
/// Owns the availability-check + save network calls; reports the confirmed
/// username back via `on_done` so the caller can update session state.
#[component]
pub fn UsernameModal(on_done: Callback<String>) -> impl IntoView {
    let lang = use_lang();
    let d = move || dict(lang.get());
    let auth = use_context::<RwSignal<Option<AuthState>>>().unwrap_or_else(|| RwSignal::new(None));

    let input = RwSignal::new(String::new());
    let avail = RwSignal::new(Option::<bool>::None);
    let checking = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let err = RwSignal::new(Option::<String>::None);
    let debounce_handle: StoredValue<Option<i32>> = StoredValue::new(None);

    let on_input = move |ev: web_sys::Event| {
        let v = event_target_value(&ev);
        input.set(v.clone());
        avail.set(None);
        err.set(None);
        if let Some(h) = debounce_handle.get_value() {
            if let Some(w) = web_sys::window() {
                w.clear_timeout_with_handle(h);
            }
        }
        if v.trim().chars().count() < 3 {
            return;
        }
        checking.set(true);
        let cb = Closure::once(move || {
            spawn_local(async move {
                match api::check_username(&v).await {
                    Ok(a) => avail.set(Some(a.available)),
                    Err(_) => avail.set(None),
                }
                checking.set(false);
            });
        });
        let handle = web_sys::window()
            .and_then(|w| {
                w.set_timeout_with_callback_and_timeout_and_arguments_0(
                    cb.as_ref().unchecked_ref::<js_sys::Function>(),
                    400,
                )
                .ok()
            })
            .unwrap_or(-1);
        cb.forget();
        debounce_handle.set_value(Some(handle));
    };

    let save = move |_: web_sys::MouseEvent| {
        let Some(a) = auth.get_untracked() else {
            return;
        };
        let name = input.get_untracked();
        if name.trim().chars().count() < 3 {
            return;
        }
        saving.set(true);
        err.set(None);
        spawn_local(async move {
            match api::set_username(&name, &a.token).await {
                Ok(_) => {
                    saving.set(false);
                    on_done.run(name);
                }
                Err(e) => {
                    err.set(Some(e));
                    saving.set(false);
                }
            }
        });
    };

    view! {
        <div>
            <h3 class="font-display text-lg text-stone-100 mb-2">{move || d().rec_username_title}</h3>
            <p class="text-sm text-stone-400 mb-3">{move || d().rec_username_body}</p>
            <input
                type="text"
                class="w-full bg-sc-card text-stone-200 border border-sc-border-input rounded px-3 py-2 text-sm focus:outline-none focus:border-sc-accent-border"
                prop:value=move || input.get()
                on:input=on_input
            />
            {move || (avail.get() == Some(false)).then(|| view! {
                <p class="text-red-400 text-xs mt-1">{move || d().rec_username_taken}</p>
            })}
            {move || err.get().map(|e| view! { <p class="text-red-400 text-xs mt-1">{e}</p> })}
            <button
                class="mt-3 px-3 py-2 rounded text-sm bg-sc-accent-bg text-stone-100 border border-sc-accent-border disabled:opacity-50"
                prop:disabled=move || {
                    saving.get() || checking.get()
                        || input.get().trim().chars().count() < 3
                        || avail.get() == Some(false)
                }
                on:click=save
            >{move || d().rec_username_save}</button>
        </div>
    }
}

/// Recommend-this-movie button + modal. Renders nothing when signed out —
/// the caller (MovieDetail) already knows auth state, this just self-gates
/// so it's safe to drop in anywhere.
#[component]
pub fn RecommendButton(movie_id: i64) -> impl IntoView {
    let auth = use_context::<RwSignal<Option<AuthState>>>().unwrap_or_else(|| RwSignal::new(None));
    let lang = use_lang();
    let d = move || dict(lang.get());

    let flow = RwSignal::new(RecFlow::Closed);
    let note = RwSignal::new(String::new());
    let friends = RwSignal::new(Vec::<FriendInfo>::new());
    let selected_friend = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let copied = RwSignal::new(false);

    let close = move || {
        flow.set(RecFlow::Closed);
        note.set(String::new());
        selected_friend.set(String::new());
        error.set(None);
        copied.set(false);
    };

    let open = move |_: web_sys::MouseEvent| {
        flow.set(RecFlow::Compose);
        error.set(None);
        if let Some(a) = auth.get_untracked() {
            spawn_local(async move {
                if let Ok(list) = api::fetch_friends(&a.token).await {
                    friends.set(list);
                }
            });
        }
    };

    let current_note = move || {
        let t = note.get_untracked();
        let t = t.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    };

    let send_link = move |_: web_sys::MouseEvent| {
        let Some(a) = auth.get_untracked() else {
            return;
        };
        let n = current_note();
        error.set(None);
        spawn_local(async move {
            match api::create_rec(&a.token, movie_id, n, None).await {
                Ok(created) => {
                    let url = format!("{}/r/{}", window_origin(), created.token);
                    flow.set(RecFlow::LinkReady(url));
                }
                Err(e) if e == "username_required" => flow.set(RecFlow::NeedsUsername),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let send_to_friend = move |_: web_sys::MouseEvent| {
        let Some(a) = auth.get_untracked() else {
            return;
        };
        let to = selected_friend.get_untracked();
        if to.is_empty() {
            return;
        }
        let n = current_note();
        error.set(None);
        spawn_local(async move {
            match api::create_rec(&a.token, movie_id, n, Some(to)).await {
                Ok(_) => flow.set(RecFlow::Sent),
                Err(e) if e == "username_required" => flow.set(RecFlow::NeedsUsername),
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let copy_link = move |url: String| {
        move |_: web_sys::MouseEvent| {
            if let Some(win) = web_sys::window() {
                let _ = win.navigator().clipboard().write_text(&url);
            }
            copied.set(true);
        }
    };

    // Username picked successfully: session state gets the new username so
    // the rest of the app (nav, future rec sends) sees it without a re-login.
    let on_username_done = Callback::new(move |name: String| {
        if let Some(a) = auth.get_untracked() {
            save_auth_to_storage(&a.token, &a.user_id, &a.email, Some(&name));
        }
        auth.update(|cur| {
            if let Some(s) = cur {
                s.username = Some(name);
            }
        });
        flow.set(RecFlow::Compose);
    });

    view! {
        <div>
            {move || match auth.get() {
                Some(_) => view! {
                    <button
                        class="px-3 py-1.5 rounded text-sm text-stone-400 border border-sc-border hover:border-sc-accent-border hover:text-stone-200"
                        on:click=open
                    >{move || d().rec_button}</button>
                }.into_any(),
                None => view! {
                    <A
                        href="/signin"
                        attr:class="inline-block px-3 py-1.5 rounded text-sm text-stone-400 border border-sc-border hover:border-sc-accent-border hover:text-stone-200"
                    >{move || d().rec_signin_cta}</A>
                }.into_any(),
            }}

            {move || match flow.get() {
                RecFlow::Closed => view! { <span /> }.into_any(),

                RecFlow::Compose => modal_shell(close, view! {
                    <h3 class="font-display text-xl text-stone-100 mb-3">{move || d().rec_modal_title}</h3>
                    <textarea
                        class="w-full bg-sc-card text-stone-200 border border-sc-border-input rounded-md px-3 py-2 text-sm placeholder-stone-600 focus:outline-none focus:border-sc-accent-border resize-none"
                        rows="2"
                        maxlength="140"
                        placeholder=move || d().rec_note_placeholder
                        prop:value=move || note.get()
                        on:input=move |ev| note.set(event_target_value(&ev))
                    />
                    <p class="text-xs text-stone-600 text-right mt-1">
                        {move || format!("{}/140", note.get().chars().count())}
                    </p>
                    {move || error.get().map(|e| view! { <p class="text-red-400 text-xs mt-2">{e}</p> })}
                    <div class="flex flex-col gap-2 mt-4">
                        <button
                            class="px-3 py-2 rounded text-sm bg-sc-accent-bg text-stone-100 border border-sc-accent-border"
                            on:click=send_link
                        >{move || d().rec_copy_link}</button>
                        {move || (!friends.get().is_empty()).then(|| view! {
                            <div class="flex gap-2 items-center">
                                <select
                                    class="flex-1 bg-sc-card text-stone-200 border border-sc-border-input rounded px-2 py-2 text-sm"
                                    prop:value=move || selected_friend.get()
                                    on:change=move |ev| selected_friend.set(event_target_value(&ev))
                                >
                                    <option value="">{move || d().rec_send_to_friend}</option>
                                    {friends.get().into_iter().map(|f| {
                                        let username = f.username;
                                        view! { <option value=username.clone()>{username.clone()}</option> }
                                    }).collect::<Vec<_>>()}
                                </select>
                                <button
                                    class="px-3 py-2 rounded text-sm border border-sc-border hover:border-sc-accent-border text-stone-300"
                                    on:click=send_to_friend
                                >{move || d().rec_send_to_friend}</button>
                            </div>
                        })}
                    </div>
                }).into_any(),

                RecFlow::NeedsUsername => modal_shell(
                    close,
                    view! { <UsernameModal on_done=on_username_done /> },
                ).into_any(),

                RecFlow::LinkReady(url) => {
                    let url_for_copy = url.clone();
                    let wa_url = format!("https://wa.me/?text={}", js_sys::encode_uri_component(&url));
                    modal_shell(close, view! {
                        <h3 class="font-display text-xl text-stone-100 mb-3">{move || d().rec_modal_title}</h3>
                        <input
                            type="text"
                            readonly=true
                            class="w-full bg-sc-card text-stone-300 border border-sc-border-input rounded px-3 py-2 text-sm"
                            prop:value=url.clone()
                        />
                        <div class="flex gap-2 mt-3">
                            <button
                                class="px-3 py-2 rounded text-sm border border-sc-border hover:border-sc-accent-border text-stone-300"
                                on:click=copy_link(url_for_copy)
                            >{move || d().rec_copy_link}</button>
                            <a
                                href=wa_url.clone()
                                target="_blank"
                                rel="noopener"
                                class="px-3 py-2 rounded text-sm bg-sc-accent-bg text-stone-100 border border-sc-accent-border no-underline"
                            >{move || d().rec_whatsapp}</a>
                        </div>
                        {move || copied.get().then(|| view! {
                            <p class="text-sc-accent text-xs mt-2">{move || d().rec_link_copied}</p>
                        })}
                    }).into_any()
                }

                RecFlow::Sent => modal_shell(close, view! {
                    <p class="text-stone-100 text-sm">{move || d().rec_sent}</p>
                }).into_any(),
            }}
        </div>
    }
}

// ── /r/{token} landing page ─────────────────────────────────────────────────

/// Public landing page for a share-link rec. Reached with or without a
/// session; the CTA differs accordingly. Signed-in visitors are auto-claimed
/// silently on arrival — the movie is the point (C3), the claim is bookkeeping.
#[component]
pub fn RecLandingPage() -> impl IntoView {
    let params = use_params_map();
    let auth = use_context::<RwSignal<Option<AuthState>>>().unwrap_or_else(|| RwSignal::new(None));
    let lang = use_lang();
    let d = move || dict(lang.get());
    // `StoredValue` makes the non-`Copy` `navigate` handle `Copy` itself, so
    // every closure that captures it (`want_to_watch`, nested per-render
    // view closures) stays `Fn`/reusable instead of degrading to `FnOnce`
    // the moment it's moved into a nested `move ||`.
    let navigate: StoredValue<_> = StoredValue::new(use_navigate());

    let token = move || params.with_untracked(|p| p.get("token").unwrap_or_default().to_string());

    let (loading, set_loading) = signal(true);
    let (rec, set_rec) = signal(Option::<RecPublic>::None);
    let (not_found, set_not_found) = signal(false);
    let (claiming, set_claiming) = signal(false);

    Effect::new(move |_| {
        let t = token();
        set_loading.set(true);
        set_not_found.set(false);
        spawn_local(async move {
            match api::fetch_rec_public(&t).await {
                Ok(r) => set_rec.set(Some(r)),
                Err(_) => set_not_found.set(true),
            }
            set_loading.set(false);
        });

        // Auto-claim: silent, idempotent, self-claim already a server no-op.
        if let Some(a) = auth.get_untracked() {
            let t2 = token();
            spawn_local(async move {
                let _ = api::claim_rec(&a.token, &t2).await;
            });
        }
    });

    let want_to_watch = move |_: web_sys::MouseEvent| {
        let Some(a) = auth.get_untracked() else {
            return;
        };
        let Some(r) = rec.get_untracked() else {
            return;
        };
        let t = token();
        set_claiming.set(true);
        spawn_local(async move {
            let _ = api::claim_rec(&a.token, &t).await;
            let _ = api::mark_rec_read(&a.token, &t).await;
            let _ = api::upsert_watchlist_via_rec(
                r.movie.id,
                WatchState::WantToWatch,
                None,
                Some(t.clone()),
                &a.token,
            )
            .await;
            navigate.with_value(|nav| {
                nav(
                    &format!("/movie/{}", encode_movie_id(r.movie.id)),
                    NavigateOptions::default(),
                )
            });
        });
    };

    view! {
        <div class="max-w-md mx-auto px-4 py-16">
            {move || if loading.get() {
                view! {
                    <div class="animate-pulse space-y-4">
                        <div class="h-72 bg-sc-border rounded" />
                        <div class="h-6 bg-sc-border rounded w-2/3 mx-auto" />
                    </div>
                }.into_any()
            } else if not_found.get() {
                view! {
                    <div class="text-center">
                        <h1 class="font-display text-4xl text-stone-100 mb-4">{move || d().nf_title}</h1>
                        <p class="text-stone-400 mb-8">{move || d().nf_body}</p>
                        <a href="/" class="text-sc-accent hover:text-sc-accent-hover">{move || d().nf_back}</a>
                    </div>
                }.into_any()
            } else if let Some(r) = rec.get() {
                view! {
                    <div class="text-center">
                        {r.movie.poster_url.clone().map(|p| view! {
                            <img src=p alt=r.movie.title.clone() class="w-48 mx-auto rounded shadow-lg mb-6" />
                        })}
                        <h1 class="font-display text-2xl text-stone-100 mb-1">{r.movie.title.clone()}</h1>
                        <p class="text-stone-500 text-sm mb-4">{r.movie.year.map(|y| y.to_string()).unwrap_or_default()}</p>
                        <p class="text-stone-300 mb-2">
                            {move || d().rec_landing_recommended_you.replace("{}", &r.sender_username)}
                        </p>
                        {r.note.clone().map(|n| view! {
                            <p class="text-stone-400 text-sm italic mb-4">"\u{201c}"{n}"\u{201d}"</p>
                        })}
                        <div class="mt-6">
                            {move || match auth.get() {
                                Some(_) => view! {
                                    <button
                                        class="px-4 py-2.5 rounded text-sm bg-sc-accent-bg text-stone-100 border border-sc-accent-border disabled:opacity-50"
                                        prop:disabled=move || claiming.get()
                                        on:click=want_to_watch
                                    >{move || d().rec_want_to_watch}</button>
                                }.into_any(),
                                None => view! {
                                    <a
                                        href=format!("/signin?next=/r/{}", token())
                                        class="inline-block px-4 py-2.5 rounded text-sm bg-sc-accent-bg text-stone-100 border border-sc-accent-border no-underline"
                                    >{move || d().rec_landing_cta}</a>
                                }.into_any(),
                            }}
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <div /> }.into_any()
            }}
        </div>
    }
}

// ── /recs inbox ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum RecsTab {
    Received,
    Sent,
}

/// Received + sent recommendations. Received is grouped per friend (a list
/// is human-scale here — no pagination). Sent exists primarily as the
/// revocation surface (Ethos: sender can retract at any time).
#[component]
pub fn RecsPage() -> impl IntoView {
    let auth = use_context::<RwSignal<Option<AuthState>>>().unwrap_or_else(|| RwSignal::new(None));
    let unread_recs = use_context::<RwSignal<i64>>().unwrap_or_else(|| RwSignal::new(0));
    let lang = use_lang();
    let d = move || dict(lang.get());
    // `StoredValue`: see the identical comment in `RecLandingPage` — keeps
    // every row-click closure `Fn`/reusable instead of FnOnce.
    let navigate: StoredValue<_> = StoredValue::new(use_navigate());

    let tab = RwSignal::new(RecsTab::Received);
    let received = RwSignal::new(Vec::<ReceivedRec>::new());
    let sent = RwSignal::new(Vec::<SentRec>::new());
    let loading = RwSignal::new(false);

    let reload = move || {
        let Some(a) = auth.get_untracked() else {
            return;
        };
        loading.set(true);
        let t1 = a.token.clone();
        spawn_local(async move {
            if let Ok(list) = api::fetch_received_recs(&t1).await {
                received.set(list);
            }
            loading.set(false);
        });
        let t2 = a.token.clone();
        spawn_local(async move {
            if let Ok(list) = api::fetch_sent_recs(&t2).await {
                sent.set(list);
            }
        });
    };

    Effect::new(move |_| {
        if auth.get().is_some() {
            reload();
        }
    });

    let open_and_read = move |token: String, movie_id: i64| {
        let Some(a) = auth.get_untracked() else {
            return;
        };
        received.update(|list| {
            if let Some(r) = list.iter_mut().find(|r| r.token == token) {
                if !r.read {
                    r.read = true;
                    unread_recs.update(|c| *c = (*c - 1).max(0));
                }
            }
        });
        spawn_local(async move {
            let _ = api::mark_rec_read(&a.token, &token).await;
        });
        navigate.with_value(|nav| {
            nav(
                &format!("/movie/{}", encode_movie_id(movie_id)),
                NavigateOptions::default(),
            )
        });
    };

    let want_to_watch_row = move |token: String, movie_id: i64| {
        let Some(a) = auth.get_untracked() else {
            return;
        };
        received.update(|list| {
            if let Some(r) = list.iter_mut().find(|r| r.token == token) {
                if !r.read {
                    r.read = true;
                    unread_recs.update(|c| *c = (*c - 1).max(0));
                }
            }
        });
        spawn_local(async move {
            let _ = api::mark_rec_read(&a.token, &token).await;
            let _ = api::upsert_watchlist_via_rec(
                movie_id,
                WatchState::WantToWatch,
                None,
                Some(token),
                &a.token,
            )
            .await;
        });
    };

    let revoke = move |token: String| {
        let Some(a) = auth.get_untracked() else {
            return;
        };
        if !web_sys::window()
            .map(|w| {
                w.confirm_with_message(d().rec_revoke_confirm)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
        {
            return;
        }
        spawn_local(async move {
            if api::revoke_rec(&a.token, &token).await.is_ok() {
                sent.update(|list| list.retain(|s| s.token != token));
            }
        });
    };

    view! {
        <Title text=move || d().rec_inbox_title />
        <div class="max-w-3xl mx-auto px-4 py-8">
            <h1 class="text-4xl font-bold text-stone-100 mb-6">{move || d().rec_inbox_title}</h1>

            {move || auth.get().is_none().then(|| view! {
                <div class="py-16 text-center">
                    <p class="text-stone-400 mb-4">{move || d().watchlist_signin_prompt}</p>
                    <A href="/signin?next=/recs"
                        attr:class="text-sc-accent hover:text-sc-accent-hover border border-sc-accent-border rounded px-4 py-2 text-sm">
                        {move || d().watchlist_signin_btn}
                    </A>
                </div>
            })}

            {move || auth.get().is_some().then(|| view! {
                <div>
                    <div class="flex gap-2 mb-6 border-b border-sc-border">
                        <button
                            class=move || if tab.get() == RecsTab::Received {
                                "px-3 py-2 text-sm text-sc-accent border-b-2 border-sc-accent"
                            } else {
                                "px-3 py-2 text-sm text-stone-500 hover:text-stone-300"
                            }
                            on:click=move |_| tab.set(RecsTab::Received)
                        >{move || d().rec_tab_received}</button>
                        <button
                            class=move || if tab.get() == RecsTab::Sent {
                                "px-3 py-2 text-sm text-sc-accent border-b-2 border-sc-accent"
                            } else {
                                "px-3 py-2 text-sm text-stone-500 hover:text-stone-300"
                            }
                            on:click=move |_| tab.set(RecsTab::Sent)
                        >{move || d().rec_tab_sent}</button>
                    </div>

                    {move || match tab.get() {
                        RecsTab::Received => {
                            let list = received.get();
                            if loading.get() && list.is_empty() {
                                view! { <p class="text-stone-500 text-sm">"..."</p> }.into_any()
                            } else if list.is_empty() {
                                view! { <p class="text-stone-500 py-8 text-center">{move || d().rec_inbox_empty}</p> }.into_any()
                            } else {
                                let mut groups: BTreeMap<String, Vec<ReceivedRec>> = BTreeMap::new();
                                for r in list {
                                    groups.entry(r.sender_username.clone()).or_default().push(r);
                                }
                                view! {
                                    <div class="space-y-6">
                                        {groups.into_iter().map(|(sender, recs)| {
                                            view! {
                                                <div>
                                                    <h2 class="text-xs uppercase tracking-widest text-stone-500 mb-2">
                                                        {move || d().rec_inbox_from.replace("{}", &sender)}
                                                    </h2>
                                                    <div class="space-y-2">
                                                        {recs.into_iter().map(|r| {
                                                            let row_class = if r.read {
                                                                "flex gap-3 p-3 rounded bg-sc-card border border-sc-border cursor-pointer"
                                                            } else {
                                                                "flex gap-3 p-3 rounded bg-sc-card border border-sc-accent-border cursor-pointer"
                                                            };
                                                            let tok1 = r.token.clone();
                                                            let tok2 = r.token.clone();
                                                            let mid = r.movie.id;
                                                            let poster = r.movie.poster_url.clone();
                                                            view! {
                                                                <div class=row_class on:click=move |_| open_and_read(tok1.clone(), mid)>
                                                                    {poster.map(|p| view! {
                                                                        <img src=p alt=r.movie.title.clone() class="w-12 h-18 object-cover rounded flex-shrink-0" />
                                                                    })}
                                                                    <div class="flex-1 min-w-0">
                                                                        <p class="text-stone-100 text-sm font-medium truncate">{r.movie.title.clone()}</p>
                                                                        <p class="text-stone-500 text-xs">{r.movie.year.map(|y| y.to_string()).unwrap_or_default()}</p>
                                                                        {r.note.clone().map(|n| view! {
                                                                            <p class="text-stone-400 text-xs italic mt-1">{n}</p>
                                                                        })}
                                                                        <button
                                                                            class="text-xs text-sc-accent hover:text-sc-accent-hover mt-1"
                                                                            on:click=move |ev: web_sys::MouseEvent| {
                                                                                ev.stop_propagation();
                                                                                want_to_watch_row(tok2.clone(), mid);
                                                                            }
                                                                        >{move || d().rec_want_to_watch}</button>
                                                                    </div>
                                                                </div>
                                                            }
                                                        }).collect::<Vec<_>>()}
                                                    </div>
                                                </div>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }
                        }
                        RecsTab::Sent => {
                            let list = sent.get();
                            if list.is_empty() {
                                view! { <p class="text-stone-500 py-8 text-center">{move || d().rec_inbox_empty}</p> }.into_any()
                            } else {
                                view! {
                                    <div class="space-y-2">
                                        {list.into_iter().map(|s| {
                                            let tok = s.token.clone();
                                            view! {
                                                <div class="flex gap-3 p-3 rounded bg-sc-card border border-sc-border">
                                                    {s.movie.poster_url.clone().map(|p| view! {
                                                        <img src=p alt=s.movie.title.clone() class="w-12 h-18 object-cover rounded flex-shrink-0" />
                                                    })}
                                                    <div class="flex-1 min-w-0">
                                                        <p class="text-stone-100 text-sm font-medium truncate">{s.movie.title.clone()}</p>
                                                        {s.note.clone().map(|n| view! {
                                                            <p class="text-stone-400 text-xs italic mt-1">{n}</p>
                                                        })}
                                                        <p class="text-stone-500 text-xs mt-1">
                                                            {move || d().rec_claims.replace("{}", &s.claim_count.to_string())}
                                                        </p>
                                                    </div>
                                                    <button
                                                        class="text-xs text-red-400 hover:text-red-300 self-start"
                                                        on:click=move |_| revoke(tok.clone())
                                                    >{move || d().rec_revoke}</button>
                                                </div>
                                            }
                                        }).collect::<Vec<_>>()}
                                    </div>
                                }.into_any()
                            }
                        }
                    }}
                </div>
            })}
        </div>
    }
}
