use clap::{Arg, ArgAction, Command};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    /// Target URLs to scan
    pub targets: Vec<String>,
    /// Template files or directories
    pub templates: Vec<PathBuf>,
    /// Output file for results
    pub output: Option<PathBuf>,
    /// Output format (json, yaml, text)
    pub output_format: OutputFormat,
    /// Rate limiting: requests per second
    pub rate_limit: f64,
    /// Minimum delay between requests
    pub delay: Duration,
    /// HTTP timeout
    pub timeout: Duration,
    /// Maximum redirects to follow
    pub max_redirects: u32,
    /// User agent string
    pub user_agent: String,
    /// Custom headers
    pub headers: Vec<(String, String)>,
    /// Proxy URL
    pub proxy: Option<String>,
    /// Verbose output
    pub verbose: bool,
    /// Silent mode (only show matches)
    pub silent: bool,
    /// Maximum number of cached requests
    pub max_cache_size: usize,
    /// Filter templates by severity
    pub severity_filter: Option<Vec<String>>,
    /// Filter templates by tags
    pub tag_filter: Option<Vec<String>>,
    /// Include templates by ID
    pub include_templates: Option<Vec<String>>,
    /// Exclude templates by ID
    pub exclude_templates: Option<Vec<String>>,
    /// Maximum number of retries for failed requests
    pub max_retries: u32,
    /// Show statistics
    pub show_stats: bool,
    /// Number of concurrent template executions
    pub concurrency: usize,
    /// No banner (silent startup)
    pub no_banner: bool,
    /// Skip disk template cache
    pub no_cache: bool,
    /// Clear disk template cache and exit
    pub clear_cache: bool,
    /// Load templates and print count, then exit without scanning
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Text,
    Json,
    Yaml,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            targets: Vec::new(),
            templates: Vec::new(),
            output: None,
            output_format: OutputFormat::Text,
            rate_limit: 10.0, // 10 requests per second
            delay: Duration::from_millis(100),
            timeout: Duration::from_secs(30),
            max_redirects: 3,
            user_agent: "ruclei/1.0".to_string(),
            headers: Vec::new(),
            proxy: None,
            verbose: false,
            silent: false,
            max_cache_size: 1000,
            severity_filter: None,
            tag_filter: None,
            include_templates: None,
            exclude_templates: None,
            max_retries: 3,
            show_stats: false,
            concurrency: 25,
            no_banner: false,
            no_cache: false,
            clear_cache: false,
            dry_run: false,
        }
    }
}

