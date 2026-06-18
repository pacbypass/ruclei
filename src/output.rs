use chrono::Local;
use colored::*;
use std::io::{self, Write};

const VERSION: &str = "0.2.0";

pub fn print_banner() {
    let banner = format!(
        r#"
                     __     _
   ____  __  _______/ /__  (_)
  / __ \/ / / / ___/ / _ \/ /
 / / / / /_/ / /__/ /  __/ /
/_/ /_/\__,_/\___/_/\___/_/   {}

		ruclei - Rust Nuclei Engine
"#,
        VERSION.bold()
    );
    eprintln!("{}", banner.bold());
}

/// Print an informational log line to stderr: [INF] message
pub fn log_info(msg: &str) {
    eprintln!("{} {}", "[INF]".bright_cyan().bold(), msg);
}

/// Print a warning log line to stderr: [WRN] message
pub fn log_warn(msg: &str) {
    eprintln!("{} {}", "[WRN]".bright_yellow().bold(), msg);
}

/// Print an error log line to stderr: [ERR] message
pub fn log_err(msg: &str) {
    eprintln!("{} {}", "[ERR]".bright_red().bold(), msg);
}

/// Print a debug line to stderr (only when verbose): [DBG] message
pub fn log_debug(msg: &str) {
    eprintln!("{} {}", "[DBG]".white().dimmed(), msg);
}

/// Print a finding line to stdout in exact nuclei format:
/// [timestamp] [template-id:matcher] [http] [severity] url
pub fn print_finding(
    template_id: &str,
    matcher_name: Option<&str>,
    severity: &str,
    url: &str,
    extracted: &std::collections::HashMap<String, Vec<String>>,
) {
    let ts = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // [template-id:matcher-name] or [template-id]
    let template_part = match matcher_name {
        Some(m) if !m.is_empty() => format!("[{}:{}]", template_id, m),
        _ => format!("[{}]", template_id),
    };

    let severity_colored = colorize_severity(severity);
    let protocol_colored = "[http]".bright_blue().bold().to_string();
    let ts_colored = format!("[{}]", ts).white().dimmed().to_string();
    let template_colored = template_part.bold().to_string();
    let url_colored = url.bold().to_string();

    // Build extracted data suffix
    let extracted_str = if extracted.is_empty() {
        String::new()
    } else {
        let parts: Vec<String> = extracted
            .iter()
            .map(|(k, vs)| format!("[{}=\"{}\"]", k, vs.join(",")))
            .collect();
        format!(" {}", parts.join(" ").bright_cyan())
    };

    // Acquire stdout lock to ensure the line is atomic
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    writeln!(
        handle,
        "{} {} {} {} {}{}",
        ts_colored,
        template_colored,
        protocol_colored,
        severity_colored,
        url_colored,
        extracted_str,
    )
    .ok();
}

fn colorize_severity(severity: &str) -> String {
    let s = format!("[{}]", severity.to_lowercase());
    match severity.to_lowercase().as_str() {
        "critical" => s.bright_red().bold().to_string(),
        "high" => s.red().bold().to_string(),
        "medium" => s.bright_yellow().bold().to_string(),
        "low" => s.bright_green().bold().to_string(),
        "info" => s.bright_blue().bold().to_string(),
        _ => s.white().to_string(),
    }
}
