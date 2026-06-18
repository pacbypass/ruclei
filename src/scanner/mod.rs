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

    /// Generate a cache key for request deduplication
    pub fn cluster_key(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.url.hash(&mut hasher);
        self.method.hash(&mut hasher);
        let mut sorted: Vec<_> = self.headers.iter().collect();
        sorted.sort_by_key(|(k, _)| k.as_str());
        for (k, v) in sorted {
            k.hash(&mut hasher);
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
