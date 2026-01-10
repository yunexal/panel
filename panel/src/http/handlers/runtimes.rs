use crate::http::handlers::HtmlTemplate;
use crate::{
    models::{Image, Runtime},
    state::AppState,
};
use askama::Template;
use axum::{
    extract::{Form, Json, State},
    response::{IntoResponse, Redirect},
};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "runtimes.html")]
struct RuntimesTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    // Using a tuple struct or wrapper for logic
    runtimes: Vec<RuntimeWithImages>,
}

struct RuntimeWithImages {
    runtime: Runtime,
    images: Vec<Image>,
    image_count: usize,
}

#[derive(Template)]
#[template(path = "runtime_create.html")]
struct CreateRuntimeTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
}

#[derive(Template)]
#[template(path = "runtime_edit.html")]
struct EditRuntimeTemplate {
    panel_font: String,
    panel_font_url: String, // Added
    panel_name: String,
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    runtime: Runtime,
}

pub async fn edit_runtime_page_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    let runtime = sqlx::query_as::<_, Runtime>(
        "SELECT id::text, name, description, color, sort_order FROM runtimes WHERE id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await;

    match runtime {
        Ok(r) => {
            let elapsed = start_time.elapsed();
            let execution_time = elapsed.as_secs_f64() * 1000.0;

            HtmlTemplate(EditRuntimeTemplate {
                panel_name,
                panel_font,
                panel_font_url,
                panel_version,
                execution_time,
                active_tab: "runtimes".to_string(),
                runtime: r,
            })
            .into_response()
        }
        Err(_) => Redirect::to("/runtimes").into_response(),
    }
}

pub async fn update_runtime_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Form(payload): Form<UpdateRuntimeRequest>,
) -> Redirect {
    let _ = sqlx::query(
        "UPDATE runtimes SET name = $1, description = $2, color = $3 WHERE id = $4::uuid",
    )
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.color)
    .bind(&id)
    .execute(&state.db)
    .await;

    Redirect::to("/runtimes")
}

pub async fn delete_runtime_handler(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let _ = sqlx::query("DELETE FROM runtimes WHERE id = $1::uuid")
        .bind(&id)
        .execute(&state.db)
        .await;

    // Return empty 200 OK for hx-delete to just work, but we are pushing URL /runtimes,
    // so ideally we should redirect or return a script.
    // Since hx-target="body", we might want to return the redirect header for HTMX.

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("HX-Redirect", "/runtimes".parse().unwrap());
    (headers, "Deleted")
}

#[derive(serde::Deserialize)]
pub struct CreateRuntimeRequest {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    #[allow(dead_code)]
    #[serde(default = "default_color")]
    pub color: String,
}

#[derive(serde::Deserialize)]
pub struct UpdateRuntimeRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "#007bff".to_string()
}

#[derive(Template)]
#[template(path = "image_create.html")]
struct CreateImageTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    runtime_id: String,
}

#[derive(serde::Deserialize)]
pub struct CreateImageRequest {
    pub name: String,
    pub docker_images: String,
    pub description: Option<String>,
    pub startup_command: String,
    pub stop_command: String,
    #[serde(default)]
    pub requires_port: bool,
    pub log_config: String,
    pub config_files: String,
    pub start_config: String,
    // New Fields
    #[serde(default)]
    pub install_script: String,
    #[serde(default)]
    pub install_container: String,
    #[serde(default)]
    pub install_entrypoint: String,
    #[serde(default = "default_array_json")]
    pub variables: String,
}

fn default_array_json() -> String {
    "[]".to_string()
}

pub async fn create_image_page_handler(
    State(state): State<AppState>,
    axum::extract::Path(runtime_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(CreateImageTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "runtimes".to_string(),
        runtime_id,
    })
}

pub async fn create_image_handler(
    State(state): State<AppState>,
    axum::extract::Path(runtime_id): axum::extract::Path<String>,
    Form(payload): Form<CreateImageRequest>,
) -> Redirect {
    let id = Uuid::new_v4().to_string();

    let _ = sqlx::query("INSERT INTO images (id, runtime_id, name, docker_images, description, startup_command, stop_command, requires_port, log_config, config_files, start_config, install_script, install_container, install_entrypoint, variables) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)")
        .bind(&id)
        .bind(&runtime_id)
        .bind(&payload.name)
        .bind(&payload.docker_images)
        .bind(&payload.description)
        .bind(&payload.startup_command)
        .bind(&payload.stop_command)
        .bind(payload.requires_port)
        .bind(&payload.log_config)
        .bind(&payload.config_files)
        .bind(&payload.start_config)
        .bind(&payload.install_script)
        .bind(&payload.install_container)
        .bind(&payload.install_entrypoint)
        .bind(&payload.variables)
        .execute(&state.db)
        .await;

    Redirect::to("/runtimes")
}

