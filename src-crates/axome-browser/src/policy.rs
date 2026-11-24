use url::Url;

/// Validates a navigation target URL. Allows `http://` and `https://` only.
pub fn validate_navigation_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("URL must not be empty".to_owned());
    }
    let parsed = Url::parse(trimmed).map_err(|err| format!("Invalid URL '{trimmed}': {err}"))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        scheme => Err(format!(
            "Blocked URL scheme '{scheme}://'. Only http:// and https:// URLs are allowed."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_http_and_https() {
        assert!(validate_navigation_url("http://localhost:5173").is_ok());
        assert!(validate_navigation_url("https://example.com/docs?q=1").is_ok());
    }

    #[test]
    fn blocks_dangerous_schemes() {
        for url in [
            "file:///etc/passwd",
            "chrome://settings",
            "javascript:alert(1)",
            "data:text/html,<h1>hi</h1>",
            "about:blank",
            "",
        ] {
            assert!(
                validate_navigation_url(url).is_err(),
                "expected '{url}' to be rejected"
            );
        }
    }
}
