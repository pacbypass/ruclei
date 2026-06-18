use crate::scanner::HttpResponse;
use crate::template::Extractor;
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;

/// Engine for extracting data from HTTP responses
#[derive(Debug)]
pub struct ExtractorEngine {
    /// Cache for compiled regex patterns
    regex_cache: HashMap<String, Regex>,
}

#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub extractor_name: Option<String>,
    pub extracted_values: Vec<String>,
}

impl ExtractionResult {
    pub fn new(extractor_name: Option<String>, values: Vec<String>) -> Self {
        Self {
            extractor_name,
            extracted_values: values,
        }
    }

    pub fn empty(extractor_name: Option<String>) -> Self {
        Self {
            extractor_name,
            extracted_values: Vec::new(),
        }
    }

    pub fn has_values(&self) -> bool {
        !self.extracted_values.is_empty()
    }
}

impl ExtractorEngine {
    pub fn new() -> Self {
        Self {
            regex_cache: HashMap::new(),
        }
    }

    /// Extract data using all extractors
    pub fn extract_all(
        &mut self,
        extractors: &[Extractor],
        response: &HttpResponse,
    ) -> Result<HashMap<String, Vec<String>>> {
        let mut results = HashMap::new();

        for extractor in extractors {
            let extraction_result = self.extract_single(extractor, response)?;

            if extraction_result.has_values() {
                let key = extraction_result
                    .extractor_name
                    .unwrap_or_else(|| format!("extractor_{}", results.len()));
                results.insert(key, extraction_result.extracted_values);
            }
        }

        Ok(results)
    }

    /// Extract data using a single extractor
    pub fn extract_single(
        &mut self,
        extractor: &Extractor,
        response: &HttpResponse,
    ) -> Result<ExtractionResult> {
        match extractor.extractor_type.as_str() {
            "regex" => self.extract_regex(extractor, response),
            "kval" => self.extract_kval(extractor, response),
            "xpath" => self.extract_xpath(extractor, response),
            "json" => self.extract_json(extractor, response),
            "dsl" => self.extract_dsl(extractor, response),
            _ => Err(anyhow::anyhow!(
                "Unsupported extractor type: {}",
                extractor.extractor_type
            )),
        }
    }

    /// Extract using regex patterns
    fn extract_regex(
        &mut self,
        extractor: &Extractor,
        response: &HttpResponse,
    ) -> Result<ExtractionResult> {
        if let Some(patterns) = &extractor.regex {
            let search_text = self.get_search_text(extractor, response);
            let mut extracted_values = Vec::new();

            for pattern in patterns {
                let regex = self.get_or_compile_regex(pattern)?;

                // Extract all matches
                for captures in regex.captures_iter(&search_text) {
                    // If group is specified, extract that specific group
                    if let Some(group_num) = extractor.group {
                        if let Some(capture) = captures.get(group_num as usize) {
                            extracted_values.push(capture.as_str().to_string());
                        }
                    } else {
                        // Extract all capture groups, or the full match if no groups
                        if captures.len() > 1 {
                            // Has capture groups, extract them
                            for i in 1..captures.len() {
                                if let Some(capture) = captures.get(i) {
                                    extracted_values.push(capture.as_str().to_string());
                                }
                            }
                        } else {
                            // No capture groups, extract full match
                            if let Some(full_match) = captures.get(0) {
                                extracted_values.push(full_match.as_str().to_string());
                            }
                        }
                    }
                }
            }

            Ok(ExtractionResult::new(
                extractor.name.clone(),
                extracted_values,
            ))
        } else {
            Err(anyhow::anyhow!("Regex extractor missing patterns"))
        }
    }

    /// Extract key-value pairs from headers or response
    fn extract_kval(
        &self,
        extractor: &Extractor,
        response: &HttpResponse,
    ) -> Result<ExtractionResult> {
        if let Some(keys) = &extractor.kval {
            let mut extracted_values = Vec::new();

            for key in keys {
                match extractor.part.as_deref() {
                    Some("header") => {
                        // Extract from headers
                        if let Some(value) = response.get_header(key) {
                            extracted_values.push(value.clone());
                        }
                    }
                    _ => {
                        // Extract from response body (simple key=value parsing)
                        let search_text = self.get_search_text(extractor, response);
                        if let Some(value) = self.extract_key_value(&search_text, key) {
                            extracted_values.push(value);
                        }
                    }
                }
            }

            Ok(ExtractionResult::new(
                extractor.name.clone(),
                extracted_values,
            ))
        } else {
            Err(anyhow::anyhow!("Kval extractor missing keys"))
        }
    }

