use std::{fs, path::PathBuf};

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use sqlx::{Row, SqlitePool, migrate::Migrator};

static MIGRATOR: Migrator = sqlx::migrate!();

pub fn prepare_database_dir(database_url: &str) -> std::io::Result<()> {
    let Some(path) = sqlite_file_path(database_url) else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    Ok(())
}

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::migrate::MigrateError> {
    MIGRATOR.run(pool).await
}

pub async fn ensure_admin(
    pool: &SqlitePool,
    admin_user: &str,
    admin_password: &str,
) -> Result<(), sqlx::Error> {
    let count: i64 = sqlx::query("SELECT COUNT(*) FROM admins")
        .fetch_one(pool)
        .await?
        .get(0);

    if count > 0 {
        return Ok(());
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(admin_password.as_bytes(), &salt)
        .expect("default password hashing should not fail")
        .to_string();

    sqlx::query("INSERT INTO admins (username, password_hash) VALUES ($1, $2)")
        .bind(admin_user)
        .bind(password_hash)
        .execute(pool)
        .await?;

    Ok(())
}

fn sqlite_file_path(database_url: &str) -> Option<PathBuf> {
    let raw = database_url.strip_prefix("sqlite://")?;
    let path = raw.split('?').next().unwrap_or(raw);
    if path == ":memory:" || path.is_empty() {
        return None;
    }

    Some(PathBuf::from(path))
}
