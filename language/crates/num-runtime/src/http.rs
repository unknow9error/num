use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

impl HttpRequest {
    pub fn new(
        method: impl Into<String>,
        path: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            headers: BTreeMap::new(),
            body: body.into(),
        }
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub reason: String,
    pub body: String,
    pub content_type: String,
}

impl HttpResponse {
    pub fn text(status: u16, reason: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            status,
            reason: reason.into(),
            body: body.into(),
            content_type: "text/plain; charset=utf-8".to_string(),
        }
    }

    pub fn json(status: u16, reason: impl Into<String>, body: serde_json::Value) -> Self {
        let body = serde_json::to_string_pretty(&body).unwrap_or_else(|_| {
            "{\"error\":{\"kind\":\"internal\",\"code\":\"json_render_failed\"}}".to_string()
        });
        Self {
            status,
            reason: reason.into(),
            body: format!("{body}\n"),
            content_type: "application/json; charset=utf-8".to_string(),
        }
    }
}

pub fn serve_once<F>(addr: &str, handler: F) -> Result<(), String>
where
    F: FnOnce(HttpRequest) -> HttpResponse,
{
    let listener =
        TcpListener::bind(addr).map_err(|err| format!("failed to bind {addr}: {err}"))?;
    serve_listener_once(&listener, handler)
}

pub fn serve<F>(addr: &str, max_requests: Option<usize>, mut handler: F) -> Result<(), String>
where
    F: FnMut(HttpRequest) -> HttpResponse,
{
    let listener =
        TcpListener::bind(addr).map_err(|err| format!("failed to bind {addr}: {err}"))?;
    let mut served = 0_usize;
    loop {
        serve_listener_once(&listener, &mut handler)?;
        served += 1;
        if max_requests.is_some_and(|limit| served >= limit) {
            return Ok(());
        }
    }
}

fn serve_listener_once<F>(listener: &TcpListener, handler: F) -> Result<(), String>
where
    F: FnOnce(HttpRequest) -> HttpResponse,
{
    let (mut stream, _) = listener
        .accept()
        .map_err(|err| format!("failed to accept HTTP connection: {err}"))?;

    let raw = read_request(&mut stream)?;

    let response = match parse_request(&raw) {
        Ok(request) => handler(request),
        Err(message) => HttpResponse::json(
            400,
            "Bad Request",
            serde_json::json!({
                "error": {
                    "kind": "parse",
                    "code": "invalid_http_request",
                    "message": message,
                    "request_id": null,
                    "correlation_id": null,
                }
            }),
        ),
    };
    stream
        .write_all(&response_bytes(&response))
        .map_err(|err| format!("failed to write HTTP response: {err}"))?;
    Ok(())
}

fn read_request(reader: &mut impl Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    let header_end = loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| format!("failed to read HTTP request: {err}"))?;
        if read == 0 {
            break find_header_end(&bytes)
                .ok_or_else(|| "connection closed before HTTP headers completed".to_string())?;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_HEADER_BYTES && find_header_end(&bytes).is_none() {
            return Err(format!("HTTP headers exceed {} bytes", MAX_HEADER_BYTES));
        }
        if let Some(header_end) = find_header_end(&bytes) {
            break header_end;
        }
    };

    let head = String::from_utf8_lossy(&bytes[..header_end]);
    let expected_body_len = content_length(&head)?;
    if expected_body_len > MAX_BODY_BYTES {
        return Err(format!("HTTP body exceeds {} bytes", MAX_BODY_BYTES));
    }

    let body_start = header_end + header_separator_len(&bytes[header_end..]);
    while bytes.len().saturating_sub(body_start) < expected_body_len {
        let read = reader
            .read(&mut buffer)
            .map_err(|err| format!("failed to read HTTP request body: {err}"))?;
        if read == 0 {
            return Err(format!(
                "connection closed before reading declared Content-Length {}",
                expected_body_len
            ));
        }
        bytes.extend_from_slice(&buffer[..read]);
    }

    Ok(String::from_utf8_lossy(&bytes).to_string())
}

