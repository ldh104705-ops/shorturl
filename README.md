# 숫자형 단축 URL 서비스

## 배포 URL

배포 예정

- 서비스 URL: (배포 예정)
- 테스트 단축 URL: (배포 예정)
- 테스트 원본 URL: (배포 예정)

## 소개

긴 URL을 넣으면 `http://localhost:8080/123`처럼 **숫자로만 된** 단축 주소를 만들어 주는 서비스입니다.
화면과 서버를 모두 [Dioxus](https://dioxuslabs.com) 0.7 풀스택(Rust)으로 구현했고, 데이터는 PostgreSQL에 저장합니다.

- 회원가입·로그인 없이 누구나 생성할 수 있습니다.
- 한 번 발급한 번호와 원본 URL의 연결은 바꿀 수 없습니다(수정·삭제·만료 없음).
- `/숫자`로 접속하면 서버가 HTTP 302 리디렉션으로 원본 URL로 보냅니다.

## 프로젝트 구조

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

`postgres:17` 컨테이너가 `localhost:5432`에 뜹니다(사용자·비밀번호·DB 이름 모두 `shorturl`, 로컬 전용 값).
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
| `PUBLIC_BASE_URL` | 화면에 표시할 단축 주소 앞부분 | `http://localhost:8080` |
| `IP`, `PORT` | 서버 바인딩 주소 (Dioxus가 읽음, 선택) | dx 기본값 |

`.env`는 `.gitignore`에 들어 있어 커밋되지 않습니다.

### 4. 실행

```bash
dx serve --platform web --fullstack
```

브라우저에서 <http://localhost:8080> 을 엽니다. 첫 빌드는 몇 분 걸립니다.

### 5. 테스트

```bash
cargo test --features server --no-default-features
```

URL 검증과 번호 파싱 규칙을 단위 테스트합니다.

## 동작 방식

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
- 2048바이트를 넘으면 거부

통과한 URL은 표준 형식으로 정규화됩니다(한글 등은 퍼센트 인코딩). **경로·쿼리·프래그먼트(`#...`)는 그대로 유지**되고,
이 문자열을 그대로 `Location` 헤더에 넣기 때문에 리디렉션 후에도 보존됩니다.

### 숫자 발급 방식

번호는 애플리케이션이 계산하지 않고 **PostgreSQL이 발급**합니다.

```sql
id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY
```

- IDENTITY 시퀀스는 DB 안에서 원자적으로 값을 늘리므로, 요청이 동시에 와도 두 요청이 같은 번호를 받지 않습니다.
- 번호는 1부터 늘어나지만 **연속을 보장하지 않습니다.** 같은 URL을 다시 요청해 `ON CONFLICT`로 끝난 INSERT도 시퀀스 값을 하나 쓰기 때문에 중간 번호가 비어 있을 수 있습니다. (과제에서 연속성은 요구하지 않음)
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

### 생성 후 불변 설계

여러 겹으로 막습니다. 화면에서 버튼을 숨기는 것만으로 끝내지 않습니다.

1. **기능이 없음**: 코드에 UPDATE·DELETE 쿼리나 수정·삭제·만료 API가 없습니다. `src/server/db.rs`는 INSERT와 SELECT만 합니다.
2. **덮어쓰기 대신 `DO NOTHING`**: 이미 있는 URL이 들어와도 기존 행을 바꾸지 않습니다.
3. **변경 요청 명시적 거부**: `/{숫자}`에 대한 `POST`·`PUT`·`PATCH`·`DELETE`는 항상 `405 Method Not Allowed`를 돌려줍니다 (`src/server/redirect.rs`의 `reject_change`).
4. **DB 트리거**: `links` 테이블에 `BEFORE UPDATE OR DELETE`, `BEFORE TRUNCATE` 트리거가 있어서, 코드 버그나 DB 직접 접속으로도 기존 행을 바꿀 수 없습니다.
5. **번호 재사용 없음**: 행을 지울 수 없고 IDENTITY 시퀀스는 되돌아가지 않으므로, 한 번 쓴 번호가 다른 URL에 다시 배정되지 않습니다.

## 확인 시나리오와 결과 (로컬)

아래는 로컬(`dx serve` + Docker PostgreSQL)에서 확인한 결과입니다.

| 시나리오 | 결과 |
| --- | --- |
| `https://example.org/a/b?x=1&y=2#frag` 생성 | `200`, `id: 1` |
| 같은 URL 다시 생성 | 같은 `id: 1` 반환 |
| 서로 다른 URL 20개 동시 생성 | 20개 모두 서로 다른 번호 |
| `GET /1` | `302`, `Location: https://example.org/a/b?x=1&y=2#frag` (경로·쿼리·프래그먼트 유지) |
| `example.org`, `ftp://x.org`, `javascript:alert(1)`, 빈 값 | `400` + 안내 문구 |
| `GET /999999`, `/0123`, `/abc` | `404` 안내 페이지 |
| `PUT`·`PATCH`·`DELETE`·`POST /1` | `405`, 이후에도 `/1`은 원래 URL로 이동 |
| DB에서 직접 `UPDATE`·`DELETE` | 트리거가 오류로 거부 |
| DB에서 직접 `id`를 지정해 `INSERT` | `GENERATED ALWAYS`로 거부 |
| 서버 재시작 후 `GET /1` | 같은 원본 URL로 이동 |

### 직접 재현하기

```bash
# 생성
curl -s -X POST http://localhost:8080/api/links \
  -H "content-type: application/json" \
  -d '{"url":"https://example.org/a/b?x=1#frag"}'

# 이동 (Location 헤더 확인)
curl -si http://localhost:8080/1 | head -n 5

# 잘못된 URL → 400
curl -si -X POST http://localhost:8080/api/links \
  -H "content-type: application/json" -d '{"url":"ftp://x.org"}'

# 없는 번호 → 404
curl -si http://localhost:8080/999999 | head -n 1

# 변경 시도 → 405
curl -si -X PUT http://localhost:8080/1 -d 'https://evil.example' | head -n 1

# DB 직접 변경 시도 → 트리거 오류
docker compose exec db psql -U shorturl -c "UPDATE links SET original_url='x' WHERE id=1;"
```
