use crate::scanner::HttpResponse;
use crate::template::Matcher;
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;

#[derive(Debug)]
pub struct MatcherEngine {
    regex_cache: HashMap<String, Regex>,
}

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub matched: bool,
    pub matcher_name: Option<String>,
    pub matched_values: Vec<String>,
}

impl MatchResult {
    pub fn matched(name: Option<String>) -> Self {
        Self { matched: true, matcher_name: name, matched_values: vec![] }
    }

    pub fn matched_with_values(name: Option<String>, values: Vec<String>) -> Self {
        Self { matched: true, matcher_name: name, matched_values: values }
    }

    pub fn not_matched() -> Self {
        Self { matched: false, matcher_name: None, matched_values: vec![] }
    }
}

impl MatcherEngine {
    pub fn new() -> Self {
        Self { regex_cache: HashMap::new() }
    }

    /// Evaluate all matchers with a request-level condition ("and"/"or").
    pub fn evaluate_matchers(
        &mut self,
        matchers: &[Matcher],
        condition: &str,
        response: &HttpResponse,
    ) -> Result<MatchResult> {
        if matchers.is_empty() {
            return Ok(MatchResult::not_matched());
        }

        let mut results = Vec::with_capacity(matchers.len());
        for matcher in matchers {
            let r = self.evaluate_single(matcher, response)?;
            results.push(r);
        }

        let overall = match condition {
            "and" => results.iter().all(|r| r.matched),
            _ => results.iter().any(|r| r.matched),
        };

        if overall {
            let first = results.into_iter().find(|r| r.matched)
                .unwrap_or_else(MatchResult::not_matched);
            Ok(first)
        } else {
            Ok(MatchResult::not_matched())
        }
    }

    fn evaluate_single(&mut self, matcher: &Matcher, response: &HttpResponse) -> Result<MatchResult> {
        let result = match matcher.matcher_type.as_str() {
            "status" => self.match_status(matcher, response)?,
            "size" => self.match_size(matcher, response)?,
            "word" | "words" => self.match_words(matcher, response)?,
            "regex" => self.match_regex(matcher, response)?,
            "binary" => self.match_binary(matcher, response)?,
            "dsl" => self.match_dsl(matcher, response)?,
            "xpath" => MatchResult::not_matched(), // not implemented; skip silently
            t => {
                return Err(anyhow::anyhow!("Unsupported matcher type: {}", t));
            }
        };

        if matcher.negative.unwrap_or(false) {
            Ok(MatchResult {
                matched: !result.matched,
                matcher_name: result.matcher_name,
                matched_values: result.matched_values,
            })
        } else {
            Ok(result)
        }
    }

    fn match_status(&self, matcher: &Matcher, response: &HttpResponse) -> Result<MatchResult> {
        let codes = matcher.status.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Status matcher missing status codes"))?;
        if codes.contains(&response.status) {
            Ok(MatchResult::matched_with_values(
                matcher.name.clone(),
                vec![response.status.to_string()],
            ))
        } else {
            Ok(MatchResult::not_matched())
        }
    }

    fn match_size(&self, matcher: &Matcher, response: &HttpResponse) -> Result<MatchResult> {
        let sizes = matcher.size.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Size matcher missing sizes"))?;
        let body_size = response.body.len() as i64;
        if sizes.contains(&body_size) {
            Ok(MatchResult::matched_with_values(
                matcher.name.clone(),
                vec![body_size.to_string()],
            ))
        } else {
            Ok(MatchResult::not_matched())
        }
    }

    fn match_words(&self, matcher: &Matcher, response: &HttpResponse) -> Result<MatchResult> {
        let words = matcher.words.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Word matcher missing words"))?;
        let text = self.get_part_text(matcher.part.as_deref(), response);
        let ci = matcher.case_insensitive.unwrap_or(false);
        let text_cmp = if ci { text.to_lowercase() } else { text.clone() };

        // Inner condition: "and" requires ALL words, "or" requires ANY
        let inner_cond = matcher.condition.as_deref().unwrap_or("or");

        let matched_words: Vec<String> = words.iter()
            .filter(|w| {
                let w_cmp = if ci { w.to_lowercase() } else { w.to_string() };
                text_cmp.contains(&w_cmp)
            })
            .cloned()
            .collect();

        let matched = match inner_cond {
            "and" => matched_words.len() == words.len(),
            _ => !matched_words.is_empty(),
        };

        if matched {
            Ok(MatchResult::matched_with_values(matcher.name.clone(), matched_words))
        } else {
            Ok(MatchResult::not_matched())
        }
    }

