//! `/숫자` 요청 처리: 원본 URL로 HTTP 302 리디렉션하거나 404 안내를 보여준다.

use super::{auth::require_worker_secret, db, validate::parse_code};
use dioxus::server::axum::{
    extract::Path,
    http::{header, StatusCode},
    middleware::from_fn_with_state,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use std::sync::Arc;

/// `/{code}` 라우터. 비밀 헤더 검사를 이 라우트에 직접(route_layer) 붙인다.
/// 매칭된 요청은 경로 인코딩(`/%31` 등)과 상관없이 핸들러보다 먼저 검사된다.
pub fn router(secret: Arc<str>) -> Router {
    Router::new()
        .route(
            "/{code}",
            // GET(과 HEAD)만 허용. 수정·삭제 메서드는 명시적으로 405로 거부한다.
            get(follow)
                .post(reject_change)
                .put(reject_change)
                .patch(reject_change)
                .delete(reject_change),
        )
        .route_layer(from_fn_with_state(secret, require_worker_secret))
}

/// GET /{code}
pub async fn follow(Path(code): Path<String>) -> Response {
    let Some(id) = parse_code(&code) else {
        return not_found();
    };

    match db::find(id).await {
        // Location 헤더에 저장된 URL을 그대로 넣으므로 경로·쿼리·#프래그먼트가 유지된다.
        Ok(Some(original_url)) => {
            (StatusCode::FOUND, [(header::LOCATION, original_url)]).into_response()
        }
        Ok(None) => not_found(),
        Err(err) => {
            eprintln!("DB 조회 실패: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "서버 오류가 발생했습니다.",
            )
                .into_response()
        }
    }
}

/// POST·PUT·PATCH·DELETE /{code}: 생성된 연결은 바꿀 수 없으므로 항상 405로 거부한다.
pub async fn reject_change() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::ALLOW, "GET, HEAD")],
        "단축 주소는 생성 후 수정·삭제할 수 없습니다.",
    )
        .into_response()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Html(
            r#"<!doctype html>
<html lang="ko"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>없는 단축 주소</title>
<style>body{font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;line-height:1.6}</style>
</head><body>
<h1>404 · 없는 단축 주소입니다</h1>
<p>입력한 번호로 등록된 URL이 없습니다. 주소를 다시 확인해 주세요.</p>
<p><a href="/">새 단축 주소 만들기</a></p>
</body></html>"#,
        ),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dioxus::server::axum::{body::Body, extract::Request};
    use tower::ServiceExt;

    /// 실제 리디렉션 라우터: 비밀 헤더가 없거나 틀리면 핸들러(DB)에 닿기 전에 403.
    #[tokio::test]
    async fn redirect_router_rejects_without_secret() {
        for path in ["/1", "/%31", "/%31%32", "/abc", "/0123"] {
            for method in ["GET", "HEAD", "PUT", "DELETE"] {
                for secret in [None, Some("wrong")] {
                    let mut req = Request::builder().method(method).uri(path);
                    if let Some(s) = secret {
                        req = req.header("X-Worker-Secret", s);
                    }
                    let res = router(Arc::from("s3cret"))
                        .oneshot(req.body(Body::empty()).unwrap())
                        .await
                        .unwrap();
                    assert_eq!(
                        res.status(),
                        StatusCode::FORBIDDEN,
                        "{method} {path} {secret:?}"
                    );
                }
            }
        }
    }

    /// 비밀이 맞으면 핸들러까지 간다. (DB가 필요 없는 경로로 확인: 숫자가 아니면 404, 변경은 405)
    #[tokio::test]
    async fn redirect_router_passes_with_secret() {
        for (method, path, expected) in [
            ("GET", "/abc", StatusCode::NOT_FOUND),
            ("GET", "/0123", StatusCode::NOT_FOUND),
            ("PUT", "/%31", StatusCode::METHOD_NOT_ALLOWED),
        ] {
            let req = Request::builder()
                .method(method)
                .uri(path)
                .header("X-Worker-Secret", "s3cret")
                .body(Body::empty())
                .unwrap();
            let res = router(Arc::from("s3cret")).oneshot(req).await.unwrap();
            assert_eq!(res.status(), expected, "{method} {path}");
        }
    }
}
