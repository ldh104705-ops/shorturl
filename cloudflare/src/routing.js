// 라우팅 규칙. Worker 진입점(index.js)과 테스트가 함께 쓴다.

/** 0으로 시작하지 않는 양의 정수 (i64 범위 안: 최대 18자리). 백엔드의 parse_code와 같은 규칙. */
const CODE_PATH = /^\/[1-9][0-9]{0,17}$/;

/** 허용 목록. 백엔드로 보낼 요청이면 종류("create" | "redirect")를, 아니면 null을 돌려준다. */
export function classify(request) {
  const { pathname } = new URL(request.url);
  const method = request.method;
  if (pathname === "/api/links" && method === "POST") return "create";
  if (CODE_PATH.test(pathname) && (method === "GET" || method === "HEAD")) return "redirect";
  return null;
}

/** 프로토콜·호스트·포트가 모두 같은지 */
export function sameOrigin(a, b) {
  return a.protocol === b.protocol && a.hostname === b.hostname && a.port === b.port;
}

/**
 * 백엔드로 보낼 URL. BACKEND_URL에서 출발해 경로와 쿼리만 바꾼다.
 * new URL(path, base)는 "//다른호스트/..." 같은 경로에서 호스트가 바뀔 수 있어서 쓰지 않는다.
 * 결과가 BACKEND_URL과 다른 출처면 null.
 */
export function backendTarget(requestUrl, backend) {
  const incoming = new URL(requestUrl);
  const target = new URL(backend.href);
  target.pathname = incoming.pathname;
  target.search = incoming.search;
  return sameOrigin(target, backend) ? target : null;
}
