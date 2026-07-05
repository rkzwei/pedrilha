//! Friend-recommendation UI (Ethos C1). All rec components live here —
//! pages.rs is already ~3k lines and must not grow with this feature.

use gem_finder_shared::types::FriendInfo;
use leptos::prelude::*;
use leptos::task::spawn_local;
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
fn modal_shell(on_close: impl Fn() + Copy + 'static, children: impl IntoView + 'static) -> impl IntoView {
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
            {move || auth.get().is_some().then(|| view! {
                <button
                    class="px-3 py-1.5 rounded text-sm text-stone-400 border border-sc-border hover:border-sc-accent-border hover:text-stone-200"
                    on:click=open
                >{move || d().rec_button}</button>
            })}

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
