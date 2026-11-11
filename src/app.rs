//! 화면(UI). 브라우저에서 wasm으로 실행됩니다.

use crate::api::{create_link, CreatedLink};
use dioxus::prelude::*;

const MAIN_CSS: Asset = asset!("/assets/main.css");

/// 클립보드 복사 JS. 최신 API가 막히면 예전 방식(execCommand)으로 한 번 더 시도한다.
const COPY_JS: &str = r#"
const text = await dioxus.recv();
try {
  await navigator.clipboard.writeText(text);
  return true;
} catch (_) {
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  ta.select();
  const ok = document.execCommand("copy");
  ta.remove();
  return ok;
}
"#;

#[component]
pub fn App() -> Element {
    // use_signal: 값이 바뀌면 화면이 자동으로 다시 그려지는 상태
    let mut input = use_signal(String::new);
    let mut result = use_signal(|| None::<CreatedLink>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    // 복사 상태: None = 아직 안 누름, Some(true) = 성공, Some(false) = 실패
    let mut copied = use_signal(|| None::<bool>);

    // 생성 버튼: 서버 함수 create_link를 호출 (실제로는 POST /api/links 요청)
    let on_submit = move |evt: FormEvent| async move {
        evt.prevent_default();
        busy.set(true);
        error.set(None);
        copied.set(None);
        match create_link(input()).await {
            Ok(link) => result.set(Some(link)),
            Err(err) => {
                result.set(None);
                // 서버가 보낸 안내 문구만 꺼내서 보여준다.
                let msg = match err {
                    ServerFnError::ServerError { message, .. } => message,
                    other => other.to_string(),
                };
                error.set(Some(msg));
            }
        }
        busy.set(false);
    };

    // 복사 버튼: 브라우저의 클립보드 API를 JS로 호출한다.
    // Rust → JS로 값을 넘길 때는 문자열을 JS 코드에 끼워 넣지 않고 dioxus.recv()로 받는다.
    let on_copy = move |_| async move {
        let Some(link) = result() else { return };
        let js = document::eval(COPY_JS);
        let sent = js.send(link.short_url).is_ok();
        let ok = sent && matches!(js.await, Ok(v) if v == true);
        copied.set(Some(ok));
    };

    rsx! {
        document::Title { "숫자형 단축 URL" }
        document::Stylesheet { href: MAIN_CSS }

        main {
            h1 { "숫자형 단축 URL" }
            p { class: "hint", "긴 주소를 넣으면 /123 같은 짧은 주소를 만들어 드립니다. 로그인은 필요 없습니다." }

            form { onsubmit: on_submit,
                input {
                    r#type: "url",
                    placeholder: "https://example.org/very/long/path?query=1",
                    value: "{input}",
                    oninput: move |e| input.set(e.value()),
                    required: true,
                }
                button { r#type: "submit", disabled: busy(), if busy() { "생성 중…" } else { "생성" } }
            }

            if let Some(msg) = error() {
                p { class: "error", role: "alert", "{msg}" }
            }

            if let Some(link) = result() {
                section { class: "result",
                    p { "단축 주소" }
                    div { class: "row",
                        a { href: "{link.short_url}", target: "_blank", "{link.short_url}" }
                        button { onclick: on_copy, if copied() == Some(true) { "복사됨" } else { "복사" } }
                    }
                    if copied() == Some(false) {
                        p { class: "error", "브라우저가 클립보드 접근을 막았습니다. 주소를 직접 선택해 복사해 주세요." }
                    }
                    p { class: "hint", "원본: {link.original_url}" }
                }
            }
        }
    }
}
