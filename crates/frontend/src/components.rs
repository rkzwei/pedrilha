use leptos::prelude::*;
use leptos::task::spawn_local;

/// Sign-in modal — email input → POST /api/auth/magic → "check your email" confirmation.
/// On success the user gets a magic link; clicking it navigates to `/auth/verify?token=...`
/// which exchanges the token for a JWT and stores it in the auth context.
///
/// Always rendered in the DOM; visibility is controlled via CSS display so mount/unmount
/// doesn't cause state-loss bugs. State resets whenever `is_open` transitions to true.
#[component]
pub fn SignInModal(is_open: Signal<bool>, on_close: Callback<()>) -> impl IntoView {
    let (email, set_email) = signal(String::new());
    let (sent, set_sent) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);
    let (loading, set_loading) = signal(false);

    // Reset form state every time the modal is opened.
    Effect::new(move |_| {
        if is_open.get() {
            set_sent.set(false);
            set_error.set(None);
            set_email.set(String::new());
            set_loading.set(false);
        }
    });

    let submit = move || {
        let e = email.get_untracked();
        if e.is_empty() {
            return;
        }
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            match crate::api::send_magic_link(&e).await {
                Ok(_) => set_sent.set(true),
                Err(msg) => set_error.set(Some(msg)),
            }
            set_loading.set(false);
        });
    };

    view! {
        // Full-screen backdrop — display:none when closed, flex when open.
        <div
            class="fixed inset-0 bg-black bg-opacity-75 z-50 flex items-center justify-center"
            style:display=move || if is_open.get() { "" } else { "none" }
            on:click=move |_| on_close.run(())
        >
            // Modal panel — stop propagation so clicking inside doesn't close
            <div
                class="bg-sc-panel border border-sc-border rounded-lg p-8 w-full max-w-sm mx-4"
                on:click=|ev: web_sys::MouseEvent| ev.stop_propagation()
            >
                {move || if sent.get() {
                    view! {
                        <div class="text-center">
                            <p class="text-4xl mb-4">"📬"</p>
                            <p class="text-stone-100 font-semibold text-lg">"Check your email"</p>
                            <p class="text-stone-400 text-sm mt-2">
                                "We sent a sign-in link to "
                                <span class="text-stone-300">{email.get()}</span>
                            </p>
                            <button
                                class="mt-6 text-sm text-stone-500 hover:text-stone-300 transition-colors"
                                on:click=move |_| on_close.run(())
                            >"Close"</button>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div>
                            <div class="flex justify-between items-start mb-6">
                                <div>
                                    <h2 class="text-xl font-bold text-stone-100">"Sign in"</h2>
                                    <p class="text-stone-400 text-sm mt-1">
                                        "We'll email you a magic link — no password needed."
                                    </p>
                                </div>
                                <button
                                    class="text-stone-600 hover:text-stone-300 text-lg leading-none ml-4"
                                    on:click=move |_| on_close.run(())
                                >"✕"</button>
                            </div>
                            <input
                                type="email"
                                placeholder="your@email.com"
                                class="w-full bg-sc-card text-stone-200 border border-sc-border-input rounded px-3 py-2 text-sm mb-3 focus:outline-none focus:border-sc-accent-border"
                                prop:value=move || email.get()
                                on:input=move |ev| set_email.set(event_target_value(&ev))
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    if ev.key() == "Enter" { submit(); }
                                }
                            />
                            {move || error.get().map(|e| view! {
                                <p class="text-red-400 text-xs mb-3">{e}</p>
                            })}
                            <button
                                class="w-full bg-sc-accent-bg text-stone-100 rounded px-4 py-2 text-sm font-medium disabled:opacity-50"
                                on:click=move |_| submit()
                                prop:disabled=move || loading.get()
                            >
                                {move || if loading.get() { "Sending…" } else { "Send sign-in link" }}
                            </button>
                        </div>
                    }.into_any()
                }}
            </div>
        </div>
    }
}
