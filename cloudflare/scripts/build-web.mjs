// 웹 화면 정적 파일을 만들어 cloudflare/public/ 에 복사한다.
//
// 1. 저장소 루트에서 클라이언트 전용(spa 기능) 릴리스 번들을 만든다.
//    풀스택 브라우저 빌드는 서버가 SSR한 HTML을 하이드레이션하도록 되어 있어서
//    정적 index.html 만으로는 화면이 그려지지 않는다. spa 빌드는 빈 페이지에서 바로 그린다.
// 2. 번들의 public/ (index.html, assets/*.wasm·js·css)을 cloudflare/public/ 로 복사한다.
import { execSync } from "node:child_process";
import { cpSync, existsSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, "..", "..");
const bundled = join(repoRoot, "target", "dx", "shorturl", "release", "web", "public");
const out = join(here, "..", "public");

// 이전 번들(풀스택 빌드 등)의 파일이 섞이지 않게 먼저 비운다.
rmSync(bundled, { recursive: true, force: true });

execSync("dx bundle --platform web --release --no-default-features --features spa", {
  cwd: repoRoot,
  stdio: "inherit",
});

if (!existsSync(join(bundled, "index.html"))) {
  console.error(`번들 결과를 찾지 못했습니다: ${bundled}`);
  process.exit(1);
}

rmSync(out, { recursive: true, force: true });
cpSync(bundled, out, { recursive: true });
console.log(`정적 파일 복사 완료: ${out}`);