    /// XPath extraction is not yet implemented; returns empty result silently
    fn extract_xpath(
        &self,
        extractor: &Extractor,
        _response: &HttpResponse,
    ) -> Result<ExtractionResult> {
        Ok(ExtractionResult::empty(extractor.name.clone()))
    }

    /// Extract using JSON path expressions
    fn extract_json(
        &self,
        extractor: &Extractor,
        response: &HttpResponse,
    ) -> Result<ExtractionResult> {
        if let Some(json_paths) = &extractor.json {
            let mut extracted_values = Vec::new();
            let search_text = self.get_search_text(extractor, response);

            // Try to parse as JSON
            if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&search_text) {
                for path in json_paths {
                    if let Some(value) = self.extract_json_path(&json_value, path) {
                        extracted_values.push(value);
                    }
                }
            }

            Ok(ExtractionResult::new(
                extractor.name.clone(),
                extracted_values,
            ))
        } else {
            Err(anyhow::anyhow!("JSON extractor missing paths"))
        }
    }

    /// Extract using DSL expressions
    fn extract_dsl(
        &self,
        extractor: &Extractor,
        response: &HttpResponse,
    ) -> Result<ExtractionResult> {
        if let Some(expressions) = &extractor.dsl {
            let mut extracted_values = Vec::new();

            for expr in expressions {
                if let Some(value) = self.evaluate_dsl_extraction(expr, response)? {
                    extracted_values.push(value);
                }
            }

            Ok(ExtractionResult::new(
                extractor.name.clone(),
                extracted_values,
            ))
        } else {
            Err(anyhow::anyhow!("DSL extractor missing expressions"))
        }
    }

    /// Get the text to search based on extractor part specification
    fn get_search_text(&self, extractor: &Extractor, response: &HttpResponse) -> String {
        match extractor.part.as_deref() {
            Some("header") => response.headers_string(),
            Some("body") => response.body.clone(),
            Some("all") | None => response.full_response(),
            _ => response.body.clone(), // Default to body
        }
    }

    /// Get or compile regex pattern, using cache
    fn get_or_compile_regex(&mut self, pattern: &str) -> Result<&Regex> {
        if !self.regex_cache.contains_key(pattern) {
            let regex = if let Some(case_insensitive) = pattern.strip_prefix("(?i)") {
                Regex::new(&format!("(?i){}", case_insensitive))
            } else {
                Regex::new(pattern)
            }
            .with_context(|| format!("Failed to compile regex pattern: {}", pattern))?;

            self.regex_cache.insert(pattern.to_string(), regex);
        }

        Ok(self.regex_cache.get(pattern).unwrap())
    }

    /// Extract key-value pair from text (simple implementation)
    fn extract_key_value(&self, text: &str, key: &str) -> Option<String> {
        for line in text.lines() {
            let line = line.trim();

            // Try different formats: key=value, key:value, key value
            for separator in &["=", ":", " "] {
                if let Some(pos) = line.find(&format!("{}{}", key, separator)) {
                    let value_start = pos + key.len() + separator.len();
                    if let Some(value_part) = line.get(value_start..) {
                        let value = value_part.trim();
                        // Extract until whitespace or common delimiters
                        let end_pos = value
                            .find(|c: char| c.is_whitespace() || "\"';,".contains(c))
                            .unwrap_or(value.len());
                        return Some(value[..end_pos].to_string());
                    }
                }
            }
        }
        None
    }

    /// Extract value from JSON using simple path (simplified implementation)
    fn extract_json_path(&self, json: &serde_json::Value, path: &str) -> Option<String> {
        // This is a very simplified JSON path implementation
        // A real implementation would use a proper JSONPath library

        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;

        for part in parts {
            // Remove array notation for now
            let clean_part = part.split('[').next().unwrap_or(part);

            match current {
                serde_json::Value::Object(obj) => {
                    if let Some(value) = obj.get(clean_part) {
                        current = value;
                    } else {
                        return None;
                    }
                }
                serde_json::Value::Array(arr) => {
                    // Handle array indexing
                    if let Some(bracket_start) = part.find('[') {
                        if let Some(bracket_end) = part.find(']') {
                            let index_str = &part[bracket_start + 1..bracket_end];
                            if let Ok(index) = index_str.parse::<usize>() {
                                if let Some(value) = arr.get(index) {
                                    current = value;
                                } else {
                                    return None;
                                }
                            } else {
                                return None;
                            }
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }

        // Convert final value to string
        match current {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            serde_json::Value::Bool(b) => Some(b.to_string()),
            serde_json::Value::Null => Some("null".to_string()),
            _ => Some(current.to_string()),
        }
    }

    /// Evaluate DSL expression for extraction
    fn evaluate_dsl_extraction(
        &self,
        expr: &str,
        response: &HttpResponse,
    ) -> Result<Option<String>> {
        // Simplified DSL evaluator for extraction

        if expr == "status_code" {
            return Ok(Some(response.status.to_string()));
        }

        if expr == "content_length" {
            return Ok(Some(response.content_length.to_string()));
        }

        if expr == "body" {
            return Ok(Some(response.body.clone()));
        }

        if expr.starts_with("header[") && expr.ends_with("]") {
            let header_name = &expr[7..expr.len() - 1]; // Remove "header[" and "]"
            if let Some(value) = response.get_header(header_name) {
                return Ok(Some(value.clone()));
            }
        }

        // For unrecognized expressions, return None
        Ok(None)
    }

    /// Clear the regex cache
    pub fn clear_cache(&mut self) {
        self.regex_cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        (self.regex_cache.len(), self.regex_cache.capacity())
    }
}

impl Default for ExtractorEngine {
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

    fn create_test_response(body: &str) -> HttpResponse {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("X-Custom-Header".to_string(), "custom-value".to_string());

        HttpResponse::new(
            200,
            headers,
            body.to_string(),
            body.len() as u64,
            Duration::from_millis(100),
            "https://example.com".to_string(),
        )
    }

    #[test]
    fn test_regex_extraction() {
        let mut engine = ExtractorEngine::new();
        let response = create_test_response("Version: 1.2.3, Build: 456");

        let extractor = Extractor {
            extractor_type: "regex".to_string(),
            name: Some("version".to_string()),
            regex: Some(vec![r"Version: (\d+\.\d+\.\d+)".to_string()]),
            ..Default::default()
        };

        let result = engine.extract_single(&extractor, &response).unwrap();
        assert!(result.has_values());
        assert_eq!(result.extracted_values, vec!["1.2.3"]);
    }

    #[test]
    fn test_kval_header_extraction() {
        let mut engine = ExtractorEngine::new();
        let response = create_test_response("test body");

        let extractor = Extractor {
            extractor_type: "kval".to_string(),
            name: Some("custom_header".to_string()),
            part: Some("header".to_string()),
            kval: Some(vec!["X-Custom-Header".to_string()]),
            ..Default::default()
        };

        let result = engine.extract_single(&extractor, &response).unwrap();
        assert!(result.has_values());
        assert_eq!(result.extracted_values, vec!["custom-value"]);
    }

    #[test]
    fn test_json_extraction() {
        let mut engine = ExtractorEngine::new();
        let json_body = r#"{"user": {"name": "John", "age": 30}, "status": "active"}"#;
        let response = create_test_response(json_body);

        let extractor = Extractor {
            extractor_type: "json".to_string(),
            name: Some("username".to_string()),
            json: Some(vec!["user.name".to_string()]),
            ..Default::default()
        };

        let result = engine.extract_single(&extractor, &response).unwrap();
        assert!(result.has_values());
        assert_eq!(result.extracted_values, vec!["John"]);
    }

    #[test]
    fn test_dsl_extraction() {
        let mut engine = ExtractorEngine::new();
        let response = create_test_response("test body");

        let extractor = Extractor {
            extractor_type: "dsl".to_string(),
            name: Some("status".to_string()),
            dsl: Some(vec!["status_code".to_string()]),
            ..Default::default()
        };

        let result = engine.extract_single(&extractor, &response).unwrap();
        assert!(result.has_values());
        assert_eq!(result.extracted_values, vec!["200"]);
    }
}
