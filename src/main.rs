//! 숫자형 단축 URL 서비스 진입점.
//!
//! Dioxus 풀스택은 같은 코드를 두 번 빌드합니다.
//! - `web` 기능: 브라우저에서 돌아가는 wasm 화면
//! - `server` 기능: 화면을 내려주고, API·리디렉션·DB를 처리하는 서버
//!
//! `#[cfg(feature = "server")]`가 붙은 코드는 서버 빌드에만 들어갑니다.

mod api;
mod app;
#[cfg(feature = "server")]
mod server;

fn main() {
    // 브라우저: 화면 컴포넌트만 실행
    #[cfg(not(feature = "server"))]
    dioxus::launch(app::App);

    // 서버: Dioxus 기본 라우터(화면 + /api/*)에 `/숫자` 리디렉션 라우트를 더해 실행
    #[cfg(feature = "server")]
    dioxus::serve(|| async move {
        use dioxus::server::axum::routing::get;
        use server::redirect::{follow, reject_change};

        dotenvy::dotenv().ok(); // .env 파일이 있으면 환경변수로 읽기
        server::db::init().await?; // DB 연결 + 마이그레이션(테이블 생성)

        Ok(dioxus::server::router(app::App).route(
            "/{code}",
            // GET(과 HEAD)만 허용. 수정·삭제 메서드는 명시적으로 405로 거부한다.
            get(follow)
                .post(reject_change)
                .put(reject_change)
                .patch(reject_change)
                .delete(reject_change),
        ))
    });
}
