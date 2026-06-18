use crate::scanner::{HttpResponse, ScanRequest};
use std::collections::HashMap;

/// Request clustering system to avoid duplicate HTTP requests
/// This maintains a cache of requests and their responses
#[derive(Debug)]
pub struct RequestCluster {
    /// Cache mapping request cluster keys to their responses
    cache: HashMap<String, HttpResponse>,
    /// Statistics about cache usage
    stats: ClusterStats,
}

#[derive(Debug, Default, Clone)]
pub struct ClusterStats {
    pub total_requests: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub unique_requests: u64,
}

impl ClusterStats {
    pub fn hit_rate(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.cache_hits as f64 / self.total_requests as f64
        }
    }

    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate()
    }
}

impl RequestCluster {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            stats: ClusterStats::default(),
        }
    }

    /// Check if a request is already cached
    pub fn is_cached(&self, request: &ScanRequest) -> bool {
        let key = request.cluster_key();
        self.cache.contains_key(&key)
    }

    /// Get cached response for a request if it exists
    pub fn get_cached_response(&mut self, request: &ScanRequest) -> Option<HttpResponse> {
        let key = request.cluster_key();
        self.stats.total_requests += 1;

        if let Some(response) = self.cache.get(&key) {
            self.stats.cache_hits += 1;
            Some(response.clone())
        } else {
            self.stats.cache_misses += 1;
            None
        }
    }

    /// Store a response in the cache
    pub fn cache_response(&mut self, request: &ScanRequest, response: HttpResponse) {
        let key = request.cluster_key();

        if !self.cache.contains_key(&key) {
            self.stats.unique_requests += 1;
        }

        self.cache.insert(key, response);
    }

    /// Get or execute a request, using cache when possible
    pub fn get_or_execute<F>(
        &mut self,
        request: &ScanRequest,
        executor: F,
    ) -> anyhow::Result<HttpResponse>
    where
        F: FnOnce(&ScanRequest) -> anyhow::Result<HttpResponse>,
    {
        if let Some(cached_response) = self.get_cached_response(request) {
            return Ok(cached_response);
        }

        // Execute the request
        let response = executor(request)?;

        // Cache the response
        self.cache_response(request, response.clone());

        Ok(response)
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.stats = ClusterStats::default();
    }

    /// Get cache statistics
    pub fn stats(&self) -> &ClusterStats {
        &self.stats
    }

    /// Get number of cached requests
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }

    /// Remove old entries to prevent memory issues (simple LRU-like behavior)
    /// This is a simplified approach - in a real implementation you might want
    /// to use a proper LRU cache with timestamps
    pub fn cleanup(&mut self, max_entries: usize) {
        if self.cache.len() > max_entries {
            let keys_to_remove: Vec<String> = self
                .cache
                .keys()
                .take(self.cache.len() - max_entries)
                .cloned()
                .collect();

            for key in keys_to_remove {
                self.cache.remove(&key);
            }
        }
    }

    /// Get all unique request patterns (for debugging/analysis)
    pub fn get_request_patterns(&self) -> Vec<String> {
        self.cache.keys().cloned().collect()
    }

    /// Check if two requests would be clustered together
    pub fn would_cluster(req1: &ScanRequest, req2: &ScanRequest) -> bool {
        req1.cluster_key() == req2.cluster_key()
    }
}

impl Default for RequestCluster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::HttpResponse;
    use std::collections::HashMap;
    use std::time::Duration;

    fn create_test_response() -> HttpResponse {
        HttpResponse::new(
            200,
            HashMap::new(),
            "test body".to_string(),
            9,
            Duration::from_millis(100),
            "https://example.com".to_string(),
        )
    }

    #[test]
    fn test_request_clustering() {
        let mut cluster = RequestCluster::new();
        let request = ScanRequest::new("https://example.com".to_string());
        let response = create_test_response();

        // Initially not cached
        assert!(!cluster.is_cached(&request));
        assert!(cluster.get_cached_response(&request).is_none());

        // Cache the response
        cluster.cache_response(&request, response.clone());

        // Now should be cached
        assert!(cluster.is_cached(&request));
        let cached = cluster.get_cached_response(&request).unwrap();
        assert_eq!(cached.status, response.status);
        assert_eq!(cached.body, response.body);
    }

    #[test]
    fn test_cluster_stats() {
        let mut cluster = RequestCluster::new();
        let request = ScanRequest::new("https://example.com".to_string());
        let response = create_test_response();

        // Miss
        assert!(cluster.get_cached_response(&request).is_none());
        assert_eq!(cluster.stats().cache_misses, 1);
        assert_eq!(cluster.stats().cache_hits, 0);

        // Cache and hit
        cluster.cache_response(&request, response);
        let _cached = cluster.get_cached_response(&request);
        assert_eq!(cluster.stats().cache_hits, 1);
        assert_eq!(cluster.stats().cache_misses, 1);

        // Hit rate should be 50%
        assert_eq!(cluster.stats().hit_rate(), 0.5);
    }

    #[test]
    fn test_get_or_execute() {
        let mut cluster = RequestCluster::new();
        let request = ScanRequest::new("https://example.com".to_string());
        let expected_response = create_test_response();

        let mut call_count = 0;
        let executor = |_req: &ScanRequest| -> anyhow::Result<HttpResponse> {
            call_count += 1;
            Ok(expected_response.clone())
        };

        // First call should execute
        let response1 = cluster.get_or_execute(&request, executor).unwrap();
        assert_eq!(call_count, 1);
        assert_eq!(response1.status, expected_response.status);

        // Second call should use cache
        let response2 = cluster
            .get_or_execute(&request, |_| {
                call_count += 1;
                Ok(create_test_response())
            })
            .unwrap();
        assert_eq!(call_count, 1); // Should not increment
        assert_eq!(response2.status, expected_response.status);
    }

    #[test]
    fn test_would_cluster() {
        let req1 = ScanRequest::new("https://example.com".to_string());
        let req2 = ScanRequest::new("https://example.com".to_string());
        let req3 = ScanRequest::new("https://different.com".to_string());

        assert!(RequestCluster::would_cluster(&req1, &req2));
        assert!(!RequestCluster::would_cluster(&req1, &req3));
    }
}
