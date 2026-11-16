//! Cloudflare Worker와 공유하는 비밀 헤더(X-Worker-Secret) 확인.
//!
//! 생성 API와 `/숫자` 리디렉션은 Worker를 거친 요청만 받는다. Worker는 Cloudflare Secret에 둔
//! 같은 값을 헤더에 넣어 보내고, 서버는 환경변수 WORKER_SECRET과 비교한다.
//! Railway 주소로 직접 들어온 요청은 이 헤더가 없으므로 403이 된다.
//!
//! 검사는 경로 문자열 모양(숫자인지 등)으로 판단하지 않고, 라우트 자체에 붙인다.
//! - `/{code}` 리디렉션: `redirect::router`가 `route_layer`로 이 미들웨어를 붙인다.
//!   `/%31`처럼 인코딩된 경로도 같은 라우트에 매칭되므로 항상 검사된다.
//! - `/api/links` 서버 함수: 두 겹으로 검사한다.
//!   1. `require_secret_on_api_routes` 미들웨어가 **본문을 해석하기 전에** 매칭된 라우트
//!      (`MatchedPath`)를 보고 검사한다. 원문 경로 문자열이 아니라 라우터가 고른 라우트 패턴으로 판단한다.
//!   2. 서버 함수 본문 맨 앞에서 `request_is_from_worker`로 한 번 더 검사한다.