    fn match_regex(&mut self, matcher: &Matcher, response: &HttpResponse) -> Result<MatchResult> {
        let patterns = matcher.regex.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Regex matcher missing patterns"))?;
        let text = self.get_part_text(matcher.part.as_deref(), response);
        let inner_cond = matcher.condition.as_deref().unwrap_or("or");
        let mut matched_values = Vec::new();

        for pattern in patterns {
            let re = self.get_or_compile(pattern)?;
            if re.is_match(&text) {
                matched_values.push(pattern.clone());
            }
        }

        let matched = match inner_cond {
            "and" => matched_values.len() == patterns.len(),
            _ => !matched_values.is_empty(),
        };

        if matched {
            Ok(MatchResult::matched_with_values(matcher.name.clone(), matched_values))
        } else {
            Ok(MatchResult::not_matched())
        }
    }

    fn match_binary(&self, matcher: &Matcher, response: &HttpResponse) -> Result<MatchResult> {
        let patterns = matcher.binary.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Binary matcher missing patterns"))?;
        let bytes = response.body.as_bytes();
        let mut found = Vec::new();

        for pattern in patterns {
            let hex = pattern.replace(' ', "");
            if let Ok(needle) = hex::decode(&hex) {
                if !needle.is_empty() && bytes.windows(needle.len()).any(|w| w == needle.as_slice()) {
                    found.push(pattern.clone());
                }
            }
        }

        if !found.is_empty() {
            Ok(MatchResult::matched_with_values(matcher.name.clone(), found))
        } else {
            Ok(MatchResult::not_matched())
        }
    }

    fn match_dsl(&mut self, matcher: &Matcher, response: &HttpResponse) -> Result<MatchResult> {
        let exprs = matcher.dsl.as_ref()
            .ok_or_else(|| anyhow::anyhow!("DSL matcher missing expressions"))?;
        let inner_cond = matcher.condition.as_deref().unwrap_or("or");
        let mut matched_exprs = Vec::new();

        for expr in exprs {
            if eval_dsl_bool(expr, response) {
                matched_exprs.push(expr.clone());
            }
        }

        let matched = match inner_cond {
            "and" => matched_exprs.len() == exprs.len(),
            _ => !matched_exprs.is_empty(),
        };

        if matched {
            Ok(MatchResult::matched_with_values(matcher.name.clone(), matched_exprs))
        } else {
            Ok(MatchResult::not_matched())
        }
    }

    fn get_part_text(&self, part: Option<&str>, response: &HttpResponse) -> String {
        match part {
            Some("header") | Some("headers") => response.headers_string(),
            Some("body") | None => response.body.clone(),
            Some("all") | Some("raw") | Some("response") => response.full_response(),
            Some(other) => {
                if other.starts_with("header") {
                    response.headers_string()
                } else {
                    response.body.clone()
                }
            }
        }
    }

    fn get_or_compile(&mut self, pattern: &str) -> Result<&Regex> {
        if !self.regex_cache.contains_key(pattern) {
            let re = Regex::new(pattern)
                .with_context(|| format!("Invalid regex: {}", pattern))?;
            self.regex_cache.insert(pattern.to_string(), re);
        }
        Ok(self.regex_cache.get(pattern).unwrap())
    }
}

// ─── DSL evaluator ────────────────────────────────────────────────────────────

