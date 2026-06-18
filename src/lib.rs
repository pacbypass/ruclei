pub mod cli;
pub mod template;
pub mod scanner;
pub mod matcher;
pub mod extractor;
pub mod cluster;
pub mod rate_limit;
pub mod output;
pub mod raw_request;

use anyhow::{Context, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use url::Url;

use cli::{Config, OutputFormat};
use cluster::RequestCluster;
use extractor::ExtractorEngine;
use matcher::MatcherEngine;
use rate_limit::RateLimiter;
use scanner::{HttpClient, ScanRequest, ScanResult};
use template::{Template, parser::TemplateParser};

// ─── Atomic scan statistics ───────────────────────────────────────────────────

struct AtomicStats {
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    templates_executed: AtomicU64,
    matches_found: AtomicU64,
}

impl AtomicStats {
    fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            templates_executed: AtomicU64::new(0),
            matches_found: AtomicU64::new(0),
        }
    }
}

/// Public scan statistics, collected at the end of a run.
#[derive(Debug, Default)]
pub struct ScanStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub templates_loaded: u64,
    pub templates_executed: u64,
    pub matches_found: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub duration: Duration,
}

impl ScanStats {
    pub fn success_rate(&self) -> f64 {
        if self.total_requests == 0 { 0.0 }
        else { self.successful_requests as f64 / self.total_requests as f64 * 100.0 }
    }
    pub fn cache_hit_rate(&self) -> f64 {
        let t = self.cache_hits + self.cache_misses;
        if t == 0 { 0.0 } else { self.cache_hits as f64 / t as f64 * 100.0 }
    }
    pub fn rps(&self) -> f64 {
        let s = self.duration.as_secs_f64();
        if s == 0.0 { 0.0 } else { self.total_requests as f64 / s }
    }
}

// ─── Scanner ─────────────────────────────────────────────────────────────────

pub struct RucleiScanner {
    config: Arc<Config>,
    http_client: Arc<HttpClient>,
    template_parser: TemplateParser,
    request_cluster: Arc<Mutex<RequestCluster>>,
    rate_limiter: Arc<Mutex<RateLimiter>>,
    templates: Vec<Template>,
    pub stats: ScanStats,
}

impl RucleiScanner {
    pub fn new(config: Config) -> Result<Self> {
        let http_client = HttpClient::new()
            .context("Failed to create HTTP client")?
            .with_timeout(config.timeout)
            .context("Failed to configure HTTP client timeout")?
            .with_user_agent(config.user_agent.clone());

        let rate_limiter = if config.rate_limit > 0.0 {
            RateLimiter::per_second(config.rate_limit)
        } else {
            RateLimiter::with_delay(config.delay)
        };

        // Configure rayon thread pool to match concurrency setting
        rayon::ThreadPoolBuilder::new()
            .num_threads(config.concurrency)
            .build_global()
            .ok(); // ok() because it can only be set once; ignore if already set

        Ok(Self {
            template_parser: {
                let mut p = TemplateParser::new();
                p.no_cache = config.no_cache;
                p
            },
            config: Arc::new(config),
            http_client: Arc::new(http_client),
            request_cluster: Arc::new(Mutex::new(RequestCluster::new())),
            rate_limiter: Arc::new(Mutex::new(rate_limiter)),
            templates: Vec::new(),
            stats: ScanStats::default(),
        })
    }

    pub fn clear_template_cache(&self) -> Result<()> {
        self.template_parser.clear_cache()
    }

