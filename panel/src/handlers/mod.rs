pub mod dashboard;
pub mod nodes;
pub mod allocations;
pub mod auth;
pub mod api;
pub mod scripts;
pub mod logs;
pub mod overview;
pub mod servers;

use axum::response::{Html, IntoResponse, Response};
use askama::Template;
use axum::http::StatusCode;

pub struct HtmlTemplate<T>(pub T);

impl<T> IntoResponse for HtmlTemplate<T>
where
    T: Template,
{
    fn into_response(self) -> Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to render template. Error: {}", err),
            )
                .into_response(),
        }
    }
}