/// Evaluate a nuclei DSL expression against an HTTP response, returning bool.
/// Returns false on any unrecognized expression rather than erroring.
fn eval_dsl_bool(expr: &str, resp: &HttpResponse) -> bool {
    let expr = expr.trim();

    match expr.to_lowercase().as_str() {
        "true" => return true,
        "false" => return false,
        _ => {}
    }

    // !expr  (negation of a sub-expression)
    if let Some(inner) = expr.strip_prefix('!') {
        let inner = inner.trim();
        if !inner.starts_with(|c: char| c.is_alphabetic()) {
            return !eval_dsl_bool(inner, resp);
        }
        // fall through — might be a function like !contains(...)
    }

    // contains(haystack, needle)
    if let Some(args) = strip_fn(expr, "contains") {
        let (h, n) = split_args(&args);
        return resolve_str(&h, resp).contains(&resolve_str(&n, resp));
    }
    if let Some(args) = strip_fn(expr, "!contains") {
        let (h, n) = split_args(&args);
        return !resolve_str(&h, resp).contains(&resolve_str(&n, resp));
    }

    // startswith / endswith
    if let Some(args) = strip_fn(expr, "startswith") {
        let (s, p) = split_args(&args);
        return resolve_str(&s, resp).starts_with(&resolve_str(&p, resp));
    }
    if let Some(args) = strip_fn(expr, "endswith") {
        let (s, p) = split_args(&args);
        return resolve_str(&s, resp).ends_with(&resolve_str(&p, resp));
    }

    // regex(pattern, text)
    if let Some(args) = strip_fn(expr, "regex") {
        let (pat, src) = split_args(&args);
        let pat_s = resolve_str(&pat, resp);
        let src_s = resolve_str(&src, resp);
        return Regex::new(&pat_s).map(|re| re.is_match(&src_s)).unwrap_or(false);
    }

    // len(x) operator N
    if let Some(rest) = expr.strip_prefix("len(") {
        if let Some(inner_close) = rest.find(')') {
            let inner = &rest[..inner_close];
            let after = rest[inner_close + 1..].trim();
            let val = resolve_str(inner, resp).len() as i64;
            if let Some((op, rhs)) = parse_cmp(after) {
                if let Ok(n) = rhs.trim().parse::<i64>() {
                    return apply_cmp(val, &op, n);
                }
            }
            return val > 0;
        }
    }

    // Numeric comparisons for well-known variables
    for var in &["status_code", "content_length", "body_size"] {
        if let Some(rest) = expr.strip_prefix(*var) {
            let rest = rest.trim();
            if rest.is_empty() {
                // bare variable reference — evaluate as bool (truthy)
                return true;
            }
            if let Some((op, rhs)) = parse_cmp(rest) {
                let lhs = resolve_num(var, resp);
                if let Ok(n) = rhs.trim().parse::<i64>() {
                    return apply_cmp(lhs, &op, n);
                }
            }
        }
    }

    // Generic binary comparison: lhs op rhs
    for op in &["==", "!=", ">=", "<=", ">", "<"] {
        if let Some(pos) = find_op(expr, op) {
            let lhs = expr[..pos].trim();
            let rhs = expr[pos + op.len()..].trim();
            let lv = resolve_str(lhs, resp);
            let rv = resolve_str(rhs, resp);
            // Try numeric comparison first
            if let (Ok(ln), Ok(rn)) = (lv.parse::<i64>(), rv.parse::<i64>()) {
                return apply_cmp(ln, op, rn);
            }
            return match *op {
                "==" => lv == rv,
                "!=" => lv != rv,
                _ => false,
            };
        }
    }

    false
}

fn strip_fn(expr: &str, name: &str) -> Option<String> {
    let prefix = format!("{}(", name);
    if expr.to_lowercase().starts_with(&prefix.to_lowercase()) && expr.ends_with(')') {
        Some(expr[prefix.len()..expr.len() - 1].to_string())
    } else {
        None
    }
}

/// Split two args at the top-level comma (respects nested parens and quotes)
fn split_args(args: &str) -> (String, String) {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut str_char = '"';
    for (i, c) in args.char_indices() {
        if in_str {
            if c == str_char { in_str = false; }
            continue;
        }
        match c {
            '"' | '\'' => { in_str = true; str_char = c; }
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                return (
                    args[..i].trim().to_string(),
                    args[i + 1..].trim().to_string(),
                );
            }
            _ => {}
        }
    }
    (args.trim().to_string(), String::new())
}

/// Resolve a DSL token to its string value
fn resolve_str(token: &str, resp: &HttpResponse) -> String {
    let token = token.trim();
    // Strip quotes
    if (token.starts_with('"') && token.ends_with('"'))
        || (token.starts_with('\'') && token.ends_with('\''))
    {
        return token[1..token.len() - 1].to_string();
    }
    match token.to_lowercase().as_str() {
        "body" => resp.body.clone(),
        "header" | "all_headers" | "raw_header" => resp.headers_string(),
        "status_code" => resp.status.to_string(),
        "content_length" | "body_size" => resp.content_length.to_string(),
        "content_type" => resp.get_header("content-type").cloned().unwrap_or_default(),
        "location" => resp.get_header("location").cloned().unwrap_or_default(),
        "server" => resp.get_header("server").cloned().unwrap_or_default(),
        _ => {
            // Try as a header name
            if let Some(v) = resp.get_header(token) {
                return v.clone();
            }
            // Unknown variable (ip, cname, Host, etc.) → empty string
            String::new()
        }
    }
}

fn resolve_num(token: &str, resp: &HttpResponse) -> i64 {
    match token.to_lowercase().as_str() {
        "status_code" => resp.status as i64,
        "content_length" | "body_size" => resp.content_length as i64,
        _ => 0,
    }
}