    pub fn load_templates(&mut self) -> Result<()> {
        if !self.config.is_silent() {
            output::log_info(&format!(
                "Loading templates from: {}",
                self.config.templates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        let mut all_templates = Vec::new();

        for path in self.config.templates.iter() {
            if path.is_file() {
                match self.template_parser.parse_file(path) {
                    Ok(t) if self.config.should_include_template(&t) => all_templates.push(t),
                    Ok(_) => {}
                    Err(e) => {
                        if self.config.is_verbose() {
                            output::log_warn(&format!("Skipping {}: {:#}", path.display(), e));
                        }
                    }
                }
            } else if path.is_dir() {
                match self.template_parser.parse_directory(path) {
                    Ok(ts) => {
                        for t in ts {
                            if self.config.should_include_template(&t) {
                                all_templates.push(t);
                            }
                        }
                    }
                    Err(e) => output::log_warn(&e.to_string()),
                }
            } else {
                output::log_warn(&format!("Template path not found: {}", path.display()));
            }
        }

        self.templates = all_templates;
        self.stats.templates_loaded = self.templates.len() as u64;

        if !self.config.is_silent() {
            output::log_info(&format!(
                "Templates loaded for current scan: {}",
                self.templates.len()
            ));
            output::log_info(&format!(
                "Targets loaded for current scan: {}",
                self.config.targets.len()
            ));
        }

        if self.templates.is_empty() {
            return Err(anyhow::anyhow!("No valid templates loaded"));
        }
        Ok(())
    }

    pub fn run(&mut self) -> Result<Vec<ScanResult>> {
        let start = Instant::now();

        let atomic_stats = Arc::new(AtomicStats::new());
        let http_client = Arc::clone(&self.http_client);
        let cluster = Arc::clone(&self.request_cluster);
        let rate_limiter = Arc::clone(&self.rate_limiter);
        let config = Arc::clone(&self.config);
        let max_cache = config.max_cache_size;

        // Build all (target, template) pairs for parallel execution
        let work_items: Vec<(String, Template)> = self.config.targets
            .iter()
            .flat_map(|target| {
                self.templates.iter().map(move |t| (target.clone(), t.clone()))
            })
            .collect();

        let all_results: Vec<Vec<ScanResult>> = work_items
            .par_iter()
            .map(|(target, template)| {
                let stats = Arc::clone(&atomic_stats);
                let http = Arc::clone(&http_client);
                let cl = Arc::clone(&cluster);
                let rl = Arc::clone(&rate_limiter);
                let cfg = Arc::clone(&config);

                stats.templates_executed.fetch_add(1, Ordering::Relaxed);

                let ctx = ExecCtx {
                    http_client: &http,
                    cluster: &cl,
                    rate_limiter: &rl,
                    stats: &stats,
                    config: &cfg,
                    max_cache,
                };
                let results = execute_template(template, target, &ctx);

                match results {
                    Ok(rs) => {
                        for r in &rs {
                            if r.matched {
                                stats.matches_found.fetch_add(1, Ordering::Relaxed);
                                if !cfg.is_silent() {
                                    output::print_finding(
                                        &r.template_id,
                                        r.matcher_name.as_deref(),
                                        &template.info.severity,
                                        &r.request.url,
                                        &r.extracted_data,
                                    );
                                }
                            }
                        }
                        rs
                    }
                    Err(e) => {
                        if cfg.is_verbose() {
                            output::log_debug(&format!(
                                "Template {} failed: {}", template.id, e
                            ));
                        }
                        vec![]
                    }
                }
            })
            .collect();

        let elapsed = start.elapsed();

        // Collect stats
        let cluster_stats = self.request_cluster.lock().unwrap().stats().clone();
        self.stats.total_requests = atomic_stats.total_requests.load(Ordering::Relaxed);
        self.stats.successful_requests = atomic_stats.successful_requests.load(Ordering::Relaxed);
        self.stats.failed_requests = atomic_stats.failed_requests.load(Ordering::Relaxed);
        self.stats.templates_executed = atomic_stats.templates_executed.load(Ordering::Relaxed);
        self.stats.matches_found = atomic_stats.matches_found.load(Ordering::Relaxed);
        self.stats.cache_hits = cluster_stats.cache_hits;
        self.stats.cache_misses = cluster_stats.cache_misses;
        self.stats.duration = elapsed;

        if self.config.show_stats {
            self.print_stats();
        }

        Ok(all_results.into_iter().flatten().collect())
    }

    fn print_stats(&self) {
        let s = &self.stats;
        output::log_info(&format!("Templates executed:  {}", s.templates_executed));
        output::log_info(&format!("Total requests:      {}", s.total_requests));
        output::log_info(&format!("Successful:          {}", s.successful_requests));
        output::log_info(&format!("Failed:              {}", s.failed_requests));
        output::log_info(&format!("Matches found:       {}", s.matches_found));
        output::log_info(&format!("Cache hit rate:      {:.1}%", s.cache_hit_rate()));
        output::log_info(&format!("Duration:            {:.2}s", s.duration.as_secs_f64()));
        if s.duration.as_secs_f64() > 0.0 {
            output::log_info(&format!("Avg RPS:             {:.1}", s.rps()));
        }
    }

    pub fn write_results(&self, results: &[ScanResult]) -> Result<()> {
        if let Some(out_path) = &self.config.output {
            let matched: Vec<&ScanResult> = results.iter().filter(|r| r.matched).collect();
            let content = match self.config.output_format {
                OutputFormat::Json => format_json(matched, &self.templates)?,
                OutputFormat::Yaml => {
                    let j = format_json(matched, &self.templates)?;
                    let v: serde_json::Value = serde_json::from_str(&j)?;
                    serde_yaml::to_string(&v).context("YAML serialization failed")?
                }
                OutputFormat::Text => format_text(matched, &self.templates),
            };
            fs::write(out_path, content)
                .with_context(|| format!("Failed to write to {}", out_path.display()))?;
            if self.config.is_verbose() {
                output::log_info(&format!("Results written to: {}", out_path.display()));
            }
        }
        Ok(())
    }
}

// ─── Parallel template execution (free function so rayon can call it) ─────────

struct ExecCtx<'a> {
    http_client: &'a Arc<HttpClient>,
    cluster: &'a Arc<Mutex<RequestCluster>>,
    rate_limiter: &'a Arc<Mutex<RateLimiter>>,
    stats: &'a Arc<AtomicStats>,
    config: &'a Arc<Config>,
    max_cache: usize,
}

fn execute_template(
    template: &Template,
    target: &str,
    ctx: &ExecCtx<'_>,
) -> Result<Vec<ScanResult>> {
    let ExecCtx { http_client, cluster, rate_limiter, stats, config, max_cache } = ctx;
    let mut matcher_engine = MatcherEngine::new();
    let mut extractor_engine = ExtractorEngine::new();

    let base_url = build_base_url(target);
    let vars = raw_request::build_vars(&base_url);
    let mut results = Vec::new();

    for request in template.get_http_requests() {
        let matchers_condition = request.matchers_condition.as_deref().unwrap_or("or").to_string();

        // Build list of (url, scan_request) from paths or raw requests
        let scan_requests: Vec<ScanRequest> = if !request.path.is_empty() {
            request.path.iter()
                .filter_map(|p| resolve_path(p, &base_url, &vars).ok())
                .map(|url| build_scan_request(&url, request, config))
                .filter_map(|r| r.ok())
                .collect()
        } else if !request.raw_request.is_empty() {
            request.raw_request.iter()
                .filter_map(|raw| {
                    raw_request::parse_raw_request(raw, &base_url, &vars).ok()
                        .map(|req| apply_config_headers(req, request, config))
                })
                .collect()
        } else {
            vec![build_scan_request(&base_url, request, config)?]
        };

        for scan_req in scan_requests {
            // Rate limit: acquire outside the HTTP call, sleep outside the lock
            let wait = {
                let mut rl = rate_limiter.lock().unwrap();
                rl.acquire()
            };
            if !wait.is_zero() {
                std::thread::sleep(wait);
            }

            // Try cache first, then execute
            let response = {
                let cached = {
                    let mut c = cluster.lock().unwrap();
                    c.get_cached_response(&scan_req)
                };

                if let Some(resp) = cached {
                    resp
                } else {
                    // Execute outside the cluster lock
                    stats.total_requests.fetch_add(1, Ordering::Relaxed);
                    match http_client.execute_with_retries(&scan_req, config.max_retries) {
                        Ok(r) => {
                            stats.successful_requests.fetch_add(1, Ordering::Relaxed);
                            // Store in cache
                            {
                                let mut c = cluster.lock().unwrap();
                                c.cache_response(&scan_req, r.clone());
                                // Evict if too large
                                if c.cache_size() > *max_cache {
                                    c.cleanup(max_cache / 2);
                                }
                            }
                            r
                        }
                        Err(e) => {
                            stats.failed_requests.fetch_add(1, Ordering::Relaxed);
                            if config.is_verbose() {
                                output::log_debug(&format!(
                                    "Request failed [{}]: {}", scan_req.url, e
                                ));
                            }
                            continue;
                        }
                    }
                }
            };

            let mut scan_result = ScanResult::new(scan_req, response, template.id.clone());

            // Evaluate matchers
            if let Some(matchers) = &request.matchers {
                match matcher_engine.evaluate_matchers(matchers, &matchers_condition, &scan_result.response) {
                    Ok(mr) if mr.matched => {
                        scan_result = scan_result.with_match(mr.matcher_name);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        if config.is_verbose() {
                            output::log_debug(&format!("Matcher error [{}]: {}", template.id, e));
                        }
                    }
                }
            }

            // Run extractors
            let run_extractors = scan_result.matched || request.matchers.is_none();
            if run_extractors {
                if let Some(extractors) = &request.extractors {
                    match extractor_engine.extract_all(extractors, &scan_result.response) {
                        Ok(data) => { scan_result = scan_result.with_extracted_data(data); }
                        Err(e) => {
                            if config.is_verbose() {
                                output::log_debug(&format!("Extractor error [{}]: {}", template.id, e));
                            }
                        }
                    }
                }
            }

            results.push(scan_result);
        }
    }

    Ok(results)
}

// ─── URL helpers ──────────────────────────────────────────────────────────────

fn build_base_url(target: &str) -> String {
    if target.starts_with("http://") || target.starts_with("https://") {
        target.trim_end_matches('/').to_string()
    } else {
        format!("https://{}", target.trim_end_matches('/'))
    }
}

fn resolve_path(path: &str, base_url: &str, vars: &HashMap<String, String>) -> Result<String> {
    // Substitute template variables
    let resolved = raw_request::build_vars(base_url)
        .iter()
        .chain(vars.iter())
        .fold(path.to_string(), |acc, (k, v)| {
            acc.replace(&format!("{{{{{}}}}}", k), v)
        });

    if resolved.starts_with("http://") || resolved.starts_with("https://") {
        return Ok(resolved);
    }

    let base = base_url.trim_end_matches('/');
    let p = resolved.trim_start_matches('/');
    if p.is_empty() {
        Ok(base.to_string())
    } else {
        Ok(format!("{}/{}", base, p))
    }
}

fn build_scan_request(url: &str, request: &template::Request, config: &Config) -> Result<ScanRequest> {
    let method = request.method.as_deref().unwrap_or("GET").to_string();
    let mut headers: HashMap<String, String> = HashMap::new();
    for (k, v) in &config.headers {
        headers.insert(k.clone(), v.clone());
    }
    if let Some(th) = &request.headers {
        headers.extend(th.clone());
    }

    let req = ScanRequest::new(url.to_string())
        .with_method(method)
        .with_headers(headers)
        .with_redirects(
            request.redirects.unwrap_or(true),
            request.max_redirects.unwrap_or(config.max_redirects),
        );

    if let Some(body) = &request.body {
        Ok(req.with_body(body.clone()))
    } else {
        Ok(req)
    }
}

/// Apply config-level headers to a raw-parsed request (without overwriting raw headers)
fn apply_config_headers(mut req: ScanRequest, request: &template::Request, config: &Config) -> ScanRequest {
    for (k, v) in &config.headers {
        req.headers.entry(k.clone()).or_insert_with(|| v.clone());
    }
    req = req.with_redirects(
        request.redirects.unwrap_or(true),
        request.max_redirects.unwrap_or(config.max_redirects),
    );
    req
}

// ─── Output formatters ────────────────────────────────────────────────────────

fn format_json(results: Vec<&ScanResult>, templates: &[Template]) -> Result<String> {
    let items: Vec<serde_json::Value> = results.iter()
        .map(|r| {
            let t = templates.iter().find(|t| t.id == r.template_id);
            serde_json::json!({
                "template_id":    r.template_id,
                "template_name":  t.map(|t| t.info.name.as_str()),
                "severity":       t.map(|t| t.info.severity.as_str()),
                "url":            r.request.url,
                "method":         r.request.method,
                "status_code":    r.response.status,
                "content_length": r.response.content_length,
                "response_time":  r.response.response_time.as_millis(),
                "matcher_name":   r.matcher_name,
                "extracted_data": r.extracted_data,
            })
        })
        .collect();
    serde_json::to_string_pretty(&items).context("JSON serialization failed")
}

fn format_text(results: Vec<&ScanResult>, templates: &[Template]) -> String {
    let mut out = String::new();
    for r in results {
        let t = templates.iter().find(|t| t.id == r.template_id);
        out.push_str(&format!(
            "[{}] [{}] {} [{}]\n",
            t.map(|t| t.info.severity.to_uppercase()).unwrap_or_default(),
            r.template_id,
            t.map(|t| t.info.name.as_str()).unwrap_or(""),
            r.request.url,
        ));
        for (k, vs) in &r.extracted_data {
            out.push_str(&format!("  {}: {}\n", k, vs.join(", ")));
        }
        out.push('\n');
    }
    out
}

// ─── Template variable substitution ──────────────────────────────────────────

pub struct TemplateVars {
    pub base_url: String,
    pub scheme: String,
    pub host: String,
    pub hostname: String,
    pub port: String,
    pub path: String,
    pub root_url: String,
}

impl TemplateVars {
    pub fn from_url(url: &str) -> Result<Self> {
        let parsed = Url::parse(url)
            .with_context(|| format!("Invalid target URL: {}", url))?;

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

        Ok(Self {
            base_url: url.to_string(),
            scheme,
            host,
            hostname,
            port,
            path,
            root_url,
        })
    }

    pub fn substitute(&self, template: &str) -> String {
        template
            .replace("{{BaseURL}}", &self.base_url)
            .replace("{{RootURL}}", &self.root_url)
            .replace("{{Hostname}}", &self.hostname)
            .replace("{{Host}}", &self.host)
            .replace("{{Port}}", &self.port)
            .replace("{{Path}}", &self.path)
            .replace("{{Scheme}}", &self.scheme)
            .replace("{{scheme}}", &self.scheme)
            .replace("{{hostname}}", &self.hostname)
            .replace("{{host}}", &self.host)
    }
}
