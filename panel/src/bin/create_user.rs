use bcrypt::{hash, DEFAULT_COST};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
use chrono::Utc;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let db_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost/yunexal".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await?;

    let username = env::var("ADMIN_USERNAME")
        .unwrap_or_else(|_| "admin".to_string());
    let email = env::var("ADMIN_EMAIL")
        .unwrap_or_else(|_| "admin@mail.com".to_string());
    let password = env::var("ADMIN_PASSWORD")
        .unwrap_or_else(|_| "qwerty123456".to_string());

    println!("Creating user '{}' with email '{}'...", username, email);

    let hashed_password = hash(&password, DEFAULT_COST)?;
    let user_id = Uuid::new_v4();
    let created_at = Utc::now();
    let role = "admin";

    let exists = sqlx::query(
        "SELECT id FROM users WHERE email = $1 OR username = $2"
    )
    .bind(&email)
    .bind(&username)
    .fetch_optional(&pool)
    .await?;

    if exists.is_some() {
        println!("User already exists!");
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT INTO users
        (id, username, email, password_hash, role, permissions, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#
    )
    .bind(user_id)
    .bind(&username)        
    .bind(&email)           
    .bind(&hashed_password)
    .bind(role)
    .bind("{}")
    .bind(created_at)
    .execute(&pool)
    .await?;

    println!("User created successfully!");
    println!("ID: {}", user_id);
    println!("Role: {}", role);

    Ok(())
}
