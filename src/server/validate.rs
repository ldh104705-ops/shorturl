//! 입력 검증. 검사 함수는 DB나 네트워크 없이 동작하는 순수 함수라 단위 테스트하기 쉽습니다.

use std::sync::OnceLock;

/// 저장할 수 있는 URL 최대 길이(바이트).
pub const MAX_URL_LEN: usize = 2048;

/// 단축 대상으로 쓸 수 없는 자기 서비스 호스트 목록. 서버 시작 시 한 번 채운다.
static SELF_HOSTS: OnceLock<Vec<String>> = OnceLock::new();

/// 환경변수에서 자기 서비스 호스트 목록을 만든다.
/// - PUBLIC_BASE_URL          : 단축 주소에 쓰는 공개 주소 (예: https://dhurl.link)
/// - RAILWAY_PUBLIC_DOMAIN    : Railway가 자동으로 넣어 주는 *.up.railway.app 주소
/// - SELF_HOSTS               : 그 밖의 자기 주소, 쉼표로 구분 (예: workers.dev 주소, 앞으로 쓸 도메인)
pub fn init_self_hosts() {
    let env = |k: &str| std::env::var(k).ok();
    let hosts = self_hosts_from(
        env("PUBLIC_BASE_URL").as_deref(),
        env("RAILWAY_PUBLIC_DOMAIN").as_deref(),
        env("SELF_HOSTS").as_deref(),
    );
    let _ = SELF_HOSTS.set(hosts);
}

/// 서버에서 쓰는 자기 호스트 목록 (초기화 전이면 빈 목록)
pub fn self_hosts() -> &'static [String] {
    SELF_HOSTS.get().map(Vec::as_slice).unwrap_or(&[])
}

/// 설정값들을 정규화된 호스트 목록으로 만든다. 주소 형태(https://...)와 호스트만 쓴 형태 모두 받는다.
pub fn self_hosts_from(
    public_base_url: Option<&str>,
    railway_domain: Option<&str>,
    extra: Option<&str>,
) -> Vec<String> {
    let mut hosts: Vec<String> = public_base_url
        .into_iter()
        .chain(railway_domain)
        .chain(extra.into_iter().flat_map(|s| s.split(',')))
        .filter_map(config_host)
        .collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

/// 설정값 하나에서 호스트만 꺼낸다. url 크레이트로 파싱해 한글 도메인(punycode)·대소문자를 맞춘다.
fn config_host(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let with_scheme = if value.contains("://") {
        value.to_string()
    } else {
        format!("https://{value}")
    };
    let url = url::Url::parse(&with_scheme).ok()?;
    url.host_str().map(normalize_host).filter(|h| !h.is_empty())
}

/// 비교용 호스트: 소문자, 끝의 점 제거 ("Example.ORG." → "example.org")
pub fn normalize_host(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}

/// host가 자기 호스트이거나 그 하위 도메인인지 (예: dhurl.link, www.dhurl.link)
fn is_self_host(host: &str, self_hosts: &[String]) -> bool {
    let host = normalize_host(host);
    self_hosts.iter().any(|own| {
        host == *own
            || host
                .strip_suffix(own.as_str())
                .is_some_and(|rest| rest.ends_with('.'))
    })
}

/// 사용자가 입력한 URL을 검사하고, 저장·리디렉션에 쓸 정규화된 문자열을 돌려준다.
///
/// 정규화는 한글 등을 퍼센트 인코딩하는 정도이며 경로·쿼리·프래그먼트(#...)는 그대로 유지된다.
/// `self_hosts`에 있는 자기 서비스 주소는 목적지로 쓸 수 없다(단축 주소끼리 도는 순환 방지).
pub fn normalize_url(input: &str, self_hosts: &[String]) -> Result<String, &'static str> {
    let input = input.trim();
    if input.is_empty() {
        return Err("URL을 입력해 주세요.");
    }
    if input.len() > MAX_URL_LEN {
        return Err("URL이 너무 깁니다. (최대 2048자)");
    }

    let url = url::Url::parse(input).map_err(|_| "올바른 URL 형식이 아닙니다.")?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("http:// 또는 https:// 로 시작하는 주소만 사용할 수 있습니다.");
    }
    let Some(host) = url.host_str().filter(|h| !h.is_empty()) else {
        return Err("도메인(호스트)이 없는 URL입니다.");
    };
    // https://user:pass@... 형태는 피싱에 자주 쓰이므로 받지 않는다.
    if !url.username().is_empty() || url.password().is_some() {
        return Err("아이디·비밀번호가 들어간 URL은 사용할 수 없습니다.");
    }
    if is_self_host(host, self_hosts) {
        return Err("이 서비스의 주소는 단축할 수 없습니다.");
    }

    let normalized = String::from(url);
    if normalized.len() > MAX_URL_LEN {
        return Err("URL이 너무 깁니다. (최대 2048자)");
    }
    Ok(normalized)
}

