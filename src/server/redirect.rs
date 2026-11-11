//! `/숫자` 요청 처리: 원본 URL로 HTTP 302 리디렉션하거나 404 안내를 보여준다.

use super::{db, validate::parse_code};
use dioxus::server::axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{Html, IntoResponse, Response},
};

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
