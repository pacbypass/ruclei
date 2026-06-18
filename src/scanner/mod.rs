use std::collections::HashMap;

pub mod http_client;
pub mod response;

pub use http_client::HttpClient;
pub use response::HttpResponse;

#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub follow_redirects: bool,
    pub max_redirects: u32,
}

impl ScanRequest {
    pub fn new(url: String) -> Self {
        Self {
            url,
            method: "GET".to_string(),
            headers: HashMap::new(),
            body: None,
            follow_redirects: true,
            max_redirects: 3,
        }
    }

    pub fn with_method(mut self, method: String) -> Self {
        self.method = method;
        self
    }

    pub fn with_headers(mut self, headers: HashMap<String, String>) -> Self {
        self.headers = headers;
        self
    }

    pub fn with_body(mut self, body: String) -> Self {
        self.body = Some(body);
        self
    }

    pub fn with_redirects(mut self, follow: bool, max: u32) -> Self {
        self.follow_redirects = follow;
        self.max_redirects = max;
        self
    }

    /// Canonical URL for deduplication: strip trailing slash, lowercase scheme+host.
    fn canonical_url(&self) -> String {
        // Parse to normalize scheme+host casing and trailing slashes on bare origin
        if let Ok(mut u) = url::Url::parse(&self.url) {
            // Strip trailing slash on bare-origin paths
            let path = u.path().trim_end_matches('/').to_string();
            let canonical_path = if path.is_empty() {
                "/".to_string()
            } else {
                path
            };
            u.set_path(&canonical_path);
            u.to_string()
        } else {
            // Fallback: strip trailing slash manually
            self.url.trim_end_matches('/').to_string()
        }
    }

    /// Generate a cache key for request deduplication.
    /// Uses a canonical URL so that trailing-slash variants and bare origins
    /// (e.g. https://host vs https://host/) share the same cache entry.
    pub fn cluster_key(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.canonical_url().hash(&mut hasher);
        self.method.to_uppercase().hash(&mut hasher);
        // Exclude per-template headers that don't affect response content
        // (e.g. User-Agent, Accept-Language vary per config but return same page)
        let skip: &[&str] = &["user-agent", "accept-language", "accept-encoding"];
        let mut sorted: Vec<_> = self
            .headers
            .iter()
            .filter(|(k, _)| !skip.contains(&k.to_lowercase().as_str()))
            .collect();
        sorted.sort_by_key(|(k, _)| k.as_str());
        for (k, v) in sorted {
            k.to_lowercase().hash(&mut hasher);
            v.hash(&mut hasher);
        }
        if let Some(b) = &self.body {
            b.hash(&mut hasher);
        }
        format!("{:x}", hasher.finish())
    }
}

#[derive(Debug)]
pub struct ScanResult {
    pub request: ScanRequest,
    pub response: HttpResponse,
    pub matched: bool,
    pub extracted_data: HashMap<String, Vec<String>>,
    pub template_id: String,
    pub matcher_name: Option<String>,
}

impl ScanResult {
    pub fn new(request: ScanRequest, response: HttpResponse, template_id: String) -> Self {
        Self {
            request,
            response,
            matched: false,
            extracted_data: HashMap::new(),
            template_id,
            matcher_name: None,
        }
    }

    pub fn with_match(mut self, matcher_name: Option<String>) -> Self {
        self.matched = true;
        self.matcher_name = matcher_name;
        self
    }

    pub fn with_extracted_data(mut self, data: HashMap<String, Vec<String>>) -> Self {
        self.extracted_data = data;
        self
    }
}
