// Cloudflare Worker: 공개 접점.
//
// 요청 처리 순서 (wrangler.jsonc의 assets 설정)
// 1. public/ 에 있는 정적 파일(/, /assets/*.wasm·js·css)은 Cloudflare가 직접 응답한다.
//    이 경우 Worker 코드는 실행되지 않는다.
// 2. 정적 파일에 없는 경로만 이 fetch()로 들어온다.
//    /api/*, /숫자, 그 밖의 경로를 모두 Railway 백엔드로 그대로 전달한다.

export default {
  async fetch(request, env) {
    if (!env.BACKEND_URL) {
      return new Response("BACKEND_URL이 설정되지 않았습니다.", { status: 500 });
    }

    // 경로와 쿼리는 유지하고, 호스트만 백엔드로 바꾼다.
    const incoming = new URL(request.url);
    const target = new URL(incoming.pathname + incoming.search, env.BACKEND_URL);

    // 메서드·헤더·본문을 그대로 복사한 요청.
    // redirect: "manual" → 백엔드의 302를 따라가지 않고 Location과 함께 브라우저에 그대로 돌려준다.
    const proxied = new Request(target, request);
    const response = await fetch(proxied, { redirect: "manual" });

    return response;
  },
};
