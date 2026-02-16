use crate::config::app_config::AppConfig;
use sqlx::{Pool, SqlitePool};

pub async fn init(config: &AppConfig) -> Result<Pool<sqlx::Sqlite>, sqlx::Error> {
    let path = config.get_db_path();
    let url = format!("sqlite://{}", path);
    let pool = SqlitePool::connect(&url).await?;

    sqlx::query(
        r#"
        create table if not exists journal (
            id integer primary key autoincrement,
            content text not null,
            date text not null,
            create_time integer not null,
            update_time integer not null
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        create table if not exists resource (
            id integer primary key autoincrement,
            kind text not null,
            uri text not null,
            file_path text not null,
            create_time integer not null,
            update_time integer not null
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        create table if not exists blob (
            id integer primary key autoincrement,
            kind text not null,
            algo text not null,
            oid text not null,
            mime text not null,
            size integer not null,
            original_name text not null,
            uri text not null,
            daylog_uri text not null,
            file_path text not null,
            create_time integer not null,
            update_time integer not null,
            unique (kind, algo, oid)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        create table if not exists file_blob (
            id integer primary key autoincrement,
            kind text not null,
            algo text not null,
            oid text not null,
            mime text not null,
            size integer not null,
            original_name text not null,
            uri text not null,
            file_path text not null,
            create_time integer not null,
            update_time integer not null,
            unique (kind, algo, oid)
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}
