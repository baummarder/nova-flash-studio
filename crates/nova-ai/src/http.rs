//! Small HTTP abstraction so provider behavior can be tested without a network.
//!
//! The production transport uses `reqwest` blocking mode. Tests use
//! [`MockTransport`], which records requests and returns queued responses. No
//! request or response body is written to a log by this module.

use std::sync::Mutex;
use std::time::Duration;

use crate::error::{AiError, AiResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    /// Header values may contain a credential. Callers must never print this
    /// struct with its headers attached.
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout_secs: u64,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: "GET".into(),
            url: url.into(),
            headers: Vec::new(),
            body: None,
            timeout_secs: 30,
        }
    }

    pub fn post_json(url: impl Into<String>, body: Vec<u8>) -> Self {
        Self {
            method: "POST".into(),
            url: url.into(),
            headers: vec![("content-type".into(), "application/json".into())],
            body: Some(body),
            timeout_secs: 120,
        }
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn timeout(mut self, seconds: u64) -> Self {
        self.timeout_secs = seconds.max(1);
        self
    }

    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn json(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            body: body.into(),
        }
    }
}

pub trait HttpTransport: Send + Sync {
    fn execute(&self, request: HttpRequest) -> AiResult<HttpResponse>;
}

pub struct ReqwestTransport {
    client: reqwest::blocking::Client,
}

impl ReqwestTransport {
    pub fn new() -> AiResult<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("NOVA-Flash-Studio/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| AiError::Network(error.to_string()))?;
        Ok(Self { client })
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            // A client without custom configuration still exists for every
            // platform reqwest supports; this branch is only a last resort.
            client: reqwest::blocking::Client::new(),
        })
    }
}

impl HttpTransport for ReqwestTransport {
    fn execute(&self, request: HttpRequest) -> AiResult<HttpResponse> {
        let method = reqwest::Method::from_bytes(request.method.as_bytes())
            .map_err(|error| AiError::Config(format!("Ungültige HTTP-Methode: {error}")))?;
        let mut builder = self
            .client
            .request(method, &request.url)
            .timeout(Duration::from_secs(request.timeout_secs.max(1)));
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let response = builder.send().map_err(|error| {
            if error.is_timeout() {
                AiError::Timeout(redact_url(&request.url))
            } else {
                AiError::Network(error.to_string())
            }
        })?;
        let status = response.status().as_u16();
        let body = response
            .bytes()
            .map_err(|error| AiError::Network(error.to_string()))?
            .to_vec();
        Ok(HttpResponse { status, body })
    }
}

fn redact_url(url: &str) -> String {
    // API keys are sent in headers, never query strings, but avoid echoing a
    // query string in timeout diagnostics anyway.
    url.split('?').next().unwrap_or(url).to_string()
}

#[derive(Debug)]
pub struct MockTransport {
    responses: Mutex<Vec<HttpResponse>>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl MockTransport {
    pub fn new(responses: Vec<HttpResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn push(&self, response: HttpResponse) {
        self.responses.lock().unwrap().push(response);
    }

    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl HttpTransport for MockTransport {
    fn execute(&self, request: HttpRequest) -> AiResult<HttpResponse> {
        self.requests.lock().unwrap().push(request);
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            return Err(AiError::Network("Mock-Antwort fehlt".into()));
        }
        Ok(responses.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_records_requests_and_returns_queued_responses() {
        let transport = MockTransport::new(vec![HttpResponse::json(200, b"{}".to_vec())]);
        let request =
            HttpRequest::get("https://example.invalid/api").header("Authorization", "Bearer x");
        let response = transport.execute(request.clone()).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(transport.requests(), vec![request]);
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let request = HttpRequest::get("x").header("Authorization", "Bearer test");
        assert_eq!(request.header_value("authorization"), Some("Bearer test"));
    }
}
