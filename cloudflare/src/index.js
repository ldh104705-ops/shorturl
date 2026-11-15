// Cloudflare Worker: 공개 접점.
//
// 요청 처리 순서 (wrangler.jsonc의 assets 설정)
// 1. public/ 에 있는 정적 파일(/, /assets/*.wasm·js·css)은 Cloudflare가 직접 응답한다.
//    이 경우 Worker 코드는 실행되지 않는다.
// 2. 정적 파일에 없는 경로만 이 fetch()로 들어온다. 그중 허용 목록에 있는 요청만 Railway로 전달한다.
//    - POST /api/links          : 단축 주소 생성 (IP별 분당 20회 제한)
//    - GET·HEAD /{숫자}          : 리디렉션 (제한 없음, 302와 Location을 그대로 반환)
//    - 그 밖의 모든 요청          : Worker가 직접 404
//
// 필요한 환경 값
//   BACKEND_URL     (wrangler.jsonc vars)  Railway 공개 주소
//   WORKER_SECRET   (Cloudflare Secret)    백엔드와 공유하는 비밀 값. 설정 파일·코드에 넣지 않는다.
//   CREATE_LIMITER  (ratelimits binding)   생성 요청 호출 제한

import { backendTarget, classify } from "./routing.js";

/** 생성 요청 본문 최대 크기(바이트). URL 2048자 + JSON 여유분. */
const MAX_BODY_BYTES = 8 * 1024;

/** 백엔드로 넘길 요청 헤더. Cookie·Authorization 등은 넘기지 않는다. */
const FORWARD_HEADERS = ["content-type", "accept", "accept-language"];

export default {
  async fetch(request, env) {
    const route = classify(request);
    if (!route) {
      return notFound();
    }

    // 설정 확인: 둘 중 하나라도 없으면 백엔드로 보내지 않는다.
    let backend;
    try {
      backend = new URL(env.BACKEND_URL);
    } catch {
      return textResponse(500, "서버 설정 오류: BACKEND_URL");
    }
    if (!env.WORKER_SECRET) {
      return textResponse(500, "서버 설정 오류: WORKER_SECRET");
    }

    // 생성 요청에만 IP별 호출 제한
    if (route === "create") {
      const ip = request.headers.get("CF-Connecting-IP") ?? "unknown";
      const { success } = await env.CREATE_LIMITER.limit({ key: ip });
      if (!success) {
        return textResponse(429, "요청이 너무 많습니다. 잠시 후 다시 시도해 주세요.", {
          "Retry-After": "60",
        });
      }
    }

    // 목적지 고정: BACKEND_URL에서 출발해 경로와 쿼리만 바꾸고, 전송 직전에 출처를 다시 확인한다.
    const target = backendTarget(request.url, backend);
    if (!target) {
      return textResponse(502, "잘못된 전달 대상입니다.");
    }

    // 필요한 헤더만 새로 만들고, 공유 비밀 헤더는 항상 덮어쓴다.
    const headers = new Headers();
    for (const name of FORWARD_HEADERS) {
      const value = request.headers.get(name);
      if (value !== null) headers.set(name, value);
    }
    headers.set("X-Worker-Secret", env.WORKER_SECRET);

    let body;
    if (route === "create") {
      body = await request.arrayBuffer();
      if (body.byteLength > MAX_BODY_BYTES) {
        return textResponse(413, "요청 본문이 너무 큽니다.");
      }
    }

    // redirect: "manual" → 백엔드의 302를 따라가지 않고 Location과 함께 브라우저에 그대로 돌려준다.
    return fetch(target, { method: request.method, headers, body, redirect: "manual" });
  },
};

function textResponse(status, message, extraHeaders = {}) {
  return new Response(message, {
    status,
    headers: { "content-type": "text/plain; charset=utf-8", ...extraHeaders },
  });
}

function notFound() {
  return new Response(
    `<!doctype html>
<html lang="ko"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>없는 페이지</title>
<style>body{font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;line-height:1.6}</style>
</head><body>
<h1>404 · 없는 페이지입니다</h1>
<p>주소를 다시 확인해 주세요.</p>
<p><a href="/">새 단축 주소 만들기</a></p>
</body></html>`,
    { status: 404, headers: { "content-type": "text/html; charset=utf-8" } },
  );
}