use dioxus::server::axum::{
    extract::{MatchedPath, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::{Arc, OnceLock};
use subtle::ConstantTimeEq;

/// Worker가 보내는 헤더 이름 (HTTP 헤더는 대소문자를 구분하지 않는다)
pub const HEADER: &str = "x-worker-secret";

/// 서버 시작 시 읽은 비밀 값. 서버 함수에서 쓰려고 전역에 둔다.
static SECRET: OnceLock<Arc<str>> = OnceLock::new();

/// 환경변수 값을 검사한다. 없거나 공백뿐이면 에러 → 서버가 시작되지 않는다.
pub fn parse_secret(raw: Option<String>) -> anyhow::Result<String> {
    match raw.map(|s| s.trim().to_string()) {
        Some(s) if !s.is_empty() => Ok(s),
        _ => anyhow::bail!("WORKER_SECRET 환경변수가 비어 있습니다. 서버를 시작하지 않습니다."),
    }
}

/// 서버 시작 시 WORKER_SECRET을 읽어 전역에 저장하고 돌려준다.
pub fn load_secret() -> anyhow::Result<Arc<str>> {
    let secret: Arc<str> = parse_secret(std::env::var("WORKER_SECRET").ok())?.into();
    Ok(SECRET.get_or_init(|| secret).clone())
}

/// 상수 시간 비교. 값이 어디서 다른지에 따라 걸리는 시간이 달라지지 않는다.
/// (길이가 다르면 바로 false — 길이 정보만 드러나고 내용은 드러나지 않는다.)
pub fn secret_matches(expected: &str, provided: Option<&[u8]>) -> bool {
    provided.is_some_and(|p| bool::from(expected.as_bytes().ct_eq(p)))
}

/// 요청 헤더에 올바른 비밀 값이 있는지
pub fn headers_match(headers: &HeaderMap, expected: &str) -> bool {
    secret_matches(expected, headers.get(HEADER).map(|v| v.as_bytes()))
}

/// 서버 함수용: 전역 비밀 값과 비교한다. 비밀 값이 아직 없으면(시작 전) 항상 거부.
pub fn request_is_from_worker(headers: &HeaderMap) -> bool {
    SECRET
        .get()
        .is_some_and(|expected| headers_match(headers, expected))
}

/// axum 미들웨어: 비밀 헤더가 없거나 다르면 403. `route_layer`로 보호할 라우트에만 붙인다.
pub async fn require_worker_secret(
    State(secret): State<Arc<str>>,
    req: Request,
    next: Next,
) -> Response {
    if !headers_match(req.headers(), &secret) {
        return forbidden();
    }
    next.run(req).await
}

/// 비밀 헤더가 필요한 서버 함수 라우트 (라우터에 등록된 패턴 그대로)
pub const PROTECTED_API_ROUTES: &[&str] = &["/api/links"];

/// axum 미들웨어: Dioxus 라우터 전체에 `layer`로 붙인다.
/// 라우터가 이 요청을 `PROTECTED_API_ROUTES`의 라우트로 매칭했으면, 핸들러가 본문을 읽기 전에 검사한다.
/// (`MatchedPath`는 라우터가 고른 패턴이라 `/api/%6Cinks` 같은 표기로는 바뀌지 않는다.)
pub async fn require_secret_on_api_routes(
    State(secret): State<Arc<str>>,
    req: Request,
    next: Next,
) -> Response {
    let protected = req
        .extensions()
        .get::<MatchedPath>()
        .is_some_and(|m| PROTECTED_API_ROUTES.contains(&m.as_str()));
    if protected && !headers_match(req.headers(), &secret) {
        return forbidden();
    }
    next.run(req).await
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, "Forbidden").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::server::axum::{body::Body, middleware::from_fn_with_state, routing::get, Router};
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
    fn checks_header_map() {
        let mut h = HeaderMap::new();
        assert!(!headers_match(&h, "s3cret"));
        h.insert("X-Worker-Secret", "wrong".parse().unwrap());
        assert!(!headers_match(&h, "s3cret"));
        h.insert("x-worker-secret", "s3cret".parse().unwrap());
        assert!(headers_match(&h, "s3cret"));
    }

    /// route_layer로 붙인 라우트는 경로 모양과 상관없이 항상 검사된다.
    fn app() -> Router {
        Router::new()
            .route("/{code}", get(|| async { "redirect" }))
            .route_layer(from_fn_with_state(
                Arc::<str>::from("s3cret"),
                require_worker_secret,
            ))
            // route_layer 뒤에 추가한 라우트는 검사하지 않는다
            .route("/", get(|| async { "home" }))
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
    async fn route_layer_checks_every_matching_path() {
        for path in ["/7", "/%31", "/%37%37", "/abc", "/0123"] {
            for method in ["GET", "HEAD"] {
                assert_eq!(
                    status(method, path, None).await,
                    StatusCode::FORBIDDEN,
                    "{method} {path} 비밀 없음"
                );
                assert_eq!(
                    status(method, path, Some("wrong")).await,
                    StatusCode::FORBIDDEN,
                    "{method} {path} 틀린 비밀"
                );
                assert_eq!(
                    status(method, path, Some("s3cret")).await,
                    StatusCode::OK,
                    "{method} {path} 올바른 비밀"
                );
            }
        }
        assert_eq!(status("GET", "/", None).await, StatusCode::OK);
    }

    /// Dioxus 라우터와 같은 모양: 서버 함수 라우트는 핸들러에서 본문을 해석하고, 실패하면 500.
    fn api_app() -> Router {
        use dioxus::server::axum::routing::post;
        Router::new()
            .route(
                "/api/links",
                post(|body: String| async move {
                    if body.starts_with('{') {
                        (StatusCode::OK, "created")
                    } else {
                        (StatusCode::INTERNAL_SERVER_ERROR, "error deserializing")
                    }
                }),
            )
            .route("/", get(|| async { "home" }))
            .layer(from_fn_with_state(
                Arc::<str>::from("s3cret"),
                require_secret_on_api_routes,
            ))
    }

    async fn api_status(method: &str, path: &str, secret: Option<&str>, body: &str) -> StatusCode {
        let mut req = Request::builder().method(method).uri(path);
        if let Some(s) = secret {
            req = req.header("X-Worker-Secret", s);
        }
        api_app()
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn api_route_checks_secret_before_body_parsing() {
        // 비밀이 없거나 틀리면 본문이 잘못돼도(해석 전에) 403
        for secret in [None, Some("wrong")] {
            for body in [
                "xxxx",
                "url=https://example.org",
                "",
                r#"{"url":"https://example.org"}"#,
            ] {
                assert_eq!(
                    api_status("POST", "/api/links", secret, body).await,
                    StatusCode::FORBIDDEN,
                    "{secret:?} {body:?}"
                );
            }
        }
        // 비밀이 맞으면 핸들러까지 간다
        assert_eq!(
            api_status("POST", "/api/links", Some("s3cret"), r#"{"url":"x"}"#).await,
            StatusCode::OK
        );
        assert_eq!(
            api_status("POST", "/api/links", Some("s3cret"), "xxxx").await,
            StatusCode::INTERNAL_SERVER_ERROR
        );
        // 보호 대상이 아닌 라우트는 그대로
        assert_eq!(api_status("GET", "/", None, "").await, StatusCode::OK);
        // 다른 표기는 이 라우트에 매칭되지 않으므로 핸들러에 닿지 않는다
        for path in ["/api/%6Cinks", "/API/links", "/api/links/"] {
            let s = api_status("POST", path, None, r#"{"url":"x"}"#).await;
            assert!(s != StatusCode::OK, "{path} → {s}");
        }
    }
}