pub async fn runtimes_page_handler(State(state): State<AppState>) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();

    let runtimes_db = sqlx::query_as::<_, Runtime>("SELECT id::text, name, description, color, sort_order FROM runtimes ORDER BY sort_order ASC, name ASC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    let images_db = sqlx::query_as::<_, Image>("SELECT id::text, runtime_id::text, name, docker_images, description, stop_command, startup_command, log_config, config_files, start_config, requires_port, install_script::text, install_container::text, install_entrypoint::text, variables::text FROM images")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

    // Group images by runtime
    let mut runtimes = Vec::new();
    for r in runtimes_db {
        let mut my_images: Vec<Image> = images_db
            .iter()
            .filter(|i| i.runtime_id == r.id)
            .cloned()
            .collect();
        my_images.sort_by(|a, b| a.name.cmp(&b.name));

        let count = my_images.len();
        runtimes.push(RuntimeWithImages {
            runtime: r,
            images: my_images,
            image_count: count,
        });
    }

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(RuntimesTemplate {
        panel_name,
        panel_font,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "runtimes".to_string(),
        runtimes,
    })
}

pub async fn create_runtime_page_handler(State(state): State<AppState>) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();
    let panel_name = state.panel_name.read().await.clone();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();

    let elapsed = start_time.elapsed();
    let execution_time = elapsed.as_secs_f64() * 1000.0;

    HtmlTemplate(CreateRuntimeTemplate {
        panel_font,
        panel_name,
        panel_font_url,
        panel_version,
        execution_time,
        active_tab: "runtimes".to_string(),
    })
}

