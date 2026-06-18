use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub content_length: u64,
    pub response_time: Duration,
    pub final_url: String,
}

impl HttpResponse {
    pub fn new(
        status: u16,
        headers: HashMap<String, String>,
        body: String,
        content_length: u64,
        response_time: Duration,
        final_url: String,
    ) -> Self {
        Self {
            status,
            headers,
            body,
            content_length,
            response_time,
            final_url,
        }
    }

    /// Get header value by name (case-insensitive)
    pub fn get_header(&self, name: &str) -> Option<&String> {
        let name_lower = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == name_lower)
            .map(|(_, v)| v)
    }

    /// Get all header names and values as a single string for matching
    pub fn headers_string(&self) -> String {
        self.headers
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get the full response (headers + body) as a string
    pub fn full_response(&self) -> String {
        format!("{}\n\n{}", self.headers_string(), self.body)
    }

    /// Check if response indicates success
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Check if response is a redirect
    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    /// Check if response is a client error
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    /// Check if response is a server error
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    /// Get content type from headers
    pub fn content_type(&self) -> Option<&String> {
        self.get_header("content-type")
    }

    /// Check if response is HTML
    pub fn is_html(&self) -> bool {
        self.content_type()
            .map(|ct| ct.to_lowercase().contains("text/html"))
            .unwrap_or(false)
    }

    /// Check if response is JSON
    pub fn is_json(&self) -> bool {
        self.content_type()
            .map(|ct| ct.to_lowercase().contains("application/json"))
            .unwrap_or(false)
    }

    /// Check if response is XML
    pub fn is_xml(&self) -> bool {
        self.content_type()
            .map(|ct| {
                let ct_lower = ct.to_lowercase();
                ct_lower.contains("application/xml") || ct_lower.contains("text/xml")
            })
            .unwrap_or(false)
    }

    /// Get response body size in bytes
    pub fn body_size(&self) -> usize {
        self.body.len()
    }

    /// Get words count in response body
    pub fn word_count(&self) -> usize {
        self.body.split_whitespace().count()
    }

    /// Get lines count in response body
    pub fn line_count(&self) -> usize {
        self.body.lines().count()
    }
}
