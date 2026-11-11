-- 숫자 ↔ 원본 URL 매핑 테이블
CREATE TABLE links (
    -- 단축 번호. DB가 직접 발급하며(GENERATED ALWAYS) 클라이언트가 값을 지정할 수 없다.
    -- PRIMARY KEY 이므로 같은 번호가 두 번 저장될 수 없다.
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- 같은 원본 URL은 한 번만 저장된다(다시 요청하면 기존 번호를 돌려준다).
    original_url TEXT        NOT NULL UNIQUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 생성 후 불변: 애플리케이션 코드에 버그가 있거나 누군가 DB에 직접 접속해도
-- 기존 행을 수정·삭제할 수 없도록 DB 차원에서 막는다.
CREATE FUNCTION links_forbid_change() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'links 테이블의 행은 변경할 수 없습니다 (% 차단)', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER links_no_update_delete
    BEFORE UPDATE OR DELETE ON links
    FOR EACH ROW EXECUTE FUNCTION links_forbid_change();

CREATE TRIGGER links_no_truncate
    BEFORE TRUNCATE ON links
    FOR EACH STATEMENT EXECUTE FUNCTION links_forbid_change();