pub async fn create_runtime_handler(
    State(state): State<AppState>,
    Form(payload): Form<CreateRuntimeRequest>,
) -> Redirect {
    let id = Uuid::new_v4().to_string();

    let _ = sqlx::query(
        "INSERT INTO runtimes (id, name, description, color) VALUES ($1::uuid, $2, $3, $4)",
    )
    .bind(&id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(&payload.color)
    .execute(&state.db)
    .await;

    Redirect::to("/runtimes")
}

// Egg Parsing Structures
#[derive(serde::Deserialize, Debug)]
struct EggScript {
    #[serde(default)]
    script: String,
    #[serde(default)]
    container: String,
    #[serde(default)]
    entrypoint: String,
}

#[derive(serde::Deserialize, Debug)]
struct EggScripts {
    #[serde(default)]
    installation: Option<EggScript>,
}

#[derive(serde::Deserialize, Debug)]
struct EggVariable {
    name: String,
    description: String,
    env_variable: String,
    default_value: String,
    user_viewable: bool,
    user_editable: bool,
    rules: String,
    field_type: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct EggConfig {
    #[serde(default)]
    files: Option<serde_json::Value>,
    #[serde(default)]
    startup: Option<serde_json::Value>,
    #[serde(default)]
    logs: Option<serde_json::Value>,
    #[serde(default)]
    stop: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct Egg {
    name: String,
    description: Option<String>,
    #[serde(default)]
    docker_images: std::collections::HashMap<String, String>,
    startup: String,
    config: EggConfig,
    #[serde(default)]
    scripts: Option<EggScripts>,
    #[serde(default)]
    variables: Option<Vec<EggVariable>>,
}

pub async fn import_egg_handler(
    State(state): State<AppState>,
    axum::extract::Path(runtime_id): axum::extract::Path<String>,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    tracing::info!("Starting Egg Import for Runtime ID: {}", runtime_id);

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "egg_file" {
            let data = match field.bytes().await {
                Ok(d) => d,
                Err(e) => return format!("Failed to read file data: {}", e).into_response(),
            };

            let egg: Egg = match serde_json::from_slice(&data) {
                Ok(e) => e,
                Err(e) => {
                    tracing::error!("Failed to parse Egg JSON: {}", e);
                    // Try to print a snippet of the json for debugging
                    let snippet = String::from_utf8_lossy(&data)
                        .chars()
                        .take(200)
                        .collect::<String>();
                    tracing::error!("JSON Snippet: {}", snippet);
                    return format!("Failed to parse Egg JSON: {}", e).into_response();
                }
            };

            tracing::info!("Parsed Egg: {}", egg.name);

            let id = Uuid::new_v4().to_string();

            // Extract docker images
            // Store as JSON string: {"Display Name": "ghcr.io/..."}
            let images_str = serde_json::to_string(&egg.docker_images).unwrap_or("{}".to_string());

            let stop_cmd = egg.config.stop.unwrap_or_else(|| "stop".to_string());

            // Helper to normalize JSON fields formats (handles stringified JSON which Pterodactyl sometimes exports)
            let normalize_json = |v: Option<serde_json::Value>| -> String {
                match v {
                    Some(serde_json::Value::String(s)) => {
                        match serde_json::from_str::<serde_json::Value>(&s) {
                            Ok(parsed) => serde_json::to_string_pretty(&parsed).unwrap_or(s),
                            Err(_) => s,
                        }
                    }
                    Some(v) => serde_json::to_string_pretty(&v).unwrap_or(v.to_string()),
                    None => "{}".to_string(),
                }
            };

            let log_cfg = normalize_json(egg.config.logs);
            let start_cfg = normalize_json(egg.config.startup);
            let file_cfg = normalize_json(egg.config.files);

            // Script details
            let (script, container, entry) = if let Some(scripts) = egg.scripts {
                if let Some(inst) = scripts.installation {
                    (inst.script, inst.container, inst.entrypoint)
                } else {
                    (String::new(), String::new(), "bash".to_string())
                }
            } else {
                (String::new(), String::new(), "bash".to_string())
            };

            // Variables
            let vars_json = if let Some(vars) = egg.variables {
                // Map Pterodactyl vars to strictly string types
                let mapped_vars: Vec<serde_json::Value> = vars
                    .into_iter()
                    .map(|v| {
                        serde_json::json!({
                            "name": v.name,
                            "description": v.description,
                            "env_variable": v.env_variable,
                            "default_value": v.default_value,
                            "user_viewable": v.user_viewable,
                            "user_editable": v.user_editable,
                            "rules": v.rules,
                            "field_type": v.field_type.unwrap_or("text".to_string())
                        })
                    })
                    .collect();
                serde_json::to_string(&mapped_vars).unwrap_or("[]".to_string())
            } else {
                "[]".to_string()
            };

            let q_res = sqlx::query("INSERT INTO images (id, runtime_id, name, docker_images, description, startup_command, stop_command, requires_port, log_config, config_files, start_config, install_script, install_container, install_entrypoint, variables) VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)")
                .bind(&id)
                .bind(&runtime_id)
                .bind(&egg.name)
                .bind(&images_str)
                .bind(&egg.description)
                .bind(&egg.startup)
                .bind(&stop_cmd)
                .bind(true)
                .bind(&log_cfg)
                .bind(&file_cfg)
                .bind(&start_cfg)
                .bind(&script)
                .bind(&container)
                .bind(&entry)
                .bind(&vars_json)
                .execute(&state.db)
                .await;

            match q_res {
                Ok(_) => tracing::info!("Successfully imported egg: {}", egg.name),
                Err(e) => {
                    tracing::error!("Database error importing egg: {}", e);
                    return format!("Database Error: {}", e).into_response();
                }
            }

            return Redirect::to("/runtimes").into_response();
        }
    }

    tracing::warn!("No 'egg_file' field found in upload");
    "No file uploaded or field name mismatch (expected 'egg_file')".into_response()
}

#[derive(Template)]
#[template(path = "image_edit.html")]
struct EditImageTemplate {
    panel_name: String,
    panel_font: String,
    panel_font_url: String, // Added
    panel_version: String,
    execution_time: f64,
    active_tab: String,
    runtime_id: String,
    image: Image,
}

pub async fn edit_image_page_handler(
    State(state): State<AppState>,
    axum::extract::Path((runtime_id, image_id)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    let start_time = std::time::Instant::now();
    let panel_version = env!("CARGO_PKG_VERSION").to_string();
    let panel_name = state.panel_name.read().await.clone();
    let panel_font = state.panel_font.read().await.clone();
    let panel_font_url = state.panel_font_url.read().await.clone();

    let image = sqlx::query_as::<_, Image>("SELECT id::text, runtime_id::text, name, docker_images, description, stop_command, startup_command, log_config, config_files, start_config, requires_port, install_script::text, install_container::text, install_entrypoint::text, variables::text FROM images WHERE id = $1::uuid")
        .bind(&image_id)
        .fetch_one(&state.db)
        .await;

    match image {
        Ok(mut img) => {
            // Helper to recursively unwrap and prettify JSON
            // Pterodactyl sometimes double-encodes or uses escaped newlines in strings.
            fn smart_prettify(s: &str) -> String {
                if s.trim().is_empty() || s == "{}" || s == "[]" {
                    return s.to_string();
                }

                // Try parsing as Value
                match serde_json::from_str::<serde_json::Value>(s) {
                    Ok(val) => {
                        // If it parsed as a String, it might be double encoded
                        if let serde_json::Value::String(inner_str) = &val {
                            // Try parsing the inner string
                            if let Ok(inner_val) =
                                serde_json::from_str::<serde_json::Value>(inner_str)
                            {
                                return serde_json::to_string_pretty(&inner_val)
                                    .unwrap_or(s.to_string());
                            }
                        }
                        // Otherwise just pretty print the value
                        serde_json::to_string_pretty(&val).unwrap_or(s.to_string())
                    }
                    // If it failed to parse, check if it's already a single-line valid JSON that just needs formatting?
                    // Or maybe it has escaped characters like \r\n that act as literals?
                    Err(_) => {
                        // If it contains literal "\n" sequence, unescape it
                        if s.contains("\\n") {
                            let unescaped = s
                                .replace("\\n", "\n")
                                .replace("\\r", "\r")
                                .replace("\\\"", "\"");
                            // Try parsing unescaped version (wrapping in quotes might be needed if it was stripped?)
                            // Actually, if it was a raw string like "{\r\n ... }", replacing gives "{...}".
                            // Let's try parsing the replaced version
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&unescaped) {
                                return serde_json::to_string_pretty(&val).unwrap_or(s.to_string());
                            }
                        }
                        s.to_string()
                    }
                }
            }

            img.config_files = smart_prettify(&img.config_files);
            img.log_config = smart_prettify(&img.log_config);
            img.start_config = smart_prettify(&img.start_config);
            img.variables = smart_prettify(&img.variables);

            let elapsed = start_time.elapsed();
            let execution_time = elapsed.as_secs_f64() * 1000.0;

            HtmlTemplate(EditImageTemplate {
                panel_name,
                panel_font,
                panel_font_url,
                panel_version,
                execution_time,
                active_tab: "runtimes".to_string(),
                runtime_id,
                image: img,
            })
            .into_response()
        }
        Err(_) => Redirect::to(&format!("/runtimes/{}/edit", runtime_id)).into_response(),
    }
}

pub async fn update_image_handler(
    State(state): State<AppState>,
    axum::extract::Path((runtime_id, image_id)): axum::extract::Path<(String, String)>,
    Form(payload): Form<CreateImageRequest>, // Reusing request struct since fields are same
) -> Redirect {
    let _ = sqlx::query("UPDATE images SET name = $1, docker_images = $2, description = $3, startup_command = $4, stop_command = $5, requires_port = $6, log_config = $7, config_files = $8, start_config = $9, install_script = $10, install_container = $11, install_entrypoint = $12, variables = $13 WHERE id = $14::uuid")
        .bind(&payload.name)
        .bind(&payload.docker_images)
        .bind(&payload.description)
        .bind(&payload.startup_command)
        .bind(&payload.stop_command)
        .bind(payload.requires_port)
        .bind(&payload.log_config)
        .bind(&payload.config_files)
        .bind(&payload.start_config)
        .bind(&payload.install_script)
        .bind(&payload.install_container)
        .bind(&payload.install_entrypoint)
        .bind(&payload.variables)
        .bind(&image_id)
        .execute(&state.db)
        .await;

    Redirect::to(&format!("/runtimes/{}/edit", runtime_id))
}

pub async fn delete_image_handler(
    State(state): State<AppState>,
    axum::extract::Path((_runtime_id, image_id)): axum::extract::Path<(String, String)>,
) -> impl IntoResponse {
    let _ = sqlx::query("DELETE FROM images WHERE id = $1::uuid")
        .bind(&image_id)
        .execute(&state.db)
        .await;

    let mut headers = axum::http::HeaderMap::new();
    headers.insert("HX-Redirect", "/runtimes".parse().unwrap());
    (headers, "Deleted")
}

#[derive(serde::Deserialize)]
pub struct ReorderRequest {
    pub ids: Vec<String>,
}

pub async fn reorder_runtimes_handler(
    State(state): State<AppState>,
    Json(payload): Json<ReorderRequest>,
) -> impl IntoResponse {
    let mut tx = state.db.begin().await.unwrap();

    for (idx, id) in payload.ids.iter().enumerate() {
        let _ = sqlx::query("UPDATE runtimes SET sort_order = $1 WHERE id = $2::uuid")
            .bind(idx as i32)
            .bind(id)
            .execute(&mut *tx)
            .await;
    }

    let _ = tx.commit().await;
    (axum::http::StatusCode::OK, "Reordered")
}
