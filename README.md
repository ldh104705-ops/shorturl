# 숫자형 단축 URL 서비스

## 배포 URL

- 서비스 URL: https://dhurl.link
- 테스트 단축 URL: <https://dhurl.link/7>
- 테스트 원본 URL: https://www.google.com

### 배포 구조

```
브라우저 → Cloudflare Workers (화면·공개 주소 dhurl.link)
         → Railway (Rust 백엔드: 생성 API, /숫자 리디렉션)
         → Railway PostgreSQL
```

### 비밀 값 관리

DB 비밀번호(`DATABASE_URL`)와 `WORKER_SECRET` 같은 실제 비밀 값은 **Railway 환경변수와 Cloudflare Secret에만** 있고,
저장소에는 들어 있지 않습니다. 저장소에는 빈 예시 파일(`.env.example`, `cloudflare/.dev.vars.example`)만 있습니다.

## 소개

긴 URL을 넣으면 `https://도메인/123`처럼 **숫자로만 된** 단축 주소를 만들어 주는 서비스입니다.
화면과 서버를 모두 [Dioxus](https://dioxuslabs.com) 0.7 풀스택(Rust)으로 구현했고, 데이터는 PostgreSQL에 저장합니다.

- 회원가입·로그인 없이 누구나 생성할 수 있습니다.
- 한 번 발급한 번호와 원본 URL의 연결은 바꿀 수 없습니다(수정·삭제·만료 없음).
- `/숫자`로 접속하면 서버가 HTTP 302 리디렉션으로 원본 URL로 보냅니다.

### 프로젝트 구조

```
src/
  main.rs              진입점. 서버 라우터에 /{code} 리디렉션 라우트 추가
  app.rs               화면: URL 입력·생성·결과 표시·복사 (브라우저 wasm)
  api.rs               서버 함수 POST /api/links (화면에서 호출)
  server/              서버 빌드에서만 컴파일
    db.rs              DB 연결, 마이그레이션, INSERT/SELECT
    validate.rs        URL 검증, 번호 파싱 (+ 단위 테스트)
    redirect.rs        GET /{code} → 302, 없으면 404, 변경 메서드는 405
migrations/
  0001_create_links.sql  테이블·제약·불변 트리거
Dockerfile             Railway 배포용 멀티 스테이지 이미지
cloudflare/            Cloudflare Worker (공개 주소, 정적 화면, Railway 프록시)
  wrangler.jsonc       Worker 설정 (정적 파일, BACKEND_URL)
  src/index.js         정적 파일에 없는 요청을 Railway로 전달
  scripts/build-web.mjs  웹 화면 정적 파일(spa 빌드) 생성
docker-compose.yml     로컬 PostgreSQL
.env.example           환경변수 예시
```

## 로컬 실행 방법

### 1. 준비물

- [Rust](https://rustup.rs) (stable) + wasm 타깃
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- Dioxus CLI 0.7 (`dx`)
  ```bash
  cargo install dioxus-cli --version 0.7.10 --locked
  ```
  (또는 [GitHub 릴리스](https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.10)에서 `dx` 실행 파일을 받아 PATH에 넣어도 됩니다.)
- Docker (로컬 PostgreSQL용)
- Windows는 Visual Studio C++ Build Tools(MSVC)가 필요합니다.

### 2. DB 준비

```bash
docker compose up -d
```

`postgres:17` 컨테이너가 `127.0.0.1:5432`에 뜹니다(사용자·비밀번호·DB 이름 모두 `shorturl`, 로컬 전용 값).
데이터는 Docker 볼륨 `pgdata`에 저장되므로 컨테이너나 서버를 재시작해도 유지됩니다.

테이블은 **서버가 시작할 때 자동으로** 만들어집니다(`sqlx::migrate!`가 `migrations/`의 SQL을 한 번만 적용).
따로 SQL을 실행할 필요가 없습니다.

### 3. 환경변수

```bash
cp .env.example .env
```

| 이름 | 설명 | 로컬 기본값 |
| --- | --- | --- |
| `DATABASE_URL` | PostgreSQL 접속 주소 (필수) | `postgres://shorturl:shorturl@localhost:5432/shorturl` |
| `PUBLIC_BASE_URL` | 화면에 표시할 단축 주소 앞부분 (끝에 `/` 없이) | `http://localhost:8080` |
| `WORKER_SECRET` | Cloudflare Worker와 공유하는 비밀 값 (**필수**, 비면 서버가 시작되지 않음) | 직접 생성 (예: `openssl rand -hex 32`) |
| `SELF_HOSTS` | 단축 대상에서 막을 자기 주소, 쉼표 구분 (선택) | 비워 둠 |
| `IP` | 서버가 바인딩할 주소 (Dioxus가 읽음) | `127.0.0.1` (Docker 이미지는 `0.0.0.0`) |
| `PORT` | 서버가 들을 포트 (Dioxus가 읽음) | `8080` |

`.env`, `.env.*`는 `.gitignore`에 들어 있어 커밋되지 않습니다(`.env.example`만 커밋).

### 4. 실행

```bash
dx serve --platform web --fullstack
```

브라우저에서 <http://localhost:8080> 을 엽니다. 첫 빌드는 몇 분 걸립니다.

서버는 `/api/*`와 `/{숫자}` 요청에 Worker의 비밀 헤더를 요구하므로, 이 주소에서 바로 생성하면 403이 납니다.
로컬에서 화면까지 확인하려면 Worker를 앞에 띄워서 <http://127.0.0.1:8787> 로 접속합니다.

```bash
cd cloudflare
cp .dev.vars.example .dev.vars   # WORKER_SECRET에 .env와 같은 값, BACKEND_URL=http://127.0.0.1:8080
npm install
npm run build:web
npm run dev
```

### 5. 테스트

```bash
cargo test --features server --no-default-features   # URL 검증, 번호 파싱, 비밀 헤더 확인
cd cloudflare && npm test                            # Worker 라우팅·헤더·호출 제한
```

## 배포 방법과 라우팅 구성

### 구성

| 역할 | 서비스 |
| --- | --- |
| 웹 화면·공개 접점 | Cloudflare Workers (`cloudflare/`) |
| Rust 백엔드·생성 API·리디렉션 처리 | Railway (이 저장소의 `Dockerfile`) |
| 영구 DB | Railway PostgreSQL |

브라우저는 Cloudflare 주소로만 접속합니다. 화면 파일은 Cloudflare가 직접 내려주고,
생성 API(`/api/links`)와 리디렉션(`/{숫자}`)은 Worker가 Railway의 Dioxus 서버로 전달합니다.

### Railway (백엔드 + DB)

1. Railway 프로젝트에 **PostgreSQL**을 추가합니다.
2. 같은 프로젝트에 이 GitHub 저장소로 서비스를 추가합니다. 저장소 루트의 `Dockerfile`로 빌드됩니다.
   - 빌드 단계: Rust + `wasm32-unknown-unknown` + `dx` 0.7.10으로 `dx bundle --platform web --fullstack --release`
   - 실행 단계: `debian:trixie-slim`에 서버 실행 파일과 `public/` 폴더만 복사
3. 서비스 환경변수를 설정합니다.

   | 이름 | 값 |
   | --- | --- |
   | `DATABASE_URL` | Railway PostgreSQL 변수 참조 (`${{Postgres.DATABASE_URL}}`) |
   | `PUBLIC_BASE_URL` | 공개 주소 (예: `https://도메인`) |
   | `WORKER_SECRET` | Worker와 같은 비밀 값 (필수) |
   | `SELF_HOSTS` | 그 밖의 자기 주소, 쉼표 구분 (예: `shorturl.<계정>.workers.dev`) |

   `IP=0.0.0.0`은 이미지에 들어 있고, `PORT`는 Railway가 자동으로 넣어 줍니다(지정하지 않으면 8080).
   Railway가 자동으로 주는 `RAILWAY_PUBLIC_DOMAIN`도 자기 주소로 막습니다.
4. 배포되면 서버가 시작하면서 마이그레이션을 적용합니다.

DB 비밀번호 등 실제 값은 Railway 환경변수에만 넣고, 저장소·이미지에는 포함하지 않습니다.

### Cloudflare Workers (웹 화면 + 공개 주소)

`cloudflare/` 폴더의 Worker가 공개 주소입니다. 설정은 `cloudflare/wrangler.jsonc`, 코드는 `cloudflare/src/index.js`입니다.

| 요청 | 처리 |
| --- | --- |
| `/`, `/assets/*` (index.html, wasm, js, css) | **Workers Static Assets**가 `cloudflare/public/`에서 직접 응답 |
| `POST /api/links` | IP별 분당 20회 제한 후 Railway로 전달. 초과하면 429 |
| `GET`·`HEAD /{숫자}` (0으로 시작하지 않는 양수) | Railway로 전달, `redirect: "manual"`로 302와 `Location`을 그대로 반환. 호출 제한 없음 |
| 그 밖의 모든 요청 | Worker가 직접 404 (백엔드로 보내지 않음) |

정적 파일에 있는 경로는 Cloudflare가 바로 응답하고, 없는 경로만 Worker 코드가 실행됩니다.

**Worker 보안 처리** (`cloudflare/src/index.js`, `cloudflare/src/routing.js`)

- **목적지 고정**: `new URL(BACKEND_URL)`에서 출발해 경로와 쿼리만 바꾸고, 전송 직전에 프로토콜·호스트·포트가
  `BACKEND_URL`과 같은지 다시 확인합니다. 다르면 502. `//다른호스트/...` 같은 경로로 요청이 밖으로 새지 않습니다.
- **허용 목록**: 위 표의 두 가지 요청만 백엔드로 보냅니다.
- **헤더 정리**: `Content-Type`, `Accept`, `Accept-Language`만 새로 만들어 넘깁니다. `Cookie`, `Authorization` 등은 넘기지 않습니다.
- **공유 비밀 헤더**: 백엔드로 보내는 요청에 `X-Worker-Secret`을 항상 덮어써서 넣습니다. 값은 Cloudflare Secret `WORKER_SECRET`에서만 읽습니다.
- **호출 제한**: Workers Rate Limiting 바인딩(`CREATE_LIMITER`, `CF-Connecting-IP` 기준 60초에 20회).
  Cloudflare 위치별 근사치라 정확히 20회에서 끊기지 않을 수 있습니다.
- **본문 크기**: 생성 요청 본문이 8KB를 넘으면 413.

**백엔드 쪽 확인** (`src/server/auth.rs`): 생성 API와 `/{code}` 라우트는 `X-Worker-Secret`이 `WORKER_SECRET`과 같아야 하고(상수 시간 비교),
다르거나 없으면 403입니다. 그래서 Railway 주소로 직접 호출하면 막힙니다.
검사는 경로 문자열 모양으로 판단하지 않고 라우트 자체에 붙어 있습니다
(`/{code}`는 `route_layer`, `/api/links`는 라우터가 매칭한 라우트(`MatchedPath`)를 보고 **본문을 해석하기 전에** 검사하는 미들웨어와
서버 함수 본문 맨 앞의 검사 두 겹). `/%31`처럼 인코딩된 경로나 잘못된 본문도 비밀이 없으면 403입니다.

Railway 주소는 코드가 아니라 `wrangler.jsonc`의 `vars.BACKEND_URL`에 둡니다.

**웹 화면 빌드 방식**: 풀스택 브라우저 빌드는 서버가 SSR한 HTML을 하이드레이션하도록 되어 있어서,
빈 `index.html`만 정적으로 제공하면 화면이 그려지지 않습니다. 그래서 Cloudflare용으로는
**클라이언트 전용 빌드(`spa` 기능)**를 씁니다. 이 빌드는 하이드레이션 없이 바로 그리고,
생성 요청을 같은 출처의 `POST /api/links`로 보냅니다(서버 함수와 같은 요청·응답 형식).

| Cargo 기능 | 용도 |
| --- | --- |
| `web` + `fullstack` (기본값) | `dx serve` 개발, Railway 이미지의 브라우저 코드 |
| `server` | Railway 서버 (SSR, `/api/links`, `/{숫자}`, DB) |
| `spa` | Cloudflare 정적 파일 (`--no-default-features --features spa`) |

### 배포 순서

1. **Railway**: 위 절대로 백엔드와 DB를 배포하고, `PUBLIC_BASE_URL`을 공개 주소(Cloudflare 쪽 도메인)로 설정합니다.
2. **웹 화면 정적 파일 빌드** (Rust·`dx` 필요):
   ```bash
   cd cloudflare
   npm install
   npm run build:web   # dx bundle --platform web --release --no-default-features --features spa → cloudflare/public/
   ```
3. **`wrangler.jsonc`의 `BACKEND_URL`**을 Railway 공개 주소로 맞춥니다.
4. **로컬 확인(선택)**: `npm run dev` → <http://127.0.0.1:8787>
5. **비밀 값 등록과 배포**:
   ```bash
   npx wrangler login
   npx wrangler secret put WORKER_SECRET   # Railway의 WORKER_SECRET과 같은 값 입력
   npm run deploy
   ```
   `https://shorturl.<계정>.workers.dev`로 배포됩니다. 커스텀 도메인은 Cloudflare 대시보드에서 이 Worker에 연결합니다.

비밀 값은 저장소에 넣지 않습니다. `WORKER_SECRET`은 Railway 환경변수와 Cloudflare Secret에만 두고,
로컬 테스트용 값은 git에 올라가지 않는 `.env`, `cloudflare/.dev.vars`에만 둡니다.
`BACKEND_URL`은 공개 주소라 설정 파일에 두고, DB 접속 정보는 Railway에만 있습니다.

## 숫자 발급 방식, 중복 방지, 생성 후 불변

### 요청 흐름

| 요청 | 처리 |
| --- | --- |
| `GET /` | Dioxus 화면 |
| `POST /api/links` `{"url": "..."}` | 검증 → 번호 발급 → `{"id", "short_url", "original_url"}` 응답. 잘못된 URL은 400 |
| `GET /{숫자}` | 302 + `Location: 원본 URL`. 없는 번호나 숫자가 아닌 경로는 404 안내 페이지 |
| `POST`·`PUT`·`PATCH`·`DELETE /{숫자}` | 항상 405 (변경 불가) |

### URL 검증 (서버)

`src/server/validate.rs`의 `normalize_url`이 서버에서 검사합니다. 화면의 `type="url"` 검사는 편의용이고, 실제 판단은 서버가 합니다.

- 앞뒤 공백 제거 후 비어 있으면 거부
- `url` 크레이트로 파싱되지 않으면 거부
- 스킴이 `http`·`https`가 아니면 거부 (`ftp:`, `javascript:`, `mailto:` 등)
- 호스트가 없으면 거부
- `https://user:pass@...`처럼 아이디·비밀번호가 들어 있으면 거부
- 목적지가 이 서비스 자신이면 거부 (`PUBLIC_BASE_URL`의 호스트, `RAILWAY_PUBLIC_DOMAIN`, `SELF_HOSTS`와 그 하위 도메인).
  파싱한 호스트로 비교하고 대소문자와 끝의 점(`dhurl.link.`)을 맞춘 뒤 비교합니다.
- 2048바이트를 넘으면 거부

통과한 URL은 표준 형식으로 정규화됩니다(한글 등은 퍼센트 인코딩). **경로·쿼리·프래그먼트(`#...`)는 그대로 유지**되고,
이 문자열을 그대로 `Location` 헤더에 넣기 때문에 리디렉션 후에도 보존됩니다.

### 숫자 발급 방식

번호는 애플리케이션이 계산하지 않고 **PostgreSQL이 발급**합니다.

```sql
id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY
```

- IDENTITY 시퀀스는 DB 안에서 원자적으로 값을 늘리므로, 요청이 동시에 와도 두 요청이 같은 번호를 받지 않습니다.
- 번호는 1부터 늘어나지만 **연속을 보장하지 않습니다.** 같은 URL을 다시 요청해 `ON CONFLICT`로 끝난 INSERT도 시퀀스 값을 하나 쓰기 때문에 중간 번호가 비어 있을 수 있습니다.
- 주소 경로는 `0`으로 시작하지 않는 양의 정수만 받습니다(`/0123` → 404). 한 번호에 주소가 하나만 대응하게 하기 위해서입니다.

### 같은 URL을 다시 입력하면

**기존 번호를 그대로 돌려줍니다.**

```sql
INSERT INTO links (original_url) VALUES ($1)
ON CONFLICT (original_url) DO NOTHING
RETURNING id;
-- 아무 행도 안 돌아오면(이미 있음) → SELECT id FROM links WHERE original_url = $1
```

비교는 정규화된 URL 기준입니다. 예를 들어 `https://Example.org`와 `https://example.org/`는 같은 주소로 봅니다.

### 중복 방지

| 무엇을 | 어떻게 |
| --- | --- |
| 같은 번호가 두 URL에 배정 | `id`가 `PRIMARY KEY`이고, IDENTITY 시퀀스가 번호를 발급 |
| 클라이언트가 번호를 지정 | `GENERATED ALWAYS`라 INSERT에 `id`를 넣으면 DB가 거부 |
| 같은 URL이 여러 행으로 저장 | `original_url`에 `UNIQUE` 제약 + `ON CONFLICT DO NOTHING` |
| 같은 URL 동시 요청 | UNIQUE 제약이 한 건만 통과시키고, 나머지는 기존 행의 번호를 조회 |

### 생성 후 불변

여러 겹으로 막습니다. 화면에서 버튼을 숨기는 것만으로 끝내지 않습니다.

1. **기능이 없음**: 코드에 UPDATE·DELETE 쿼리나 수정·삭제·만료 API가 없습니다. `src/server/db.rs`는 INSERT와 SELECT만 합니다.
2. **덮어쓰기 대신 `DO NOTHING`**: 이미 있는 URL이 들어와도 기존 행을 바꾸지 않습니다.
3. **변경 요청 명시적 거부**: `/{숫자}`에 대한 `POST`·`PUT`·`PATCH`·`DELETE`는 항상 `405 Method Not Allowed`를 돌려줍니다 (`src/server/redirect.rs`의 `reject_change`).
4. **DB 트리거**: `links` 테이블에 `BEFORE UPDATE OR DELETE`, `BEFORE TRUNCATE` 트리거가 있어서, 코드 버그나 DB 직접 접속으로도 기존 행을 바꿀 수 없습니다.
5. **번호 재사용 없음**: 행을 지울 수 없고 IDENTITY 시퀀스는 되돌아가지 않으므로, 한 번 쓴 번호가 다른 URL에 다시 배정되지 않습니다.

## 확인 시나리오 결과와 재현 방법

### 결과 (배포 환경)

| 확인 | 결과 |
| --- | --- |
| `GET https://dhurl.link/` | `200`, 생성 화면 |
| `GET https://dhurl.link/7` | `302`, `Location: https://www.google.com/` |
| `GET https://dhurl.link/999999999` | `404` |
| `POST https://dhurl.link/api/links` `{"url":"ftp://x.org"}` | `400` + 안내 문구 |
| Railway 주소로 직접 `GET /7` (비밀 헤더 없음) | `403` |

### 결과 (로컬)

로컬(`dx serve` 개발·`--release` 빌드 + Docker PostgreSQL)에서 확인한 결과입니다.

| 시나리오 | 결과 |
| --- | --- |
| 시크릿 창에서 URL 입력 → 생성 → 복사 | 로그인 없이 `/숫자` 주소 생성, 복사 동작 |
| 생성한 주소를 다른 브라우저에서 열기 | 원본 URL로 이동, `?x=1#top` 유지 |
| 같은 URL 다시 생성 | 같은 번호 반환 |
| 서로 다른 URL 20개 동시 생성 | 20개 모두 서로 다른 번호, DB의 전체 행 수 = 서로 다른 번호 수 |
| `GET /{번호}` | `302`, `Location`에 원본 URL(경로·쿼리·프래그먼트 유지) |
| `ftp://x.org`, `javascript:alert(1)`, 빈 값 | `400` + 안내 문구 |
| `GET /999999`, `/0123`, `/abc` | `404` 안내 페이지 |
| `PUT`·`PATCH`·`DELETE`·`POST /{번호}` | `405`, 이후에도 원래 URL로 이동 |
| DB에서 직접 `UPDATE`·`DELETE` | 트리거가 오류로 거부 |
| DB에서 직접 `id`를 지정해 `INSERT` | `GENERATED ALWAYS`로 거부 |
| 서버 재시작, DB 재시작 후 `GET /{번호}` | 같은 원본 URL로 이동 |

### 재현 방법

`B`는 배포 주소(`https://dhurl.link`) 또는 로컬 Worker(`http://127.0.0.1:8787`)입니다.
Railway 서버(`:8080`)로 직접 보내면 비밀 헤더가 없어 403이 됩니다.

```bash
B=https://dhurl.link

# 생성
curl -s -X POST $B/api/links \
  -H "content-type: application/json" \
  -d '{"url":"https://example.org/a/b?x=1#frag"}'

# 이동 (Location 헤더 확인)
curl -si $B/7 | head -n 5

# 잘못된 URL → 400
curl -si -X POST $B/api/links \
  -H "content-type: application/json" -d '{"url":"ftp://x.org"}'

# 없는 번호 → 404
curl -si $B/999999999 | head -n 1

# 변경 시도 → Worker는 404(백엔드로 보내지 않음), 로컬 백엔드에 비밀 헤더와 함께 보내면 405
curl -si -X PUT $B/7 -d 'https://evil.example' | head -n 1

# 동시 생성 20개 → id가 모두 달라야 함 (배포 환경은 IP당 분당 20회 제한)
for i in $(seq 1 20); do curl -s -X POST $B/api/links \
  -H "content-type: application/json" -d "{\"url\":\"https://example.com/c$i\"}" & done; wait

# DB 직접 변경 시도 → 트리거 오류
docker compose exec db psql -U shorturl -c "UPDATE links SET original_url='x' WHERE id=1;"
```
