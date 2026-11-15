//! 화면과 서버가 함께 쓰는 API.
//!
//! - 풀스택 빌드(`fullstack` 기능, dx serve·Railway): `#[post(...)]`를 붙인 `create_link`는 "서버 함수"입니다.
//!   함수 본문은 서버에서만 실행되고, 브라우저에서 부르면 Dioxus가 `POST /api/links` 요청으로 바꿔 줍니다.
//! - 클라이언트 전용 빌드(`spa` 기능, Cloudflare 정적 파일): 서버 함수가 없으므로
//!   같은 `POST /api/links` 요청을 직접 보냅니다. 요청·응답 형식은 서버 함수와 같습니다.
//!
//! 화면(app.rs)은 어느 빌드든 `request_link`만 부릅니다.

use serde::{Deserialize, Serialize};

#[cfg(all(feature = "spa", feature = "fullstack"))]
compile_error!("`spa`는 `fullstack`·`server`와 함께 켤 수 없습니다. `--no-default-features --features spa`로 빌드하세요.");

#[cfg(not(any(feature = "spa", feature = "fullstack")))]
compile_error!("`fullstack`(기본값) 또는 `spa` 기능 중 하나를 켜야 합니다.");

/// 생성 결과. 서버 → 브라우저로 JSON으로 전달됩니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatedLink {
    pub id: i64,
    pub short_url: String,
    pub original_url: String,
}

// ───────────── 풀스택 빌드: 서버 함수 ─────────────

#[cfg(feature = "fullstack")]
use dioxus::prelude::*;

/// 상태 코드와 사용자에게 보여줄 메시지를 담은 에러를 만든다.
#[cfg(feature = "server")]
fn http_error(code: u16, message: &str) -> ServerFnError {
    ServerFnError::ServerError {
        message: message.to_string(),
        code,
        details: None,
    }
}

/// 원본 URL을 받아 숫자형 단축 주소를 만든다. 로그인 없이 누구나 호출할 수 있다.
///
/// 요청 예: `POST /api/links` 본문 `{"url": "https://example.org"}`
///
/// `headers: HeaderMap`은 서버에서만 보이는 axum 추출기다. Worker의 비밀 헤더를 확인하는 데 쓴다.
#[cfg(feature = "fullstack")]
#[post("/api/links", headers: dioxus::fullstack::HeaderMap)]
pub async fn create_link(url: String) -> Result<CreatedLink, ServerFnError> {
    use crate::server::{auth, db, validate};

    // 0) Worker를 거친 요청인지 확인한다. 이 함수(라우트) 안에서 검사하므로 경로 표기와 상관없다.
    if !auth::request_is_from_worker(&headers) {
        return Err(http_error(403, "Forbidden"));
    }

    // 1) 서버에서 다시 검증한다. 브라우저 검증은 우회할 수 있으므로 믿지 않는다.
    //    `?`는 "에러면 여기서 바로 반환"이라는 뜻이다. → 400 Bad Request
    let original_url = validate::normalize_url(&url, validate::self_hosts())
        .map_err(|msg| http_error(400, msg))?;

    // 2) DB가 번호를 발급(또는 같은 URL의 기존 번호를 조회)한다.
    let id = db::insert_or_get(&original_url).await.map_err(|err| {
        eprintln!("DB 저장 실패: {err}");
        http_error(500, "서버 오류로 주소를 만들지 못했습니다.")
    })?;

    Ok(CreatedLink {
        id,
        short_url: format!("{}/{id}", db::public_base_url()),
        original_url,
    })
}

/// 화면에서 부르는 함수. 실패하면 사용자에게 보여줄 문구를 돌려준다.
#[cfg(feature = "fullstack")]
pub async fn request_link(url: String) -> Result<CreatedLink, String> {
    create_link(url).await.map_err(|err| match err {
        ServerFnError::ServerError { message, .. } => message,
        other => other.to_string(),
    })
}

// ───────────── 클라이언트 전용 빌드: 직접 HTTP 요청 ─────────────

/// 같은 출처(현재 페이지 주소)의 `/api/links`로 POST한다. Cloudflare Worker가 Railway로 전달한다.
#[cfg(not(feature = "fullstack"))]
const CREATE_JS: &str = r#"
const url = await dioxus.recv();
try {
  const res = await fetch("/api/links", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ url }),
  });
  let body = null;
  try { body = await res.json(); } catch (_) {}
  return { ok: res.ok, status: res.status, body };
} catch (_) {
  return { ok: false, status: 0, body: null };
}
"#;

#[cfg(not(feature = "fullstack"))]
pub async fn request_link(url: String) -> Result<CreatedLink, String> {
    use dioxus::prelude::document;
    use serde_json::Value;

    #[derive(Deserialize)]
    struct FetchResult {
        ok: bool,
        status: u16,
        body: Option<Value>,
    }

    let js = document::eval(CREATE_JS);
    js.send(url)
        .map_err(|_| "요청을 보내지 못했습니다.".to_string())?;
    let raw = js
        .await
        .map_err(|_| "서버에 연결하지 못했습니다.".to_string())?;
    let res: FetchResult =
        serde_json::from_value(raw).map_err(|_| "응답을 해석하지 못했습니다.".to_string())?;

    if res.ok {
        return res
            .body
            .and_then(|b| serde_json::from_value(b).ok())
            .ok_or_else(|| "응답을 해석하지 못했습니다.".to_string());
    }

    // 서버 함수의 에러 응답: {"message": "...", "code": 400, "data": {"ServerError": {"message": "..."}}}
    let message = res.body.as_ref().and_then(|b| {
        b.pointer("/data/ServerError/message")
            .or_else(|| b.get("message"))
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    Err(message.unwrap_or_else(|| match res.status {
        0 => "서버에 연결하지 못했습니다.".to_string(),
        code => format!("요청이 실패했습니다. (HTTP {code})"),
    }))
}
