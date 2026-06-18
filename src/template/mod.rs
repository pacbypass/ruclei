use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fmt;

pub mod cache;
pub mod parser;

fn string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("string or list of strings")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Vec<String>, E> {
            Ok(vec![v.to_string()])
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<String>, A::Error> {
            let mut out = Vec::new();
            while let Some(s) = seq.next_element::<serde_yaml::Value>()? {
                out.push(match s {
                    serde_yaml::Value::String(st) => st,
                    serde_yaml::Value::Number(n) => n.to_string(),
                    other => format!("{:?}", other),
                });
            }
            Ok(out)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Template {
    pub id: String,
    pub info: Info,
    #[serde(default)]
    pub requests: Vec<Request>,
    #[serde(default)]
    pub http: Vec<Request>,
    #[serde(default)]
    pub dns: Vec<DnsRequest>,
    #[serde(default)]
    pub network: Vec<NetworkRequest>,
    #[serde(default)]
    pub variables: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub constants: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Info {
    pub name: String,
    #[serde(deserialize_with = "string_or_vec")]
    pub author: Vec<String>,
    #[serde(default)]
    pub severity: String,
    pub description: Option<String>,
    pub reference: Option<serde_yaml::Value>,
    pub tags: Option<serde_yaml::Value>,
    pub classification: Option<Classification>,
    pub metadata: Option<HashMap<String, serde_yaml::Value>>,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Classification {
    #[serde(rename = "cve-id")]
    pub cve_id: Option<serde_yaml::Value>,
    #[serde(rename = "cwe-id")]
    pub cwe_id: Option<serde_yaml::Value>,
    #[serde(rename = "cvss-metrics")]
    pub cvss_metrics: Option<String>,
    #[serde(rename = "cvss-score")]
    pub cvss_score: Option<f64>,
    pub cpe: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Request {
    pub method: Option<String>,
    #[serde(default)]
    pub path: Vec<String>,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    #[serde(rename = "raw", default)]
    pub raw_request: Vec<String>,
    pub matchers: Option<Vec<Matcher>>,
    pub extractors: Option<Vec<Extractor>>,
    #[serde(rename = "matchers-condition")]
    pub matchers_condition: Option<String>,
    #[serde(rename = "max-redirects")]
    pub max_redirects: Option<u32>,
    pub redirects: Option<bool>,
    pub pipeline: Option<bool>,
    #[serde(rename = "unsafe")]
    pub unsafe_request: Option<bool>,
    pub race: Option<bool>,
    pub threads: Option<u32>,
    pub attack: Option<String>,
    pub payloads: Option<HashMap<String, serde_yaml::Value>>,
    #[serde(rename = "stop-at-first-match")]
    pub stop_at_first_match: Option<bool>,
    #[serde(rename = "req-condition")]
    pub req_condition: Option<bool>,
    #[serde(rename = "iterate-all")]
    pub iterate_all: Option<bool>,
    pub cookie_reuse: Option<bool>,
    #[serde(rename = "disable-path-automerge")]
    pub disable_path_automerge: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DnsRequest {
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub record_type: Option<String>,
    pub class: Option<String>,
    pub retries: Option<u32>,
    pub matchers: Option<Vec<Matcher>>,
    pub extractors: Option<Vec<Extractor>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NetworkRequest {
    pub address: Option<String>,
    pub data: Option<String>,
    pub matchers: Option<Vec<Matcher>>,
    pub extractors: Option<Vec<Extractor>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Matcher {
    #[serde(rename = "type")]
    pub matcher_type: String,
    pub condition: Option<String>,
    pub part: Option<String>,
    pub status: Option<Vec<u16>>,
    pub size: Option<Vec<i64>>,
    pub words: Option<Vec<String>>,
    pub regex: Option<Vec<String>>,
    pub binary: Option<Vec<String>>,
    pub dsl: Option<Vec<String>>,
    pub encoding: Option<String>,
    pub case_insensitive: Option<bool>,
    pub negative: Option<bool>,
    pub name: Option<String>,
    pub internal: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Extractor {
    #[serde(rename = "type")]
    pub extractor_type: String,
    pub name: Option<String>,
    pub part: Option<String>,
    pub group: Option<u32>,
    pub regex: Option<Vec<String>>,
    pub kval: Option<Vec<String>>,
    pub xpath: Option<Vec<String>>,
    pub json: Option<Vec<String>>,
    pub dsl: Option<Vec<String>>,
    pub attribute: Option<String>,
    pub internal: Option<bool>,
    pub case_insensitive: Option<bool>,
}

impl Template {
    pub fn get_http_requests(&self) -> Vec<&Request> {
        if !self.http.is_empty() {
            self.http.iter().collect()
        } else {
            self.requests.iter().collect()
        }
    }

    pub fn has_requests(&self) -> bool {
        !self.requests.is_empty()
            || !self.http.is_empty()
            || !self.dns.is_empty()
            || !self.network.is_empty()
    }

    pub fn severity_level(&self) -> u8 {
        match self.info.severity.to_lowercase().as_str() {
            "info" => 1,
            "low" => 2,
            "medium" => 3,
            "high" => 4,
            "critical" => 5,
            _ => 0,
        }
    }

    pub fn tags(&self) -> Vec<String> {
        match &self.info.tags {
            Some(serde_yaml::Value::Sequence(seq)) => seq
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            Some(serde_yaml::Value::String(s)) => {
                s.split(',').map(|t| t.trim().to_string()).collect()
            }
            _ => vec![],
        }
    }
}

impl Default for Request {
    fn default() -> Self {
        Self {
            method: Some("GET".to_string()),
            path: vec!["/".to_string()],
            headers: None,
            body: None,
            raw_request: vec![],
            matchers: None,
            extractors: None,
            matchers_condition: Some("or".to_string()),
            max_redirects: Some(3),
            redirects: Some(true),
            pipeline: None,
            unsafe_request: None,
            race: None,
            threads: None,
            attack: None,
            payloads: None,
            stop_at_first_match: None,
            req_condition: None,
            iterate_all: None,
            cookie_reuse: None,
            disable_path_automerge: None,
        }
    }
}

impl Default for Extractor {
    fn default() -> Self {
        Self {
            extractor_type: "regex".to_string(),
            name: None,
            part: Some("body".to_string()),
            group: None,
            regex: None,
            kval: None,
            xpath: None,
            json: None,
            dsl: None,
            attribute: None,
            internal: None,
            case_insensitive: None,
        }
    }
}
