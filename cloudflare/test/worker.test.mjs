// Worker 단위 테스트. 실행: npm test  (node --test)
// 실제 네트워크 대신 globalThis.fetch와 호출 제한 바인딩을 가짜로 바꿔서 확인한다.
import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, it } from "node:test";

import worker from "../src/index.js";
import { backendTarget, classify, readBodyLimited } from "../src/routing.js";

const BACKEND = "https://backend.example";
const SECRET = "test-secret-value";

/** 가짜 fetch: 호출 내용을 기록하고 정해 둔 응답을 돌려준다. */
let calls;
let nextResponse;
const realFetch = globalThis.fetch;

beforeEach(() => {
  calls = [];
  nextResponse = () => new Response("ok", { status: 200 });
  globalThis.fetch = async (input, init = {}) => {
    calls.push({ url: String(input), init });
    return nextResponse();
  };
});
afterEach(() => {
  globalThis.fetch = realFetch;
});

/** 가짜 호출 제한: allow=false면 항상 초과 */
function limiter(allow = true) {
  const keys = [];
  return { keys, limit: async ({ key }) => (keys.push(key), { success: allow }) };
}

function env(overrides = {}) {
  return { BACKEND_URL: BACKEND, WORKER_SECRET: SECRET, CREATE_LIMITER: limiter(), ...overrides };
}

function req(path, init = {}) {
  return new Request(`https://public.example${path}`, init);
}

describe("classify: 허용 목록", () => {
  const cases = [
    ["POST", "/api/links", "create"],
    ["GET", "/1", "redirect"],
    ["HEAD", "/2048", "redirect"],
    ["GET", "/api/links", null],
    ["PUT", "/1", null],
    ["DELETE", "/1", null],
    ["POST", "/1", null],
    ["GET", "/0", null],
    ["GET", "/0123", null],
    ["GET", "/abc", null],
    ["GET", "/12a", null],
    ["GET", "/1/2", null],
    ["GET", "/1234567890123456789", "redirect"], // 19자리지만 i64 범위 안
    ["GET", "/9223372036854775807", "redirect"], // i64 최댓값
    ["GET", "/9223372036854775808", null], // i64 최댓값 + 1
    ["GET", "/9999999999999999999", null], // 19자리, i64 범위 밖
    ["GET", "/12345678901234567890", null], // 20자리
    ["GET", "/%31", null], // 인코딩된 숫자는 받지 않는다
    ["GET", "/api/other", null],
    ["GET", "//attacker.example/collect", null],
  ];
  for (const [method, path, expected] of cases) {
    it(`${method} ${path} → ${expected}`, () => {
      assert.equal(classify(req(path, { method })), expected);
    });
  }
});

describe("backendTarget: 목적지 고정", () => {
  const backend = new URL(BACKEND);
  for (const path of ["//attacker.example/collect", "/\\attacker.example/x", "/%2F%2Fattacker.example"]) {
    it(`${path}도 백엔드 호스트를 벗어나지 않는다`, () => {
      const target = backendTarget(`https://public.example${path}`, backend);
      assert.ok(target === null || target.host === "backend.example", String(target));
    });
  }
  it("쿼리는 유지한다", () => {
    assert.equal(backendTarget("https://public.example/7?a=1", backend).href, `${BACKEND}/7?a=1`);
  });
});

