// 라우팅 규칙. Worker 진입점(index.js)과 테스트가 함께 쓴다.

/** 0으로 시작하지 않는 양의 정수, 최대 19자리. 범위는 isCodePath에서 BigInt로 확인한다. */
const CODE_PATH = /^\/([1-9][0-9]{0,18})$/;

/** 백엔드 번호 타입(i64)의 최댓값 */
const I64_MAX = 9223372036854775807n;

/** `/숫자` 경로인지: 백엔드의 parse_code와 같은 규칙 (0으로 시작하지 않는 1 ~ i64 최댓값) */
export function isCodePath(pathname) {
  const m = CODE_PATH.exec(pathname);
  return m !== null && BigInt(m[1]) <= I64_MAX;
}

/** 허용 목록. 백엔드로 보낼 요청이면 종류("create" | "redirect")를, 아니면 null을 돌려준다. */
export function classify(request) {
  const { pathname } = new URL(request.url);
  const method = request.method;
  if (pathname === "/api/links" && method === "POST") return "create";
  if (isCodePath(pathname) && (method === "GET" || method === "HEAD")) return "redirect";
  return null;
}

/**
 * 요청 본문을 스트림으로 읽으면서 누적 크기를 센다.
 * maxBytes를 넘는 순간 읽기를 멈추고(reader.cancel) null을 돌려준다.
 * Content-Length 헤더가 없거나 거짓이어도 실제로 읽은 바이트로 판단한다.
 */
export async function readBodyLimited(request, maxBytes) {
  if (request.body === null) return new Uint8Array(0);
  const reader = request.body.getReader();
  const chunks = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > maxBytes) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }
  const body = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return body;
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
