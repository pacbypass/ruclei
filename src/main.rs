use anyhow::Result;
use ruclei::{cli, output, RucleiScanner};

fn main() -> Result<()> {
    let config = cli::parse_args()?;

    let silent = config.silent;
    let verbose = config.verbose;

    if !silent {
        output::print_banner();
    }

    let mut scanner = RucleiScanner::new(config)?;

    scanner.load_templates()?;

    let results = scanner.run()?;

    scanner.write_results(&results)?;

    if !silent && verbose {
        output::log_info(&format!(
            "Scan finished. {} matches found.",
            scanner.stats.matches_found
        ));
    }

    // Exit 0 = found vulnerabilities, exit 1 = nothing found (nuclei convention)
    let has_matches = results.iter().any(|r| r.matched);
    std::process::exit(if has_matches { 0 } else { 1 });
}
