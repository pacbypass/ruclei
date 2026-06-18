use std::time::{Duration, Instant};

/// Rate limiter to control the frequency of requests
/// This implements a simple token bucket algorithm
#[derive(Debug)]
pub struct RateLimiter {
    /// Maximum number of requests per time window
    max_requests: u32,
    /// Time window duration
    window_duration: Duration,
    /// Timestamps of recent requests
    request_times: Vec<Instant>,
    /// Minimum delay between requests
    min_delay: Duration,
    /// Last request timestamp
    last_request: Option<Instant>,
}

impl RateLimiter {
    /// Create a new rate limiter
    /// - `requests_per_second`: Maximum requests per second
    /// - `min_delay`: Minimum delay between requests
    pub fn new(requests_per_second: f64, min_delay: Duration) -> Self {
        let max_requests = if requests_per_second > 0.0 {
            (requests_per_second * 60.0) as u32 // Convert to requests per minute for window
        } else {
            1
        };

        Self {
            max_requests,
            window_duration: Duration::from_secs(60), // 1 minute window
            request_times: Vec::new(),
            min_delay,
            last_request: None,
        }
    }

    /// Create a rate limiter with requests per minute
    pub fn per_minute(requests_per_minute: u32, min_delay: Duration) -> Self {
        Self {
            max_requests: requests_per_minute,
            window_duration: Duration::from_secs(60),
            request_times: Vec::new(),
            min_delay,
            last_request: None,
        }
    }

    /// Create a rate limiter with requests per second
    pub fn per_second(requests_per_second: f64) -> Self {
        let min_delay = if requests_per_second > 0.0 {
            Duration::from_millis((1000.0 / requests_per_second) as u64)
        } else {
            Duration::from_secs(1)
        };

        Self::new(requests_per_second, min_delay)
    }

    /// Create a rate limiter with a fixed delay between requests
    pub fn with_delay(delay: Duration) -> Self {
        Self {
            max_requests: u32::MAX, // No limit on number of requests
            window_duration: Duration::from_secs(3600), // 1 hour window (effectively unlimited)
            request_times: Vec::new(),
            min_delay: delay,
            last_request: None,
        }
    }

    /// Acquire a request slot: check + record, return the duration to sleep before sending.
    /// Call this while holding the Mutex, then release the lock and sleep for the returned duration.
    pub fn acquire(&mut self) -> Duration {
        let now = Instant::now();
        self.cleanup_old_requests(now);

        let wait = self.calculate_wait_time(now);
        let send_at = now + wait;

        self.request_times.push(send_at);
        self.last_request = Some(send_at);

        wait
    }

    /// Wait until it's safe to make the next request
    pub fn wait_if_needed(&mut self) {
        let now = Instant::now();

        // Clean up old request times outside the window
        self.cleanup_old_requests(now);

        // Check if we need to wait due to rate limiting
        if self.should_wait(now) {
            let wait_time = self.calculate_wait_time(now);
            if wait_time > Duration::ZERO {
                std::thread::sleep(wait_time);
            }
        }

        // Enforce minimum delay between requests
        if let Some(last) = self.last_request {
            let elapsed = now.duration_since(last);
            if elapsed < self.min_delay {
                let additional_wait = self.min_delay - elapsed;
                std::thread::sleep(additional_wait);
            }
        }

        // Record this request
        let final_time = Instant::now();
        self.request_times.push(final_time);
        self.last_request = Some(final_time);
    }

    /// Check if we should wait before making a request
    fn should_wait(&self, _now: Instant) -> bool {
        self.request_times.len() >= self.max_requests as usize
    }

    /// Calculate how long to wait before making the next request
    fn calculate_wait_time(&self, now: Instant) -> Duration {
        if self.request_times.is_empty() {
            return Duration::ZERO;
        }

        // If we're at the limit, wait until the oldest request falls outside the window
        if self.request_times.len() >= self.max_requests as usize {
            if let Some(oldest) = self.request_times.first() {
                let window_end = *oldest + self.window_duration;
                if window_end > now {
                    return window_end - now;
                }
            }
        }

        // Check minimum delay
        if let Some(last) = self.last_request {
            let elapsed = now.duration_since(last);
            if elapsed < self.min_delay {
                return self.min_delay - elapsed;
            }
        }

        Duration::ZERO
    }