pub fn build_cli() -> Command {
    Command::new("ruclei")
        .version("1.0.0")
        .author("Rust Nuclei Clone")
        .about("Fast vulnerability scanner powered by templates")
        .arg(
            Arg::new("target")
                .short('u')
                .long("target")
                .value_name("URL")
                .help("Target URL to scan")
                .action(ArgAction::Append)
        )
        .arg(
            Arg::new("list")
                .short('l')
                .long("list")
                .value_name("FILE")
                .help("File containing list of target URLs")
        )
        .arg(
            Arg::new("templates")
                .short('t')
                .long("templates")
                .value_name("PATH")
                .help("Template file or directory")
                .action(ArgAction::Append)
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Output file to write results")
        )
        .arg(
            Arg::new("format")
                .short('f')
                .long("format")
                .value_name("FORMAT")
                .help("Output format")
                .value_parser(["text", "json", "yaml"])
                .default_value("text")
        )
        .arg(
            Arg::new("rate-limit")
                .short('r')
                .long("rate-limit")
                .value_name("RPS")
                .help("Rate limit in requests per second")
                .value_parser(clap::value_parser!(f64))
                .default_value("10.0")
        )
        .arg(
            Arg::new("delay")
                .short('d')
                .long("delay")
                .value_name("MS")
                .help("Delay between requests in milliseconds")
                .value_parser(clap::value_parser!(u64))
        )
        .arg(
            Arg::new("timeout")
                .long("timeout")
                .value_name("SECONDS")
                .help("HTTP timeout in seconds")
                .value_parser(clap::value_parser!(u64))
                .default_value("30")
        )
        .arg(
            Arg::new("max-redirects")
                .long("max-redirects")
                .value_name("NUM")
                .help("Maximum redirects to follow")
                .value_parser(clap::value_parser!(u32))
                .default_value("3")
        )
        .arg(
            Arg::new("user-agent")
                .long("user-agent")
                .value_name("STRING")
                .help("User agent string")
                .default_value("ruclei/1.0")
        )
        .arg(
            Arg::new("header")
                .short('H')
                .long("header")
                .value_name("HEADER")
                .help("Custom header (format: 'Name: Value')")
                .action(ArgAction::Append)
        )
        .arg(
            Arg::new("proxy")
                .long("proxy")
                .value_name("URL")
                .help("Proxy URL")
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Verbose output")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("silent")
                .short('s')
                .long("silent")
                .help("Silent mode (only show matches)")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("max-cache-size")
                .long("max-cache-size")
                .value_name("NUM")
                .help("Maximum number of cached requests")
                .value_parser(clap::value_parser!(usize))
                .default_value("1000")
        )
        .arg(
            Arg::new("severity")
                .long("severity")
                .value_name("LEVEL")
                .help("Filter by severity (info,low,medium,high,critical)")
                .value_delimiter(',')
                .action(ArgAction::Append)
        )
        .arg(
            Arg::new("tags")
                .long("tags")
                .value_name("TAG")
                .help("Filter by tags")
                .value_delimiter(',')
                .action(ArgAction::Append)
        )
        .arg(
            Arg::new("include-templates")
                .long("include-templates")
                .value_name("ID")
                .help("Include specific template IDs")
                .value_delimiter(',')
                .action(ArgAction::Append)
        )
        .arg(
            Arg::new("exclude-templates")
                .long("exclude-templates")
                .value_name("ID")
                .help("Exclude specific template IDs")
                .value_delimiter(',')
                .action(ArgAction::Append)
        )
        .arg(
            Arg::new("max-retries")
                .long("max-retries")
                .value_name("NUM")
                .help("Maximum retries for failed requests")
                .value_parser(clap::value_parser!(u32))
                .default_value("3")
        )
        .arg(
            Arg::new("stats")
                .long("stats")
                .help("Show scan statistics")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("concurrency")
                .short('c')
                .long("concurrency")
                .value_name("NUM")
                .help("Maximum number of concurrent templates [default: 25]")
                .value_parser(clap::value_parser!(usize))
                .default_value("25")
        )
        .arg(
            Arg::new("no-color")
                .long("no-color")
                .help("Disable colored output")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("no-cache")
                .long("no-cache")
                .help("Disable template disk cache (always parse from YAML)")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("clear-cache")
                .long("clear-cache")
                .help("Clear the template disk cache and exit")
                .action(ArgAction::SetTrue)
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .help("Load templates and print count, then exit without scanning")
                .action(ArgAction::SetTrue)
        )
}