describe("fetch 핸들러", () => {
  it("허용 목록 밖의 요청은 404이고 외부로 나가지 않는다", async () => {
    for (const path of ["//attacker.example/collect", "/abc", "/api/other"]) {
      const res = await worker.fetch(req(path), env());
      assert.equal(res.status, 404, path);
    }
    const put = await worker.fetch(req("/1", { method: "PUT", body: "x" }), env());
    assert.equal(put.status, 404);
    assert.equal(calls.length, 0);
  });

  it("생성 요청: 필요한 헤더만 넘기고 비밀 헤더는 덮어쓴다", async () => {
    const res = await worker.fetch(
      req("/api/links", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          accept: "application/json",
          cookie: "session=abc",
          authorization: "Bearer stolen",
          "x-worker-secret": "spoofed",
          "x-forwarded-host": "evil.example",
        },
        body: JSON.stringify({ url: "https://example.org" }),
      }),
      env(),
    );
    assert.equal(res.status, 200);
    assert.equal(calls.length, 1);
    const { url, init } = calls[0];
    assert.equal(url, `${BACKEND}/api/links`);
    assert.equal(init.method, "POST");
    assert.equal(init.redirect, "manual");
    const h = init.headers;
    assert.equal(h.get("x-worker-secret"), SECRET);
    assert.equal(h.get("content-type"), "application/json");
    assert.equal(h.get("accept"), "application/json");
    assert.equal(h.get("cookie"), null);
    assert.equal(h.get("authorization"), null);
    assert.equal(h.get("x-forwarded-host"), null);
    assert.equal(new TextDecoder().decode(init.body), '{"url":"https://example.org"}');
  });

  it("리디렉션: 302와 Location을 그대로 돌려주고 호출 제한을 쓰지 않는다", async () => {
    nextResponse = () => new Response(null, { status: 302, headers: { location: "https://example.org/a?x=1#top" } });
    const e = env();
    const res = await worker.fetch(req("/1"), e);
    assert.equal(res.status, 302);
    assert.equal(res.headers.get("location"), "https://example.org/a?x=1#top");
    assert.equal(calls[0].url, `${BACKEND}/1`);
    assert.equal(calls[0].init.headers.get("x-worker-secret"), SECRET);
    assert.equal(e.CREATE_LIMITER.keys.length, 0);
  });

  it("호출 제한 초과면 429이고 백엔드를 부르지 않는다", async () => {
    const e = env({ CREATE_LIMITER: limiter(false) });
    const res = await worker.fetch(
      req("/api/links", { method: "POST", headers: { "CF-Connecting-IP": "203.0.113.7" }, body: "{}" }),
      e,
    );
    assert.equal(res.status, 429);
    assert.equal(res.headers.get("retry-after"), "60");
    assert.deepEqual(e.CREATE_LIMITER.keys, ["203.0.113.7"]);
    assert.equal(calls.length, 0);
  });

  it("WORKER_SECRET이나 BACKEND_URL이 없으면 500이고 백엔드를 부르지 않는다", async () => {
    for (const bad of [{ WORKER_SECRET: undefined }, { WORKER_SECRET: "" }, { BACKEND_URL: "not a url" }]) {
      const res = await worker.fetch(req("/1"), env(bad));
      assert.equal(res.status, 500, JSON.stringify(bad));
    }
    assert.equal(calls.length, 0);
  });

  it("본문이 너무 크면 413", async () => {
    const res = await worker.fetch(req("/api/links", { method: "POST", body: "x".repeat(9000) }), env());
    assert.equal(res.status, 413);
    assert.equal(calls.length, 0);
  });

  it("Content-Length 없는 스트림 본문도 8KB를 넘는 순간 읽기를 멈추고 413", async () => {
    const s = chunkedStream(4096, 100); // 4KB씩 최대 100번(400KB)까지 내보낼 수 있는 스트림
    const request = req("/api/links", { method: "POST", body: s.stream, duplex: "half" });
    assert.equal(request.headers.get("content-length"), null);

    const res = await worker.fetch(request, env());
    assert.equal(res.status, 413);
    assert.equal(calls.length, 0);
    assert.equal(s.cancelled, true, "reader.cancel()이 호출되어야 한다");
    assert.ok(s.pulled <= 4, `8KB를 넘은 직후 멈춰야 한다 (읽은 조각 수: ${s.pulled})`);
  });

  it("Content-Length 없는 스트림 본문이 8KB 이하면 그대로 전달", async () => {
    const s = chunkedStream(4096, 2); // 정확히 8KB
    const res = await worker.fetch(
      req("/api/links", { method: "POST", body: s.stream, duplex: "half" }),
      env(),
    );
    assert.equal(res.status, 200);
    assert.equal(calls[0].init.body.byteLength, 8192);
  });
});

describe("readBodyLimited", () => {
  it("한도 이하면 모든 조각을 이어 붙인다", async () => {
    const s = chunkedStream(3, 3);
    const body = await readBodyLimited(new Request("https://x.example", { method: "POST", body: s.stream, duplex: "half" }), 9);
    assert.deepEqual([...body], [0, 0, 0, 1, 1, 1, 2, 2, 2]);
  });
  it("한도를 1바이트라도 넘으면 null", async () => {
    const s = chunkedStream(5, 2);
    const body = await readBodyLimited(new Request("https://x.example", { method: "POST", body: s.stream, duplex: "half" }), 9);
    assert.equal(body, null);
    assert.equal(s.cancelled, true);
  });
  it("본문이 없으면 빈 배열", async () => {
    const body = await readBodyLimited(new Request("https://x.example", { method: "POST" }), 9);
    assert.equal(body.byteLength, 0);
  });
});

/** chunkSize 바이트 조각을 최대 count개 내보내는 스트림. 읽힌 조각 수와 cancel 여부를 기록한다. */
function chunkedStream(chunkSize, count) {
  const state = { pulled: 0, cancelled: false };
  state.stream = new ReadableStream({
    pull(controller) {
      if (state.pulled >= count) {
        controller.close();
        return;
      }
      controller.enqueue(new Uint8Array(chunkSize).fill(state.pulled % 256));
      state.pulled += 1;
    },
    cancel() {
      state.cancelled = true;
    },
  });
  return state;
}
