// Worker 단위 테스트. 실행: npm test  (node --test)
// 실제 네트워크 대신 globalThis.fetch와 호출 제한 바인딩을 가짜로 바꿔서 확인한다.
import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, it } from "node:test";

import worker from "../src/index.js";
import { backendTarget, classify } from "../src/routing.js";

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
    ["GET", "/1234567890123456789", null], // 19자리: i64 범위 밖
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
});
