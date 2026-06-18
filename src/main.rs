use anyhow::Result;
use ruclei::{cli, output, template::cache::TemplateCache, RucleiScanner};

fn main() -> Result<()> {
    let config = cli::parse_args()?;

    let silent = config.silent;
    let verbose = config.verbose;
    let dry_run = config.dry_run;

    if !silent {
        output::print_banner();
    }

    // --clear-cache: no scanner needed
    if config.clear_cache {
        TemplateCache::new().clear()?;
        output::log_info("Template cache cleared.");
        return Ok(());
    }

    let mut scanner = RucleiScanner::new(config)?;

    scanner.load_templates()?;

    if dry_run {
        output::log_info(&format!(
            "Dry run: {} templates loaded, exiting.",
            scanner.stats.templates_loaded
        ));
        return Ok(());
    }

    let results = scanner.run()?;

    scanner.write_results(&results)?;

    if !silent && verbose {
        output::log_info(&format!(
            "Scan finished. {} matches found.",
            scanner.stats.matches_found
        ));
    }

    let has_matches = results.iter().any(|r| r.matched);
    std::process::exit(if has_matches { 0 } else { 1 });
}
