//! Cloudflare Worker와 공유하는 비밀 헤더(X-Worker-Secret) 확인.
//!
//! 생성 API와 `/숫자` 리디렉션은 Worker를 거친 요청만 받는다. Worker는 Cloudflare Secret에 둔
//! 같은 값을 헤더에 넣어 보내고, 서버는 환경변수 WORKER_SECRET과 비교한다.
//! Railway 주소로 직접 들어온 요청은 이 헤더가 없으므로 403이 된다.

use super::validate::parse_code;
use dioxus::server::axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use subtle::ConstantTimeEq;

/// Worker가 보내는 헤더 이름 (HTTP 헤더는 대소문자를 구분하지 않는다)
pub const HEADER: &str = "x-worker-secret";

/// 환경변수 값을 검사한다. 없거나 공백뿐이면 에러 → 서버가 시작되지 않는다.
pub fn parse_secret(raw: Option<String>) -> anyhow::Result<String> {
    match raw.map(|s| s.trim().to_string()) {
        Some(s) if !s.is_empty() => Ok(s),
        _ => anyhow::bail!("WORKER_SECRET 환경변수가 비어 있습니다. 서버를 시작하지 않습니다."),
    }
}

/// 서버 시작 시 WORKER_SECRET을 읽는다.
pub fn load_secret() -> anyhow::Result<Arc<str>> {
    Ok(parse_secret(std::env::var("WORKER_SECRET").ok())?.into())
}

/// 상수 시간 비교. 값이 어디서 다른지에 따라 걸리는 시간이 달라지지 않는다.
/// (길이가 다르면 바로 false — 길이 정보만 드러나고 내용은 드러나지 않는다.)
pub fn secret_matches(expected: &str, provided: Option<&[u8]>) -> bool {
    provided.is_some_and(|p| bool::from(expected.as_bytes().ct_eq(p)))
}

/// 비밀 헤더가 필요한 경로: 생성 API(/api/...)와 `/숫자`.
/// 그 밖의 경로(화면, 정적 파일, 숫자가 아닌 경로의 404)는 확인하지 않는다.
pub fn is_protected(path: &str) -> bool {
    if path == "/api" || path.starts_with("/api/") {
        return true;
    }
    path.strip_prefix('/').and_then(parse_code).is_some()
}

/// axum 미들웨어: 보호 경로에서 비밀 헤더가 없거나 다르면 403.
pub async fn require_worker_secret(
    State(secret): State<Arc<str>>,
    req: Request,
    next: Next,
) -> Response {
    if is_protected(req.uri().path()) {
        let provided = req.headers().get(HEADER).map(|v| v.as_bytes());
        if !secret_matches(&secret, provided) {
            return (StatusCode::FORBIDDEN, "Forbidden").into_response();
        }
    }
    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::server::axum::{
        body::Body,
        middleware::from_fn_with_state,
        routing::{get, post},
        Router,
    };
    use tower::ServiceExt;

    #[test]
    fn empty_secret_is_rejected() {
        assert!(parse_secret(None).is_err());
        assert!(parse_secret(Some(String::new())).is_err());
        assert!(parse_secret(Some("   \n".into())).is_err());
        assert_eq!(parse_secret(Some(" abc ".into())).unwrap(), "abc");
    }

    #[test]
    fn compares_secret() {
        assert!(secret_matches("s3cret", Some(b"s3cret")));
        assert!(!secret_matches("s3cret", Some(b"s3creT")));
        assert!(!secret_matches("s3cret", Some(b"s3cre")));
        assert!(!secret_matches("s3cret", Some(b"s3cret!")));
        assert!(!secret_matches("s3cret", Some(b"")));
        assert!(!secret_matches("s3cret", None));
    }

    #[test]
    fn protected_paths() {
        for p in ["/api/links", "/api", "/1", "/2048"] {
            assert!(is_protected(p), "{p}");
        }
        for p in [
            "/",
            "/assets/app.wasm",
            "/abc",
            "/0123",
            "/0",
            "/1/2",
            "/apiary",
        ] {
            assert!(!is_protected(p), "{p}");
        }
    }

    fn app() -> Router {
        Router::new()
            .route("/api/links", post(|| async { "created" }))
            .route("/{code}", get(|| async { "redirect" }))
            .route("/", get(|| async { "home" }))
            .layer(from_fn_with_state(
                Arc::<str>::from("s3cret"),
                require_worker_secret,
            ))
    }

    async fn status(method: &str, path: &str, secret: Option<&str>) -> StatusCode {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(s) = secret {
            req = req.header("X-Worker-Secret", s);
        }
        app()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn middleware_requires_secret_on_protected_routes() {
        assert_eq!(
            status("POST", "/api/links", None).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status("POST", "/api/links", Some("wrong")).await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            status("POST", "/api/links", Some("s3cret")).await,
            StatusCode::OK
        );
        assert_eq!(status("GET", "/7", None).await, StatusCode::FORBIDDEN);
        assert_eq!(status("GET", "/7", Some("s3cret")).await, StatusCode::OK);
        // 보호하지 않는 경로
        assert_eq!(status("GET", "/", None).await, StatusCode::OK);
        assert_eq!(status("GET", "/abc", None).await, StatusCode::OK);
    }
}
