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

    // 서버: Dioxus 기본 라우터(화면 + /api/links 서버 함수)에 `/숫자` 리디렉션 라우터를 합쳐 실행
    #[cfg(feature = "server")]
    dioxus::serve(|| async move {
        dotenvy::dotenv().ok(); // .env 파일이 있으면 환경변수로 읽기
        let secret = server::auth::load_secret()?; // WORKER_SECRET이 비어 있으면 여기서 시작 중단
        server::validate::init_self_hosts(); // 자기 서비스 주소 목록(단축 대상에서 제외)
        server::db::init().await?; // DB 연결 + 마이그레이션(테이블 생성)

        // 비밀 헤더 검사는 각 라우트에 직접 붙어 있다:
        // - /{code}     : redirect::router의 route_layer
        // - /api/links  : api::create_link 함수 본문 맨 앞
        Ok(dioxus::server::router(app::App).merge(server::redirect::router(secret)))
    });
}