pub fn parse_request(raw: &str) -> Result<HttpRequest, String> {
    let (head, body) = raw
        .split_once("\r\n\r\n")
        .or_else(|| raw.split_once("\n\n"))
        .unwrap_or((raw, ""));
    let request_line = head
        .lines()
        .next()
        .ok_or_else(|| "empty HTTP request".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "missing HTTP method".to_string())?;
    let target = parts
        .next()
        .ok_or_else(|| "missing HTTP path".to_string())?;
    let version = parts
        .next()
        .ok_or_else(|| "missing HTTP version".to_string())?;

    if !version.starts_with("HTTP/") {
        return Err(format!("unsupported HTTP request line: {request_line}"));
    }
    let body = match content_length(head)? {
        0 => "",
        len => {
            let body_bytes = body.as_bytes();
            if body_bytes.len() < len {
                return Err(format!("request body shorter than Content-Length {}", len));
            }
            std::str::from_utf8(&body_bytes[..len])
                .map_err(|_| "HTTP request body is not valid UTF-8".to_string())?
        }
    };

    Ok(HttpRequest {
        method: method.to_ascii_uppercase(),
        path: target.split('?').next().unwrap_or(target).to_string(),
        headers: parse_headers(head),
        body: body.to_string(),
    })
}

pub fn response_bytes(response: &HttpResponse) -> Vec<u8> {
    let body = response.body.as_bytes();
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        response.reason,
        response.content_type,
        body.len(),
        response.body
    )
    .into_bytes()
}

fn content_length(head: &str) -> Result<usize, String> {
    for line in head.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            return value
                .trim()
                .parse::<usize>()
                .map_err(|_| format!("invalid Content-Length header '{}'", value.trim()));
        }
    }
    Ok(0)
}

fn parse_headers(head: &str) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    for line in head.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    headers
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .or_else(|| bytes.windows(2).position(|window| window == b"\n\n"))
}

fn header_separator_len(separator: &[u8]) -> usize {
    if separator.starts_with(b"\r\n\r\n") {
        4
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_request, read_request, response_bytes, HttpRequest, HttpResponse};
    use std::io::Cursor;

    #[test]
    fn parses_request_line_and_body() {
        let request = parse_request(
            "POST /refunds?trace=1 HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n{}",
        )
        .unwrap();

        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/refunds");
        assert_eq!(request.header("host"), Some("localhost"));
        assert_eq!(request.body, "{}");
    }

    #[test]
    fn parses_headers_with_case_insensitive_lookup() {
        let request = parse_request(
            "POST /refunds HTTP/1.1\r\nX-Tenant: tenant_a\r\nx-request-id: req_1\r\nContent-Length: 0\r\n\r\n",
        )
        .unwrap();

        assert_eq!(request.header("x-tenant"), Some("tenant_a"));
        assert_eq!(request.header("X-Request-Id"), Some("req_1"));
    }

    #[test]
    fn request_new_uses_empty_headers() {
        let request = HttpRequest::new("GET", "/health", "");

        assert_eq!(request.header("x-tenant"), None);
    }

    #[test]
    fn parse_request_trims_body_to_content_length() {
        let request = parse_request(
            "POST /refunds HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\n\r\n{}extra",
        )
        .unwrap();

        assert_eq!(request.body, "{}");
    }

    #[test]
    fn read_request_uses_content_length_to_finish_body() {
        let mut reader = Cursor::new(
            b"POST /refunds HTTP/1.1\r\nHost: localhost\r\nContent-Length: 11\r\n\r\nhello worldtrailing bytes".to_vec(),
        );
        let raw = read_request(&mut reader).unwrap();
        let request = parse_request(&raw).unwrap();

        assert_eq!(request.body, "hello world");
    }

    #[test]
    fn parse_request_rejects_short_body() {
        let err = parse_request(
            "POST /refunds HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\n{}",
        )
        .unwrap_err();

        assert!(err.contains("shorter than Content-Length"));
    }

    #[test]
    fn renders_text_response() {
        let response = HttpResponse::text(200, "OK", "done\n");
        let bytes = response_bytes(&response);
        let text = String::from_utf8(bytes).unwrap();

        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Length: 5\r\n"));
        assert!(text.ends_with("done\n"));
    }

    #[test]
    fn renders_json_response() {
        let response = HttpResponse::json(
            400,
            "Bad Request",
            serde_json::json!({"error": {"kind": "parse", "code": "invalid_http_request"}}),
        );
        let bytes = response_bytes(&response);
        let text = String::from_utf8(bytes).unwrap();

        assert!(text.starts_with("HTTP/1.1 400 Bad Request\r\n"));
        assert!(text.contains("Content-Type: application/json; charset=utf-8\r\n"));
        assert!(text.contains("\"kind\": \"parse\""));
        assert!(text.ends_with("}\n"));
    }
}