/// `/123`의 `123` 부분을 번호로 바꾼다. 숫자만으로 된 양수가 아니면 `None`.
///
/// `0123`처럼 0으로 시작하는 값은 거부해서, 한 번호에 주소가 하나만 대응하도록 한다.
pub fn parse_code(segment: &str) -> Option<i64> {
    let digits_only = !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit());
    if !digits_only || segment.starts_with('0') {
        return None;
    }
    segment.parse::<i64>().ok() // i64 범위를 넘으면 None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_path_query_and_fragment() {
        let u = normalize_url("  https://example.org/a/b?x=1&y=2#frag ", &[]).unwrap();
        assert_eq!(u, "https://example.org/a/b?x=1&y=2#frag");
    }

    #[test]
    fn accepts_http_and_https() {
        assert!(normalize_url("http://example.org", &[]).is_ok());
        assert!(normalize_url("https://example.org", &[]).is_ok());
    }

    #[test]
    fn rejects_bad_input() {
        for bad in [
            "",
            "   ",
            "example.org",
            "ftp://example.org",
            "javascript:alert(1)",
            "mailto:a@b.c",
            "http://",
            "https://exa mple.org",
        ] {
            assert!(normalize_url(bad, &[]).is_err(), "should reject {bad:?}");
        }
        let too_long = format!("https://example.org/{}", "a".repeat(MAX_URL_LEN));
        assert!(normalize_url(&too_long, &[]).is_err());
    }

    #[test]
    fn parses_only_plain_positive_numbers() {
        assert_eq!(parse_code("123"), Some(123));
        assert_eq!(parse_code("2048"), Some(2048));
        for bad in [
            "",
            "0",
            "0123",
            "abc",
            "12a",
            "-1",
            "+1",
            "1.5",
            "99999999999999999999",
        ] {
            assert_eq!(parse_code(bad), None, "should reject {bad:?}");
        }
    }

    #[test]
    fn rejects_username_or_password() {
        for bad in [
            "https://user@example.org/",
            "https://user:pass@example.org/",
            "https://:pass@example.org/",
            "http://example.org@evil.example/",
        ] {
            assert_eq!(
                normalize_url(bad, &[]),
                Err("아이디·비밀번호가 들어간 URL은 사용할 수 없습니다."),
                "{bad}"
            );
        }
        // @가 경로나 쿼리에 있는 건 괜찮다
        assert!(normalize_url("https://example.org/a@b?x=c@d", &[]).is_ok());
    }

    fn own() -> Vec<String> {
        self_hosts_from(
            Some("https://DhUrl.link/"),
            Some("shorturl-production-d0d840c3.up.railway.app"),
            Some(" shorturl.example.workers.dev , future-domain.kr. ,"),
        )
    }

    #[test]
    fn builds_self_host_list() {
        assert_eq!(
            own(),
            vec![
                "dhurl.link",
                "future-domain.kr",
                "shorturl-production-d0d840c3.up.railway.app",
                "shorturl.example.workers.dev",
            ]
        );
        assert!(self_hosts_from(None, None, None).is_empty());
        assert!(self_hosts_from(Some(""), Some("  "), Some(",,")).is_empty());
        // 한글 도메인은 punycode로 맞춘다
        assert_eq!(
            self_hosts_from(None, None, Some("단축.한국")),
            vec!["xn--6j1bx91c.xn--3e0b707e"]
        );
    }

    #[test]
    fn rejects_own_hosts() {
        let own = own();
        for bad in [
            "https://dhurl.link/1",
            "https://DHURL.LINK/1",
            "https://dhurl.link./1",
            "http://dhurl.link:8080/1",
            "https://www.dhurl.link/x",
            "https://Shorturl-Production-D0D840C3.up.railway.app/5",
            "https://shorturl.example.workers.dev/5",
            "https://future-domain.kr/",
            "https://단축.한국/1",
        ] {
            let own_with_idn =
                [own.clone(), self_hosts_from(None, None, Some("단축.한국"))].concat();
            assert_eq!(
                normalize_url(bad, &own_with_idn),
                Err("이 서비스의 주소는 단축할 수 없습니다."),
                "{bad}"
            );
        }
        for ok in [
            "https://notdhurl.link/",
            "https://dhurl.link.evil.example/",
            "https://other-app.up.railway.app/",
            "https://example.org/?next=https://dhurl.link/1",
        ] {
            assert!(normalize_url(ok, &own).is_ok(), "{ok}");
        }
    }
}
