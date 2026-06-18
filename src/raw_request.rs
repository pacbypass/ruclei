use crate::scanner::ScanRequest;
use anyhow::Result;
use std::collections::HashMap;

/// Substitute template variables in a raw request string and parse it into a ScanRequest.
///
/// `raw`      — the raw HTTP request string (with \n line endings)
/// `base_url` — the resolved target base URL (used for relative paths)
/// `vars`     — map of variable name → value for substitution ({{BaseURL}}, {{Hostname}}, etc.)
pub fn parse_raw_request(
    raw: &str,
    base_url: &str,
    vars: &HashMap<String, String>,
) -> Result<ScanRequest> {
    // Apply variable substitution
    let raw = substitute_vars(raw, vars);

    // Normalize line endings
    let raw = raw.replace("\r\n", "\n");

    // Split header section from body at the first blank line
    let (header_section, body) = if let Some(pos) = find_header_body_split(&raw) {
        let (h, b) = raw.split_at(pos);
        let b = b.trim_start_matches('\n');
        (
            h.trim_end(),
            if b.is_empty() {
                None
            } else {
                Some(b.to_string())
            },
        )
    } else {
        (raw.trim(), None)
    };

    let mut lines = header_section.lines();

    // First line: METHOD PATH HTTP/VERSION
    let request_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty raw request"))?
        .trim();

    let mut parts = request_line.splitn(3, ' ');
    let method = parts.next().unwrap_or("GET").trim().to_string();
    let path = parts.next().unwrap_or("/").trim().to_string();
    // HTTP version is ignored for our purposes

    // Build absolute URL
    let url = if path.starts_with("http://") || path.starts_with("https://") {
        path
    } else {
        let base = base_url.trim_end_matches('/');
        let p = if path.starts_with('/') {
            &path
        } else {
            &format!("/{}", path)
        };
        format!("{}{}", base, p)
    };

    // Parse headers
    let mut headers: HashMap<String, String> = HashMap::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some(colon_pos) = line.find(':') {
            let name = line[..colon_pos].trim().to_string();
            let value = line[colon_pos + 1..].trim().to_string();
            headers.insert(name, value);
        }
    }

    let mut req = ScanRequest::new(url)
        .with_method(method)
        .with_headers(headers);

    if let Some(body_str) = body {
        req = req.with_body(body_str);
    }

    Ok(req)
}

/// Find the byte position of the blank line separating headers from body.
fn find_header_body_split(raw: &str) -> Option<usize> {
    // Look for \n\n
    raw.find("\n\n")
}

/// Substitute {{Variable}} placeholders in text.
fn substitute_vars(text: &str, vars: &HashMap<String, String>) -> String {
    let mut result = text.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

/// Build the variable map from a base URL for use in raw request substitution.
pub fn build_vars(base_url: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    if let Ok(parsed) = url::Url::parse(base_url) {
        let scheme = parsed.scheme().to_string();
        let host = parsed.host_str().unwrap_or("").to_string();
        let default_port: u16 = if scheme == "https" { 443 } else { 80 };
        let port_num = parsed.port().unwrap_or(default_port);
        let port = port_num.to_string();

        let hostname = if port_num == default_port {
            host.clone()
        } else {
            format!("{}:{}", host, port)
        };

        let path = parsed.path().to_string();
        let root_url = format!("{}://{}", scheme, hostname);

        vars.insert("BaseURL".to_string(), base_url.to_string());
        vars.insert("RootURL".to_string(), root_url.clone());
        vars.insert("Hostname".to_string(), hostname.clone());
        vars.insert("Host".to_string(), host.clone());
        vars.insert("Port".to_string(), port);
        vars.insert("Path".to_string(), path);
        vars.insert("Scheme".to_string(), scheme.clone());

        // lowercase aliases
        vars.insert("hostname".to_string(), hostname);
        vars.insert("host".to_string(), host);
        vars.insert("scheme".to_string(), scheme);
    }

    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_get() {
        let raw = "GET /api/status HTTP/1.1\nHost: example.com\n\n";
        let mut vars = HashMap::new();
        vars.insert("Hostname".to_string(), "example.com".to_string());
        let req = parse_raw_request(raw, "https://example.com", &vars).unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://example.com/api/status");
        assert_eq!(req.headers.get("Host"), Some(&"example.com".to_string()));
    }

    #[test]
    fn test_parse_post_with_body() {
        let raw = "POST /login HTTP/1.1\nHost: target.com\nContent-Type: application/json\n\n{\"user\":\"admin\"}";
        let vars = HashMap::new();
        let req = parse_raw_request(raw, "https://target.com", &vars).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.body, Some("{\"user\":\"admin\"}".to_string()));
    }

    #[test]
    fn test_variable_substitution() {
        let raw = "GET / HTTP/1.1\nHost: {{Hostname}}\n\n";
        let mut vars = HashMap::new();
        vars.insert("Hostname".to_string(), "test.example.com".to_string());
        let req = parse_raw_request(raw, "https://test.example.com", &vars).unwrap();
        assert_eq!(
            req.headers.get("Host"),
            Some(&"test.example.com".to_string())
        );
    }
}
