//! 화면과 서버가 함께 쓰는 API.
//!
//! `#[post(...)]`를 붙인 함수는 "서버 함수"입니다. 함수 본문은 서버에서만 실행되고,
//! 브라우저에서 이 함수를 부르면 Dioxus가 자동으로 `POST /api/links` HTTP 요청으로 바꿔 줍니다.

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// 생성 결과. 서버 → 브라우저로 JSON으로 전달됩니다.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatedLink {
    pub id: i64,
    pub short_url: String,
    pub original_url: String,
}

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
#[post("/api/links")]
pub async fn create_link(url: String) -> Result<CreatedLink, ServerFnError> {
    use crate::server::{db, validate};

    // 1) 서버에서 다시 검증한다. 브라우저 검증은 우회할 수 있으므로 믿지 않는다.
    //    `?`는 "에러면 여기서 바로 반환"이라는 뜻이다. → 400 Bad Request
    let original_url = validate::normalize_url(&url).map_err(|msg| http_error(400, msg))?;

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
