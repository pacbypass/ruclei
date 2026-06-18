use super::{HttpResponse, ScanRequest};
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct HttpClient {
    client_follow: Client,
    client_no_follow: Client,
    user_agent: String,
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        let ua = "Nuclei - Open-source project (github.com/projectdiscovery/nuclei)";
        let client_follow = Client::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(ua)
            .build()
            .context("Failed to create redirect-following HTTP client")?;

        let client_no_follow = Client::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(ua)
            .build()
            .context("Failed to create no-redirect HTTP client")?;

        Ok(Self {
            client_follow,
            client_no_follow,
            user_agent: ua.to_string(),
        })
    }

    pub fn with_timeout(self, timeout: Duration) -> Result<Self> {
        let ua = self.user_agent.clone();
        let client_follow = Client::builder()
            .timeout(timeout)
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::limited(10))
            .user_agent(&ua)
            .build()
            .context("Failed to create redirect-following HTTP client with timeout")?;

        let client_no_follow = Client::builder()
            .timeout(timeout)
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(&ua)
            .build()
            .context("Failed to create no-redirect HTTP client with timeout")?;

        Ok(Self { client_follow, client_no_follow, user_agent: ua })
    }

    pub fn with_user_agent(mut self, ua: String) -> Self {
        self.user_agent = ua;
        self
    }

    pub fn execute(&self, request: &ScanRequest) -> Result<HttpResponse> {
        let start = Instant::now();

        let client = if request.follow_redirects {
            &self.client_follow
        } else {
            &self.client_no_follow
        };

        let method = reqwest::Method::from_bytes(request.method.to_uppercase().as_bytes())
            .with_context(|| format!("Invalid HTTP method: {}", request.method))?;

        let mut req_builder = client.request(method, &request.url);

        for (key, value) in &request.headers {
            req_builder = req_builder.header(key.as_str(), value.as_str());
        }

        if let Some(body) = &request.body {
            req_builder = req_builder.body(body.clone());
        }

        let response = req_builder.send().context("Failed to send HTTP request")?;
        let elapsed = start.elapsed();

        let status = response.status().as_u16();
        let final_url = response.url().to_string();

        // Collect headers (lowercase keys, keep first value per name)
        let mut headers: HashMap<String, String> = HashMap::new();
        for (k, v) in response.headers() {
            let key = k.as_str().to_lowercase();
            if !headers.contains_key(&key) {
                if let Ok(val) = v.to_str() {
                    headers.insert(key, val.to_string());
                }
            }
        }

        let body_bytes = response.bytes().context("Failed to read response body")?;
        let body = String::from_utf8_lossy(&body_bytes).into_owned();
        let content_length = body.len() as u64;

        Ok(HttpResponse::new(status, headers, body, content_length, elapsed, final_url))
    }

    pub fn execute_with_retries(&self, request: &ScanRequest, max_retries: u32) -> Result<HttpResponse> {
        let mut last_err = None;
        for attempt in 0..=max_retries {
            match self.execute(request) {
                Ok(r) => return Ok(r),
                Err(e) => {
                    last_err = Some(e);
                    if attempt < max_retries {
                        let delay = Duration::from_millis(100 * 2_u64.pow(attempt));
                        std::thread::sleep(delay);
                    }
                }
            }
        }
        Err(last_err.unwrap())
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default HTTP client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_client_creation() {
        assert!(HttpClient::new().is_ok());
    }
}
