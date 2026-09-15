//! Admission for every loopback delivery route. Capability values never implement Debug.

use std::net::{Ipv4Addr, TcpListener};

use anyhow::{Context, Result, bail};
use axum::body::Body;
use axum::http::header::{
    ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
    ACCESS_CONTROL_EXPOSE_HEADERS, ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD,
    CACHE_CONTROL, CONTENT_LENGTH, HOST, ORIGIN, REFERRER_POLICY, TRANSFER_ENCODING, VARY,
    X_CONTENT_TYPE_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode};
use subtle::ConstantTimeEq;
use tauri::utils::config::Csp;

const CSP_PLACEHOLDER: &str = "http://127.0.0.1:0";
pub(crate) const PRODUCTION_ORIGIN: &str = "http://tauri.localhost";
const DEVELOPMENT_ORIGIN: &str = "http://localhost:1420";

pub(crate) fn random_capability() -> Result<String> {
    use std::fmt::Write;
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("could not generate local media capability")?;
    let mut token = String::with_capacity(64);
    for byte in bytes {
        write!(token, "{byte:02x}").expect("formatting into String cannot fail");
    }
    Ok(token)
}

pub(crate) struct BoundServer {
    pub listener: TcpListener,
    pub policy: DeliveryPolicy,
}

impl BoundServer {
    pub fn bind() -> Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .context("failed to bind the local playback server")?;
        listener.set_nonblocking(true)?;
        let authority = listener.local_addr()?.to_string();
        Ok(Self {
            listener,
            policy: DeliveryPolicy {
                authority,
                capability: random_capability()?,
                allow_development: cfg!(debug_assertions),
            },
        })
    }
}

pub(crate) struct DeliveryPolicy {
    authority: String,
    capability: String,
    allow_development: bool,
}

impl DeliveryPolicy {
    pub fn origin(&self) -> String {
        format!("http://{}", self.authority)
    }

    pub fn base_url(&self) -> String {
        format!("{}/cap/{}", self.origin(), self.capability)
    }

    /// Authenticate before routing or filesystem/network work. Preserve the load query.
    pub fn admit(&self, request: &mut Request<Body>) -> Result<Option<HeaderValue>, StatusCode> {
        let headers = request.headers();
        if request.uri().scheme().is_some()
            || request.uri().authority().is_some()
            || single_header(headers, HOST) != Some(self.authority.as_str())
        {
            return Err(StatusCode::FORBIDDEN);
        }
        let path = request
            .uri()
            .path_and_query()
            .ok_or(StatusCode::NOT_FOUND)?
            .as_str();
        let (token, route) = path
            .strip_prefix("/cap/")
            .and_then(|path| path.split_once('/'))
            .ok_or(StatusCode::NOT_FOUND)?;
        if !bool::from(token.as_bytes().ct_eq(self.capability.as_bytes())) {
            return Err(StatusCode::NOT_FOUND);
        }
        let origin = if headers.contains_key(ORIGIN) {
            let origin = single_header(headers, ORIGIN).ok_or(StatusCode::FORBIDDEN)?;
            if origin != PRODUCTION_ORIGIN
                && !(self.allow_development && origin == DEVELOPMENT_ORIGIN)
            {
                return Err(StatusCode::FORBIDDEN);
            }
            Some(HeaderValue::from_str(origin).map_err(|_| StatusCode::FORBIDDEN)?)
        } else {
            None
        };
        if headers.contains_key(TRANSFER_ENCODING)
            || (headers.contains_key(CONTENT_LENGTH)
                && single_header(headers, CONTENT_LENGTH) != Some("0"))
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        match *request.method() {
            Method::GET | Method::HEAD => {}
            Method::OPTIONS => {
                if origin.is_none()
                    || !matches!(
                        single_header(headers, ACCESS_CONTROL_REQUEST_METHOD),
                        Some("GET" | "HEAD")
                    )
                    || (headers.contains_key(ACCESS_CONTROL_REQUEST_HEADERS)
                        && !single_header(headers, ACCESS_CONTROL_REQUEST_HEADERS)
                            .is_some_and(|value| value.eq_ignore_ascii_case("range")))
                {
                    return Err(StatusCode::FORBIDDEN);
                }
            }
            _ => return Err(StatusCode::METHOD_NOT_ALLOWED),
        }
        *request.uri_mut() = format!("/{route}")
            .parse()
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        Ok(origin)
    }
}

fn single_header(headers: &HeaderMap, name: axum::http::header::HeaderName) -> Option<&str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}

pub(crate) fn response_headers(response: &mut Response<Body>, origin: Option<HeaderValue>) {
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(VARY, HeaderValue::from_static("Origin"));
    if let Some(origin) = origin {
        headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        headers.insert(
            ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, HEAD"),
        );
        headers.insert(
            ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Range"),
        );
        headers.insert(
            ACCESS_CONTROL_EXPOSE_HEADERS,
            HeaderValue::from_static("Accept-Ranges, Content-Range, Content-Length, Content-Type"),
        );
    }
}

/// Called before Tauri clones its configuration or creates the first WebView.
pub(crate) fn restrict_csp(csp: &mut Option<Csp>, origin: &str) -> Result<()> {
    let Some(policy) = csp else {
        bail!("local media requires an explicit CSP")
    };
    let current = policy.to_string();
    if !current.contains(CSP_PLACEHOLDER) || current.contains("127.0.0.1:*") {
        bail!("local media CSP must contain the reserved port-zero source")
    }
    *policy = Csp::Policy(current.replace(CSP_PLACEHOLDER, origin));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_csp_keeps_tauri_sources_and_uses_only_reserved_port() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let mut csp = Some(Csp::Policy(
            config["app"]["security"]["csp"]
                .as_str()
                .unwrap()
                .to_owned(),
        ));
        restrict_csp(&mut csp, "http://127.0.0.1:54321").unwrap();
        let policy = csp.unwrap().to_string();
        assert!(!policy.contains(CSP_PLACEHOLDER));
        assert!(!policy.contains("127.0.0.1:*"));
        assert_eq!(policy.matches("http://127.0.0.1:54321").count(), 3);
        assert!(policy.contains("http://ipc.localhost"));
    }

    #[test]
    fn capabilities_are_independent_and_not_debuggable_or_persisted() {
        let a = random_capability().unwrap();
        let b = random_capability().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
