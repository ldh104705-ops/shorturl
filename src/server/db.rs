//! PostgreSQL 접근. 이 파일에는 INSERT와 SELECT만 있고 UPDATE·DELETE는 없습니다.

use anyhow::Context;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::OnceLock;

/// 서버 전체가 공유하는 DB 연결 풀. 시작할 때 한 번만 채워진다.
static POOL: OnceLock<PgPool> = OnceLock::new();

/// DB에 연결하고 `migrations/` 폴더의 SQL을 적용한다(이미 적용된 건 건너뜀).
pub async fn init() -> anyhow::Result<()> {
    if POOL.get().is_some() {
        return Ok(());
    }
    let url = std::env::var("DATABASE_URL").context("DATABASE_URL 환경변수가 필요합니다")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let _ = POOL.set(pool);
    Ok(())
}

fn pool() -> &'static PgPool {
    POOL.get().expect("DB가 아직 초기화되지 않았습니다")
}

/// 단축 주소 앞부분. 예: http://localhost:8080
pub fn public_base_url() -> String {
    std::env::var("PUBLIC_BASE_URL")
        .unwrap_or_else(|_| "http://localhost:8080".into())
        .trim_end_matches('/')
        .to_string()
}

/// 새 URL이면 번호를 발급하고, 이미 있는 URL이면 기존 번호를 돌려준다.
///
/// - 번호는 DB의 IDENTITY 시퀀스가 원자적으로 발급하므로 동시 요청에도 겹치지 않는다.
/// - `ON CONFLICT DO NOTHING`: 같은 URL이 이미 있으면 아무것도 덮어쓰지 않는다.
pub async fn insert_or_get(original_url: &str) -> Result<i64, sqlx::Error> {
    let inserted: Option<i64> = sqlx::query_scalar(
        "INSERT INTO links (original_url) VALUES ($1)
         ON CONFLICT (original_url) DO NOTHING
         RETURNING id",
    )
    .bind(original_url)
    .fetch_optional(pool())
    .await?;

    match inserted {
        Some(id) => Ok(id),
        None => {
            sqlx::query_scalar("SELECT id FROM links WHERE original_url = $1")
                .bind(original_url)
                .fetch_one(pool())
                .await
        }
    }
}

/// 번호로 원본 URL을 찾는다. 없으면 `None`.
pub async fn find(id: i64) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT original_url FROM links WHERE id = $1")
        .bind(id)
        .fetch_optional(pool())
        .await
}
