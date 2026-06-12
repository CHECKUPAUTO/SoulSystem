use leptos::prelude::*;
use crate::api;

#[component]
pub fn ProjectListPage() -> impl IntoView {
    let projects = LocalResource::new(|| async move { api::list_projects().await.unwrap_or_default() });

    view! {
        <div class="project-list">
            <h1>"My Projects"</h1>
            <Suspense fallback=|| view! { <p>"Loading..."</p> }>
                <ul>
                    {move || Suspend::new(async move {
                        projects.await.into_iter().map(|p| view! {
                            <li>
                                <a href=format!("/p/{}", p.id)>{p.name}</a>
                                <p>{p.description}</p>
                            </li>
                        }).collect::<Vec<_>>()
                    })}
                </ul>
            </Suspense>
        </div>
    }
}
