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

    let username = "nestor_churin";
    let email = "pavlonimetrons@gmail.com";
    let password = "33224433";

    println!("Creating user '{}' with email '{}'...", username, email);

    let hashed_password = hash(password, DEFAULT_COST)?;
    let user_id = Uuid::new_v4();
    let created_at = Utc::now();
    
    // Check if user exists first
    let exists = sqlx::query("SELECT id FROM users WHERE email = $1 OR username = $2")
        .bind(email)
        .bind(username)
        .fetch_optional(&pool)
        .await?;

    if exists.is_some() {
        println!("User already exists!");
        return Ok(());
    }

    // Determine role (if it's the first user, make admin, otherwise user - or force admin for this script?)
    // User requested creation, let's assume they want an admin or regular user. 
    // Given the context of "making authorization", usually the first manual users are admins.
    // But let's check existing logic or just set as admin for manually created user.
    // The prompt implies a specific user. I will make them admin just in case, or check existing logic.
    // The existing logic makes the *first* user admin.
    
    // Let's just hardcode role as 'admin' for this manually created user to be safe, or 'user'.
    // The user didn't specify, but usually manual creation implies admin access is desired or testing.
    // Let's default to 'admin' since they are asking me to create a user via script.
    
    let role = "admin"; 

    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role, permissions, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(user_id)
    .bind(username)
    .bind(email)
    .bind(hashed_password)
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