pub fn parse_args() -> anyhow::Result<Config> {
    let matches = build_cli().get_matches();
    let mut config = Config::default();

    // Parse targets
    if let Some(targets) = matches.get_many::<String>("target") {
        config.targets.extend(targets.cloned());
    }

    // Parse target list file
    if let Some(list_file) = matches.get_one::<String>("list") {
        let content = std::fs::read_to_string(list_file)?;
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                config.targets.push(line.to_string());
            }
        }
    }

    // Validate that we have targets (skip check for maintenance-only flags)
    let clear_cache = matches.get_flag("clear-cache");
    let dry_run = matches.get_flag("dry-run");
    if config.targets.is_empty() && !clear_cache && !dry_run {
        return Err(anyhow::anyhow!("No targets specified. Use -u/--target or -l/--list"));
    }

    // Parse templates
    if let Some(templates) = matches.get_many::<String>("templates") {
        config.templates.extend(templates.map(PathBuf::from));
    }

    // If no templates specified, try default locations
    if config.templates.is_empty() {
        let default_paths = [
            "templates/",
            "nuclei-templates/",
            "/usr/share/nuclei-templates/",
            "/opt/nuclei-templates/",
        ];

        for path in &default_paths {
            let path_buf = PathBuf::from(path);
            if path_buf.exists() && path_buf.is_dir() {
                config.templates.push(path_buf);
                break;
            }
        }

        if config.templates.is_empty() {
            return Err(anyhow::anyhow!(
                "No templates found. Specify template path with -t/--templates"
            ));
        }
    }

    // Parse output
    if let Some(output) = matches.get_one::<String>("output") {
        config.output = Some(PathBuf::from(output));
    }

    // Parse output format
    config.output_format = match matches.get_one::<String>("format").unwrap().as_str() {
        "json" => OutputFormat::Json,
        "yaml" => OutputFormat::Yaml,
        "text" => OutputFormat::Text,
        _ => OutputFormat::Text,
    };

    // Parse rate limiting
    config.rate_limit = *matches.get_one::<f64>("rate-limit").unwrap();

    if let Some(delay_ms) = matches.get_one::<u64>("delay") {
        config.delay = Duration::from_millis(*delay_ms);
    } else {
        // Calculate delay from rate limit
        if config.rate_limit > 0.0 {
            config.delay = Duration::from_millis((1000.0 / config.rate_limit) as u64);
        }
    }

    // Parse timeout
    config.timeout = Duration::from_secs(*matches.get_one::<u64>("timeout").unwrap());

    // Parse max redirects
    config.max_redirects = *matches.get_one::<u32>("max-redirects").unwrap();

    // Parse user agent
    config.user_agent = matches.get_one::<String>("user-agent").unwrap().clone();

    // Parse custom headers
    if let Some(headers) = matches.get_many::<String>("header") {
        for header in headers {
            if let Some(colon_pos) = header.find(':') {
                let name = header[..colon_pos].trim().to_string();
                let value = header[colon_pos + 1..].trim().to_string();
                config.headers.push((name, value));
            }
        }
    }

    // Parse proxy
    if let Some(proxy) = matches.get_one::<String>("proxy") {
        config.proxy = Some(proxy.clone());
    }

    // Parse flags
    config.verbose = matches.get_flag("verbose");
    config.silent = matches.get_flag("silent");
    config.show_stats = matches.get_flag("stats");

    // Parse cache size
    config.max_cache_size = *matches.get_one::<usize>("max-cache-size").unwrap();

    // Parse severity filter
    if let Some(severities) = matches.get_many::<String>("severity") {
        config.severity_filter = Some(severities.cloned().collect());
    }

    // Parse tag filter
    if let Some(tags) = matches.get_many::<String>("tags") {
        config.tag_filter = Some(tags.cloned().collect());
    }

    // Parse include templates
    if let Some(includes) = matches.get_many::<String>("include-templates") {
        config.include_templates = Some(includes.cloned().collect());
    }

    // Parse exclude templates
    if let Some(excludes) = matches.get_many::<String>("exclude-templates") {
        config.exclude_templates = Some(excludes.cloned().collect());
    }

    // Parse max retries
    config.max_retries = *matches.get_one::<u32>("max-retries").unwrap();

    // Parse concurrency
    config.concurrency = *matches.get_one::<usize>("concurrency").unwrap();

    // No-color flag
    if matches.get_flag("no-color") {
        colored::control::set_override(false);
    }

    config.no_cache = matches.get_flag("no-cache");
    config.clear_cache = matches.get_flag("clear-cache");
    config.dry_run = matches.get_flag("dry-run");

    Ok(config)
}

impl Config {
    /// Check if a template should be included based on filters
    pub fn should_include_template(&self, template: &crate::template::Template) -> bool {
        if let Some(sev_filter) = &self.severity_filter {
            if !sev_filter.iter().any(|s| s.eq_ignore_ascii_case(&template.info.severity)) {
                return false;
            }
        }

        if let Some(tag_filter) = &self.tag_filter {
            let template_tags = template.tags();
            if template_tags.is_empty() {
                return false;
            }
            let has_match = tag_filter.iter().any(|ft| {
                template_tags.iter().any(|tt| tt.to_lowercase().contains(&ft.to_lowercase()))
            });
            if !has_match {
                return false;
            }
        }

        if let Some(includes) = &self.include_templates {
            if !includes.contains(&template.id) {
                return false;
            }
        }

        if let Some(excludes) = &self.exclude_templates {
            if excludes.contains(&template.id) {
                return false;
            }
        }

        true
    }

    /// Get delay duration between requests
    pub fn get_delay(&self) -> Duration {
        self.delay
    }

    /// Check if verbose output is enabled
    pub fn is_verbose(&self) -> bool {
        self.verbose && !self.silent
    }

    /// Check if silent mode is enabled
    pub fn is_silent(&self) -> bool {
        self.silent
    }
}

