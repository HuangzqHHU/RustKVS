//! Web 管理界面——HTTP 协议层（成员A负责）
//!
//! 纯 std 实现极简 HTTP（零第三方依赖），只支持：
//!   - GET（页面、查询）
//!   - POST（表单 application/x-www-form-urlencoded）
//!   - 每连接处理一个请求后关闭（Connection: close），不做 keep-alive/chunked
//!
//! 页面与路由在 webpage.rs（成员C）；本模块只负责协议与纯函数，
//! 通过 `serve_loop(listener, handler)` 与页面层解耦。

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 解析后的 HTTP 请求
#[derive(Debug, Clone)]
pub struct HttpRequest {
    /// "GET" / "POST"
    pub method: String,
    /// 路径，如 "/" "/get" "/cmd"（不含查询串）
    pub path: String,
    /// URL 查询参数（已解码），如 /get?key=course → [("key","course")]
    pub query: Vec<(String, String)>,
    /// POST 表单字段（已解码），如 command=SET course Rust
    pub body: Vec<(String, String)>,
}

/// 从一个带缓冲的读取器解析完整 HTTP 请求（请求行 + 头部 + body）
pub fn parse_request(reader: &mut impl BufRead) -> Option<HttpRequest> {
    // 1) 请求行：METHOD PATH HTTP/1.1
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).ok()? == 0 {
        return None;
    }
    let request_line = request_line.trim_end();
    if request_line.is_empty() {
        return None;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?; // 如 /cmd?x=1

    // 2) 拆 path 与 query
    let (path, query_string) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), Some(q)),
        None => (target.to_string(), None),
    };
    let mut query: Vec<(String, String)> = Vec::new();
    if let Some(q) = query_string {
        for pair in q.split('&') {
            if pair.is_empty() {
                continue;
            }
            match pair.split_once('=') {
                Some((k, v)) => query.push((url_decode(k), url_decode(v))),
                None => query.push((url_decode(pair), String::new())),
            }
        }
    }

    // 3) 头部：读到空行；记录 Content-Length
    let mut content_length: usize = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break; // 空行 = 头部结束
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    // 4) body（POST 表单）
    let mut body = String::new();
    if content_length > 0 {
        let mut buf = vec![0u8; content_length];
        if reader.read_exact(&mut buf).is_err() {
            return None;
        }
        body = String::from_utf8_lossy(&buf).to_string();
    }

    Some(HttpRequest {
        method,
        path,
        query,
        body: parse_form(&body),
    })
}

/// URL 解码：%XX 按十六进制还原字节，`+` 转空格
pub fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = &s[i + 1..i + 3];
                match u8::from_str_radix(hex, 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// 解析表单体：key=value&key2=value2（字段值已 URL 解码）
pub fn parse_form(body: &str) -> Vec<(String, String)> {
    let mut fields: Vec<(String, String)> = Vec::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        match pair.split_once('=') {
            Some((k, v)) => fields.push((url_decode(k), url_decode(v))),
            None => fields.push((url_decode(pair), String::new())),
        }
    }
    fields
}

/// HTML 转义（防 XSS：网页显示用户数据时必须使用）
pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// 组装 HTTP 响应（Content-Length 按 UTF-8 字节数计算，保证中文正确）
pub fn http_response(status: u16, content_type: &str, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        501 => "Not Implemented",
        _ => "Internal Server Error",
    };
    let len = body.as_bytes().len();
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
        status, reason, content_type, len, body
    )
}

/// Web 服务主循环：accept → 解析请求 → 交给 handler 生成响应 → 写回 → 关闭连接
///
/// handler 为闭包（由 server.rs 集成时提供），内部加锁调用 webpage::handle。
pub fn serve_loop<F>(listener: TcpListener, handler: F)
where
    F: Fn(&HttpRequest) -> String + Send + Sync + 'static,
{
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(_) => continue, // 单个连接失败不影响服务
        };
        // 读超时：防止恶意客户端不发完整请求导致线程挂死
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));

        let read_stream = match stream.try_clone() {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut reader = BufReader::new(read_stream);
        let mut writer = stream;

        let response = match parse_request(&mut reader) {
            Some(req) => handler(&req),
            None => http_response(400, "text/plain; charset=utf-8", "Bad Request: 无法解析请求"),
        };
        let _ = writer.write_all(response.as_bytes());
        let _ = writer.flush();
        // 连接在此关闭（Connection: close）
    }
}

/// serve_loop 的便捷启动（供 server.rs 使用，避免重复样板）
pub fn spawn_web_server(
    listener: TcpListener,
    server: Arc<Mutex<crate::server::Server>>,
) {
    std::thread::spawn(move || {
        serve_loop(listener, move |req| {
            let mut guard = server.lock().expect("服务器锁中毒");
            crate::webpage::handle(req, &mut guard)
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_decode_basic() {
        assert_eq!(url_decode("a%20b"), "a b");
        assert_eq!(url_decode("a+b"), "a b");
        assert_eq!(url_decode("hello"), "hello");
    }

    #[test]
    fn url_decode_utf8_chinese() {
        // "课程" 的 UTF-8 百分号编码
        assert_eq!(url_decode("%E8%AF%BE%E7%A8%8B"), "课程");
    }

    #[test]
    fn url_decode_invalid_percent_keeps_literal() {
        assert_eq!(url_decode("100%"), "100%");
    }

    #[test]
    fn parse_form_basic() {
        let fields = parse_form("command=SET course Rust&x=1");
        assert_eq!(fields[0], ("command".to_string(), "SET course Rust".to_string()));
        assert_eq!(fields[1], ("x".to_string(), "1".to_string()));
    }

    #[test]
    fn parse_form_url_decodes_values() {
        let fields = parse_form("command=SET%20k%20hello%20world");
        assert_eq!(fields[0], ("command".to_string(), "SET k hello world".to_string()));
    }

    #[test]
    fn parse_form_empty_body() {
        assert!(parse_form("").is_empty());
    }

    #[test]
    fn html_escape_special_chars() {
        assert_eq!(html_escape("<script>&\"'"), "&lt;script&gt;&amp;&quot;&#39;");
        assert_eq!(html_escape("普通文本"), "普通文本");
    }

    #[test]
    fn http_response_has_correct_headers() {
        let resp = http_response(200, "text/html; charset=utf-8", "你好");
        assert!(resp.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(resp.contains("Content-Type: text/html; charset=utf-8"));
        // "你好" UTF-8 = 6 字节
        assert!(resp.contains("Content-Length: 6"));
        assert!(resp.ends_with("\r\n\r\n你好"));
    }

    #[test]
    fn http_response_404_reason() {
        let resp = http_response(404, "text/html; charset=utf-8", "nope");
        assert!(resp.starts_with("HTTP/1.1 404 Not Found"));
    }
}
