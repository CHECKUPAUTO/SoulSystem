use leptos::prelude::*;
use leptos_router::components::*;
use leptos_router::path;
use frontend::pages::auth::LoginPage;
use frontend::pages::projects::ProjectListPage;
use frontend::pages::project_detail::ProjectPage;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <main>
                <Routes fallback=|| view! { "Page not found." }>
                    <Route path=path!("/") view=ProjectListPage/>
                    <Route path=path!("/login") view=LoginPage/>
                    <Route path=path!("/p/:project_id") view=ProjectPage/>
                    <Route path=path!("/p/:project_id/c/:cid") view=ProjectPage/>
                </Routes>
            </main>
        </Router>
    }
}

fn main() {
    leptos::mount::mount_to_body(App);
}