/// Find the position of a comparison operator in an expression, skipping
/// operators that are inside parens or quotes
fn find_op(expr: &str, op: &str) -> Option<usize> {
    let bytes = expr.as_bytes();
    let op_bytes = op.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut str_char = b'"';
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == str_char { in_str = false; }
            i += 1;
            continue;
        }
        if c == b'"' || c == b'\'' { in_str = true; str_char = c; i += 1; continue; }
        if c == b'(' { depth += 1; i += 1; continue; }
        if c == b')' { depth -= 1; i += 1; continue; }
        if depth == 0 && bytes[i..].starts_with(op_bytes) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_cmp(s: &str) -> Option<(String, &str)> {
    for op in &["==", "!=", ">=", "<=", ">", "<"] {
        if let Some(rest) = s.trim().strip_prefix(op) {
            return Some((op.to_string(), rest));
        }
    }
    None
}

fn apply_cmp(lhs: i64, op: &str, rhs: i64) -> bool {
    match op {
        "==" => lhs == rhs,
        "!=" => lhs != rhs,
        ">=" => lhs >= rhs,
        "<=" => lhs <= rhs,
        ">" => lhs > rhs,
        "<" => lhs < rhs,
        _ => false,
    }
}

impl Default for MatcherEngine {
    fn default() -> Self { Self::new() }
}

impl Default for Matcher {
    fn default() -> Self {
        Self {
            matcher_type: "status".to_string(),
            condition: None,
            part: None,
            status: None,
            size: None,
            words: None,
            regex: None,
            binary: None,
            dsl: None,
            encoding: None,
            case_insensitive: None,
            negative: None,
            name: None,
            internal: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::Duration;

    fn make_response(status: u16, body: &str) -> HttpResponse {
        let mut h = HashMap::new();
        h.insert("content-type".to_string(), "text/html; charset=utf-8".to_string());
        h.insert("x-powered-by".to_string(), "PHP/7.4".to_string());
        HttpResponse::new(
            status, h, body.to_string(), body.len() as u64,
            Duration::from_millis(50), "https://example.com".to_string(),
        )
    }

    #[test]
    fn test_status_matcher() {
        let mut e = MatcherEngine::new();
        let r = e.evaluate_matchers(
            &[Matcher { matcher_type: "status".to_string(), status: Some(vec![200]), ..Default::default() }],
            "or",
            &make_response(200, "ok"),
        ).unwrap();
        assert!(r.matched);
    }

    #[test]
    fn test_matchers_condition_and_fails_when_one_misses() {
        let mut e = MatcherEngine::new();
        let resp = make_response(200, "hello");
        let matchers = vec![
            Matcher { matcher_type: "status".to_string(), status: Some(vec![200]), ..Default::default() },
            Matcher {
                matcher_type: "word".to_string(),
                words: Some(vec!["NOTHERE".to_string()]),
                ..Default::default()
            },
        ];
        let r = e.evaluate_matchers(&matchers, "and", &resp).unwrap();
        assert!(!r.matched, "AND condition: both must match");

        let r = e.evaluate_matchers(&matchers, "or", &resp).unwrap();
        assert!(r.matched, "OR condition: status matches so overall true");
    }

    #[test]
    fn test_word_and_condition() {
        let mut e = MatcherEngine::new();
        let resp = make_response(200, "hello world");
        let m = Matcher {
            matcher_type: "word".to_string(),
            words: Some(vec!["hello".to_string(), "world".to_string()]),
            condition: Some("and".to_string()),
            ..Default::default()
        };
        assert!(e.evaluate_matchers(&[m], "or", &resp).unwrap().matched);
    }

    #[test]
    fn test_dsl_contains_body() {
        let resp = make_response(200, "vulnerability found in system");
        assert!(eval_dsl_bool("contains(body, \"vulnerability\")", &resp));
        assert!(!eval_dsl_bool("contains(body, \"xyznotfound\")", &resp));
    }

    #[test]
    fn test_dsl_status_code() {
        let resp = make_response(200, "ok");
        assert!(eval_dsl_bool("status_code == 200", &resp));
        assert!(!eval_dsl_bool("status_code == 404", &resp));
        assert!(eval_dsl_bool("status_code != 404", &resp));
    }

    #[test]
    fn test_dsl_not_contains() {
        let resp = make_response(200, "hello world");
        assert!(eval_dsl_bool("!contains(body, \"notpresent\")", &resp));
        assert!(!eval_dsl_bool("!contains(body, \"hello\")", &resp));
    }

    #[test]
    fn test_negative_matcher() {
        let mut e = MatcherEngine::new();
        let resp = make_response(200, "ok");
        let m = Matcher {
            matcher_type: "status".to_string(),
            status: Some(vec![404]),
            negative: Some(true),
            ..Default::default()
        };
        assert!(e.evaluate_matchers(&[m], "or", &resp).unwrap().matched);
    }
}
