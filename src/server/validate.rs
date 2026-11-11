//! 입력 검증. DB나 네트워크 없이 동작하는 순수 함수라 단위 테스트하기 쉽습니다.

/// 저장할 수 있는 URL 최대 길이(바이트).
pub const MAX_URL_LEN: usize = 2048;

/// 사용자가 입력한 URL을 검사하고, 저장·리디렉션에 쓸 정규화된 문자열을 돌려준다.
///
/// 정규화는 한글 등을 퍼센트 인코딩하는 정도이며 경로·쿼리·프래그먼트(#...)는 그대로 유지된다.
pub fn normalize_url(input: &str) -> Result<String, &'static str> {
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
    if url.host_str().is_none_or(str::is_empty) {
        return Err("도메인(호스트)이 없는 URL입니다.");
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
        let u = normalize_url("  https://example.org/a/b?x=1&y=2#frag ").unwrap();
        assert_eq!(u, "https://example.org/a/b?x=1&y=2#frag");
    }

    #[test]
    fn accepts_http_and_https() {
        assert!(normalize_url("http://example.org").is_ok());
        assert!(normalize_url("https://example.org").is_ok());
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
            assert!(normalize_url(bad).is_err(), "should reject {bad:?}");
        }
        let too_long = format!("https://example.org/{}", "a".repeat(MAX_URL_LEN));
        assert!(normalize_url(&too_long).is_err());
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
}
