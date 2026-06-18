use super::{cache::TemplateCache, Template};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub struct TemplateParser {
    cache: TemplateCache,
    /// When true, skip disk cache entirely (--no-cache flag)
    pub no_cache: bool,
}

impl Default for TemplateParser {
    fn default() -> Self {
        Self::new()
    }
}

impl TemplateParser {
    pub fn new() -> Self {
        Self {
            cache: TemplateCache::new(),
            no_cache: false,
        }
    }

    pub fn with_no_cache(mut self) -> Self {
        self.no_cache = true;
        self
    }

    pub fn clear_cache(&self) -> Result<()> {
        self.cache.clear()
    }

    pub fn parse_file<P: AsRef<Path>>(&self, path: P) -> Result<Template> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read template file: {}", path.display()))?;
        self.parse_string(&contents)
            .with_context(|| format!("Failed to parse template: {}", path.display()))
    }

    pub fn parse_string(&self, content: &str) -> Result<Template> {
        // Strip digest comment lines added by nuclei template signing
        let yaml_content: String = content
            .lines()
            .filter(|l| !l.trim_start().starts_with("# digest:"))
            .collect::<Vec<_>>()
            .join("\n");

        let template: Template =
            serde_yaml::from_str(&yaml_content).context("Failed to deserialize YAML template")?;

        self.validate_template(&template)?;
        Ok(template)
    }

    pub fn parse_directory<P: AsRef<Path>>(&self, dir_path: P) -> Result<Vec<Template>> {
        let dir_path = dir_path.as_ref();
        if !dir_path.is_dir() {
            return Err(anyhow::anyhow!("Not a directory: {}", dir_path.display()));
        }

        // Try disk cache first
        if !self.no_cache {
            if let Some(cached) = self.cache.load(dir_path) {
                return Ok(cached);
            }
        }

        // Cache miss — parse everything from YAML
        let mut templates = Vec::new();
        self.walk_directory(dir_path, &mut templates)?;

        // Persist to cache for next run
        if !self.no_cache && !templates.is_empty() {
            self.cache.save(dir_path, &templates);
        }

        Ok(templates)
    }

    fn walk_directory(&self, dir: &Path, templates: &mut Vec<Template>) -> Result<()> {
        let entries = fs::read_dir(dir)
            .with_context(|| format!("Failed to read directory: {}", dir.display()))?;

        let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        paths.sort();

        for path in paths {
            if path.is_dir() {
                self.walk_directory(&path, templates)?;
            } else if self.is_template_file(&path) {
                match self.parse_file(&path) {
                    Ok(t) => templates.push(t),
                    Err(e) => {
                        // Warn but continue loading other templates
                        eprintln!("Warning: Skipping {}: {:#}", path.display(), e);
                    }
                }
            }
        }
        Ok(())
    }

    fn is_template_file(&self, path: &Path) -> bool {
        matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("yaml") | Some("yml")
        )
    }

    fn validate_template(&self, template: &Template) -> Result<()> {
        if template.id.is_empty() {
            return Err(anyhow::anyhow!("Template missing required 'id' field"));
        }
        if template.info.name.is_empty() {
            return Err(anyhow::anyhow!(
                "Template '{}' missing required 'info.name' field",
                template.id
            ));
        }
        // Only load templates that have HTTP requests; skip dns/network/javascript quietly
        if !template.has_requests() {
            return Err(anyhow::anyhow!(
                "Template '{}' has no supported request types (dns/network/javascript not implemented)",
                template.id
            ));
        }
        // HTTP-only templates: skip those with only DNS/network requests
        let has_http = !template.requests.is_empty() || !template.http.is_empty();
        if !has_http {
            return Err(anyhow::anyhow!(
                "Template '{}' has no HTTP requests (only dns/network)",
                template.id
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_template() {
        let yaml = r#"
id: test-template
info:
  name: Test Template
  author: testuser
  severity: medium
  tags: test,http

requests:
  - method: GET
    path:
      - "/"
    matchers:
      - type: status
        status:
          - 200
"#;
        let parser = TemplateParser::new();
        let t = parser.parse_string(yaml).unwrap();
        assert_eq!(t.id, "test-template");
        assert_eq!(t.info.author, vec!["testuser"]);
        assert_eq!(t.info.severity, "medium");
    }

    #[test]
    fn test_parse_author_as_list() {
        let yaml = r#"
id: t2
info:
  name: T2
  author: [alice, bob]
  severity: info

http:
  - method: GET
    path:
      - "{{BaseURL}}"
"#;
        let parser = TemplateParser::new();
        let t = parser.parse_string(yaml).unwrap();
        assert_eq!(t.info.author, vec!["alice", "bob"]);
    }

    #[test]
    fn test_parse_with_digest_comment() {
        let yaml = r#"id: t3
info:
  name: T3
  author: user
  severity: high

http:
  - method: GET
    path:
      - "{{BaseURL}}"
# digest: abc123:xyz456
"#;
        let parser = TemplateParser::new();
        let t = parser.parse_string(yaml).unwrap();
        assert_eq!(t.id, "t3");
    }
}
