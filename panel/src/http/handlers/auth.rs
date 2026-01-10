use axum::{
    extract::{State, Path, Form, ConnectInfo},
    http::{StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use crate::{state::AppState, models::{Node, User}, http::handlers::HtmlTemplate};
use askama::Template;
use bcrypt::verify;
use chrono::{Utc, Duration};
use rand::Rng;
use serde::Deserialize;
use uuid::Uuid;
use std::net::SocketAddr;

pub fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page).post(login_handler))
        .route("/logout", post(logout_handler))
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error: Option<String>,
    pub panel_font: String,
    pub panel_font_url: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
}

pub async fn login_page(State(state): State<AppState>) -> impl IntoResponse {
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();
    
    HtmlTemplate(LoginTemplate { 
        error: None,
        panel_font,
        panel_font_url,
    })
}

pub async fn login_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Form(payload): Form<LoginRequest>,
) -> Response {
    let user_opt = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
        .bind(&payload.email)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    if let Some(user) = user_opt {
        // Allow passwordless login if from localhost
        let is_localhost = addr.ip().is_loopback();
        
        if is_localhost || verify(&payload.password, &user.password_hash).unwrap_or(false) {
            // Create session
            let session_id = Uuid::new_v4();
            let expires_at_chrono = Utc::now() + Duration::days(7); // chrono duration
            
            // Store session in DB
            let res = sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
                .bind(session_id)
                .bind(user.id)
                .bind(expires_at_chrono)
                .execute(&state.db)
                .await;

            if res.is_ok() {
                let cookie = Cookie::build(("session_id", session_id.to_string()))
                    .path("/")
                    .http_only(true)
                    .secure(false) // Set to true in prod
                    .max_age(time::Duration::days(7));

                return (jar.add(cookie), Redirect::to("/")).into_response();
            }
        }
    }

    HtmlTemplate(LoginTemplate { 
        error: Some("Invalid email or password".into()),
        panel_font,
        panel_font_url,
    }).into_response()
}

pub async fn logout_handler(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    if let Some(session_cookie) = jar.get("session_id") {
         // Delete from DB
         if let Ok(session_uuid) = Uuid::parse_str(session_cookie.value()) {
            let _ = sqlx::query("DELETE FROM sessions WHERE id = $1")
                .bind(session_uuid)
                .execute(&state.db)
                .await;
         }
    }
    
    (jar.remove(Cookie::from("session_id")), Redirect::to("/auth/login"))
}

pub async fn rotate_token_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    let node_opt = sqlx::query_as::<_, Node>("SELECT id, name, ip, port, token FROM nodes WHERE id = $1::uuid")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

    if let Some(node) = node_opt {
        let new_token: String = rand::rng()
            .sample_iter(&rand::distr::Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        if let Some(manager) = &state.redis {
            let mut con = manager.clone();
            let key = format!("node:{}:pending_token", id);
            let _: Result<(), _> = redis::AsyncCommands::set_ex(&mut con, key, &new_token, 60).await;
        }

        let url = format!("http://{}:{}/update-token", node.ip, node.port);
        let payload = serde_json::json!({ "token": new_token });

        let resp = state.http_client.post(&url)
            .header("Authorization", format!("Bearer {}", node.token))
            .json(&payload)
            .send()
            .await;

        match resp {
            Ok(res) if res.status().is_success() => {
                let _ = sqlx::query("UPDATE nodes SET token = $1 WHERE id = $2::uuid")
                    .bind(&new_token)
                    .bind(&id)
                    .execute(&state.db)
                    .await;
                
                return StatusCode::OK;
            }
            _ => return StatusCode::INTERNAL_SERVER_ERROR
        }
    }
    StatusCode::NOT_FOUND
}

use axum::{
    middleware::Next,
    extract::Request,
};

pub async fn auth_middleware(
    State(state): State<AppState>,
    jar: CookieJar,
    request: Request,
    next: Next,
) -> Result<Response, Redirect> {
    let session_cookie = jar.get("session_id");
    if let Some(cookie) = session_cookie {
        if let Ok(session_id) = Uuid::parse_str(cookie.value()) {
             let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE id = $1 AND expires_at > NOW()")
                .bind(session_id)
                .fetch_one(&state.db)
                .await
                .unwrap_or(0);
            
            if count > 0 {
                return Ok(next.run(request).await);
            }
        }
    }
    
    Err(Redirect::to("/auth/login"))
}