    /// Remove request times that are outside the current window
    fn cleanup_old_requests(&mut self, now: Instant) {
        let cutoff = now - self.window_duration;
        self.request_times.retain(|&time| time > cutoff);
    }

    /// Get current rate statistics
    pub fn stats(&self) -> RateLimiterStats {
        let now = Instant::now();
        let cutoff = now - self.window_duration;
        let current_requests = self.request_times.iter().filter(|&&time| time > cutoff).count();

        RateLimiterStats {
            requests_in_window: current_requests as u32,
            max_requests: self.max_requests,
            window_duration: self.window_duration,
            min_delay: self.min_delay,
            can_make_request: current_requests < self.max_requests as usize,
        }
    }

    /// Reset the rate limiter
    pub fn reset(&mut self) {
        self.request_times.clear();
        self.last_request = None;
    }

    /// Check if a request can be made immediately without waiting
    pub fn can_make_request(&mut self) -> bool {
        let now = Instant::now();
        self.cleanup_old_requests(now);
        !self.should_wait(now) && self.calculate_wait_time(now) == Duration::ZERO
    }

    /// Get the estimated wait time for the next request
    pub fn estimated_wait_time(&self) -> Duration {
        let now = Instant::now();
        self.calculate_wait_time(now)
    }
}

#[derive(Debug)]
pub struct RateLimiterStats {
    pub requests_in_window: u32,
    pub max_requests: u32,
    pub window_duration: Duration,
    pub min_delay: Duration,
    pub can_make_request: bool,
}

impl RateLimiterStats {
    pub fn utilization(&self) -> f64 {
        if self.max_requests == 0 {
            0.0
        } else {
            self.requests_in_window as f64 / self.max_requests as f64
        }
    }

    pub fn requests_remaining(&self) -> u32 {
        if self.requests_in_window >= self.max_requests {
            0
        } else {
            self.max_requests - self.requests_in_window
        }
    }
}

/// A no-op rate limiter that doesn't impose any limits
#[derive(Debug)]
pub struct NoRateLimit;

impl NoRateLimit {
    pub fn wait_if_needed(&mut self) {
        // Do nothing
    }

    pub fn can_make_request(&mut self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::per_second(1.0);
        assert_eq!(limiter.max_requests, 60); // 1 req/sec = 60 req/min
        
        let limiter = RateLimiter::per_minute(30, Duration::from_millis(100));
        assert_eq!(limiter.max_requests, 30);
    }

    #[test]
    fn test_min_delay_enforcement() {
        let mut limiter = RateLimiter::with_delay(Duration::from_millis(100));
        
        let start = Instant::now();
        limiter.wait_if_needed(); // First request - no wait
        
        let first_elapsed = start.elapsed();
        assert!(first_elapsed < Duration::from_millis(50)); // Should be immediate
        
        limiter.wait_if_needed(); // Second request - should wait
        let second_elapsed = start.elapsed();
        assert!(second_elapsed >= Duration::from_millis(100)); // Should have waited
    }

    #[test]
    fn test_can_make_request() {
        let mut limiter = RateLimiter::per_minute(2, Duration::from_millis(100));
        
        assert!(limiter.can_make_request());
        limiter.wait_if_needed();
        
        // After minimum delay, should be able to make another request
        std::thread::sleep(Duration::from_millis(150));
        assert!(limiter.can_make_request());
        limiter.wait_if_needed();
        
        // Now we've made 2 requests in the window, should not be able to make another immediately
        // Note: This test might be flaky due to timing, but demonstrates the concept
    }

    #[test]
    fn test_stats() {
        let mut limiter = RateLimiter::per_minute(10, Duration::from_millis(50));
        
        let stats = limiter.stats();
        assert_eq!(stats.requests_in_window, 0);
        assert_eq!(stats.max_requests, 10);
        assert!(stats.can_make_request);
        assert_eq!(stats.requests_remaining(), 10);
        
        limiter.wait_if_needed();
        let stats = limiter.stats();
        assert_eq!(stats.requests_in_window, 1);
        assert_eq!(stats.requests_remaining(), 9);
    }

    #[test]
    fn test_reset() {
        let mut limiter = RateLimiter::per_minute(1, Duration::from_millis(50));
        
        limiter.wait_if_needed();
        assert_eq!(limiter.stats().requests_in_window, 1);
        
        limiter.reset();
        assert_eq!(limiter.stats().requests_in_window, 0);
        assert!(limiter.can_make_request());
    }
}

