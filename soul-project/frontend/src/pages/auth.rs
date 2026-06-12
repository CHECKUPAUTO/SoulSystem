use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use crate::api;

#[component]
pub fn LoginPage() -> impl IntoView {
    let email = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error = RwSignal::new(Option::<String>::None);
    let navigate = use_navigate();

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let email_val = email.get();
        let password_val = password.get();
        let navigate = navigate.clone();

        leptos::task::spawn_local(async move {
            match api::login(email_val, password_val).await {
                Ok(_) => {
                    navigate("/", Default::default());
                }
                Err(e) => {
                    error.set(Some(e));
                }
            }
        });
    };

    view! {
        <div class="login-container">
            <h1>"Login"</h1>
            <form on:submit=on_submit>
                <div>
                    <label>"Email"</label>
                    <input type="email"
                        on:input=move |ev| email.set(event_target_value(&ev))
                        prop:value=email
                    />
                </div>
                <div>
                    <label>"Password"</label>
                    <input type="password"
                        on:input=move |ev| password.set(event_target_value(&ev))
                        prop:value=password
                    />
                </div>
                <button type="submit">"Login"</button>
            </form>
            {move || error.get().map(|e| view! { <p class="error">{e}</p> })}
        </div>
    }
}
