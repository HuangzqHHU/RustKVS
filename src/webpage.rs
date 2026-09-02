//! Web 管理界面——页面与路由（成员C负责）
//!
//! 依赖契约（web.rs，成员A已实现，签名勿改）：
//!   - web::HttpRequest { method, path, query, body }
//!   - web::html_escape / web::http_response
//! 复用：
//!   - parser::parse_command（成员C自己的 parser）
//!   - Server::execute（成员A，网页操作必须走它以保证持久化）
//!
//! 功能（见 WEB-PLAN.md）：
//!   GET /     主页：STATUS 状态行 + 键值数据表 + 命令执行表单
//!   GET /get  查询单个键（query: key=xxx）
//!   POST /cmd 执行命令（表单字段 command=整条命令文本）
//!   白名单：仅 SET/GET/DEL/LIST/STATUS/PING；EXIT 一律拒绝

//! Web 管理界面——页面与路由（成员C负责）

use crate::parser;
use crate::server::Server;
use crate::web::{self, HttpRequest};

/// 路由入口
pub fn handle(req: &HttpRequest, server: &mut Server) -> String {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => index_page(server, None),
        ("GET", "/data") => data_rows_response(server),
        ("GET", "/get") => query_key(req, server),
        ("POST", "/cmd") => run_command(req, server),
        _ => web::http_response(
            404,
            "text/html; charset=utf-8",
            "<h1>404 Not Found</h1><p><a href='/'>返回主页</a></p>",
        ),
    }
}

/// 把一行命令文本解析并执行，返回响应文本（或错误）
///
/// EXIT 为什么不会退出服务器：parse_command("EXIT") 成功，
/// 但 server.execute 返回 None → 这里转成错误提示返回。
fn run_line(server: &mut Server, line: &str) -> String {
    match parser::parse_command(line) {
        Ok(p) => match server.execute(&p) {
            Some(reply) => reply,
            None => "ERROR 该命令不允许在网页上执行".to_string(),
        },
        Err(e) => format!("ERROR {}", e.message),
    }
}

/// 执行网页展示用的查询，不增加用户命令计数。
fn run_line_without_count(server: &mut Server, line: &str) -> String {
    match parser::parse_command(line) {
        Ok(p) => server
            .execute_without_count(&p)
            .map(|reply| reply.to_string())
            .unwrap_or_else(|| "ERROR 该命令不允许在网页上执行".to_string()),
        Err(e) => format!("ERROR {}", e.message),
    }

}
/// 主页：状态行 + 键值表 + 命令表单 + 执行结果（flash）
fn index_page(server: &mut Server, flash: Option<&str>) -> String {
    let status = run_line_without_count(server, "STATUS");

    let rows = data_rows(server);

    let flash_html = match flash {
        Some(f) => {
            let color = if f.starts_with("ERROR") { "red" } else { "green" };
            format!(
                "<div style='color:{};margin:10px 0;'>{}</div>",
                color,
                web::html_escape(f)
            )
        }
        None => String::new(),
    };

    let body = format!(
        r#"<!DOCTYPE html>
<html lang="zh">
<head><meta charset="utf-8"><title>kvstore Web 管理</title>
<style>
body {{ font-family: "Microsoft YaHei", sans-serif; margin: 30px; }}
table {{ border-collapse: collapse; margin: 15px 0; }}
td, th {{ border: 1px solid #ccc; padding: 6px 14px; }}
th {{ background: #f0f0f0; }}
input[type=text] {{ width: 55%; padding: 6px; }}
button {{ padding: 6px 18px; }}
</style></head>
<body>
<h1>kvstore Web 管理</h1>
<p>{}</p>
{}
<h2>数据表 <button type="button" id="refresh-data" onclick="refreshDataTable()">刷新数据表</button></h2>
<table><tr><th>Key</th><th>Value</th></tr><tbody id="data-table">{}</tbody></table>
<h2>执行命令</h2>
<form method="post" action="/cmd">
  <input type="text" name="command" placeholder="例如: SET course Rust 5" autofocus>
  <button type="submit">执行</button>
</form>
<script>
async function refreshDataTable() {{
  try {{
    const response = await fetch(`/data?ts=${{Date.now()}}`, {{ cache: "no-store" }});
    if (!response.ok) return;
    document.getElementById("data-table").innerHTML = await response.text();
  }} catch (_) {{
    // 保留上一次成功显示的数据；用户可以再次点击按钮重试。
  }}
}}
</script>
<p style="color:#888;">支持: SET key value [ttl] / GET key / DEL key / LIST / STATUS / PING（EXIT 不可用）</p>
</body></html>"#,
        web::html_escape(&status),
        flash_html,
        if rows.is_empty() { "<tr><td colspan=2>（空）</td></tr>".to_string() } else { rows }
    );
    web::http_response(200, "text/html; charset=utf-8", &body)
}

/// GET /get?key=xxx：单键查询
fn query_key(req: &HttpRequest, server: &mut Server) -> String {
    let key = req.query.iter().find(|(k, _)| k == "key").map(|(_, v)| v.clone());
    match key {
        Some(k) => {
            let reply = run_line(server, &format!("GET {}", k));
            let body = format!(
                "<h1>查询结果</h1><pre>{}</pre><p><a href='/'>返回主页</a></p>",
                web::html_escape(&reply)
            );
            web::http_response(200, "text/html; charset=utf-8", &body)
        }
        None => web::http_response(
            400,
            "text/html; charset=utf-8",
            "<h1>400 缺少 key 参数</h1><p><a href='/'>返回主页</a></p>",
        ),
    }
}
/// 从当前 Server 状态生成数据表行；每次调用都会重新执行 LIST/GET。
fn data_rows(server: &mut Server) -> String {
    let list_reply = run_line_without_count(server, "LIST");
    let keys: Vec<&str> = match list_reply.strip_prefix("KEYS") {
        Some(rest) => rest.trim().split(' ').filter(|s| !s.is_empty()).collect(),
        None => Vec::new(),
    };
    let mut rows = String::new();
    for key in keys {
        let reply = run_line_without_count(server, &format!("GET {}", key));
        let value = reply
            .strip_prefix("VALUE")
            .and_then(|r| r.trim().split_once(' '))
            .map(|(_, value)| value.to_string())
            .unwrap_or_else(|| reply.clone());
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td></tr>",
            web::html_escape(key),
            web::html_escape(&value)
        ));
    }

    if rows.is_empty() {
        "<tr><td colspan=2>（空）</td></tr>".to_string()
    } else {
        rows
    }
}

/// GET /data：返回当前数据表行，供主页手动刷新使用。
fn data_rows_response(server: &mut Server) -> String {
    web::http_response(200, "text/html; charset=utf-8", &data_rows(server))
}

/// POST /cmd：执行命令，返回带结果的主页
fn run_command(req: &HttpRequest, server: &mut Server) -> String {
    let command = req
        .body
        .iter()
        .find(|(k, _)| k == "command")
        .map(|(_, v)| v.trim().to_string());
    match command {
        Some(line) if !line.is_empty() => {
            let result = run_line(server, &line);
            index_page(server, Some(&result))
        }
        _ => index_page(server, Some("ERROR 命令为空")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn test_server(tag: &str) -> (Server, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "kvstore_webpage_{}_{}.log",
            std::process::id(),
            tag
        ));
        let _ = std::fs::remove_file(&path);
        (Server::new(path.to_str().unwrap()), path)
    }

    fn get(path: &str) -> HttpRequest {
        HttpRequest {
            method: "GET".to_string(),
            path: path.to_string(),
            query: Vec::new(),
            body: Vec::new(),
        }
    }

    #[test]
    fn data_route_reflects_the_latest_written_value() {
        let (mut server, log_path) = test_server("latest_data");
        let set = parser::parse_command("SET course Rust").unwrap();
        assert_eq!(server.execute(&set), Some("OK".to_string()));

        let response = handle(&get("/data"), &mut server);

        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("<td>course</td><td>Rust</td>"));
        let _ = std::fs::remove_file(log_path);
    }

    #[test]
    fn homepage_uses_manual_refresh_without_automatic_reload() {
        let (mut server, log_path) = test_server("manual_refresh");
        let response = handle(&get("/"), &mut server);

        assert!(response.contains("id=\"refresh-data\""));
        assert!(response.contains("onclick=\"refreshDataTable()\""));
        assert!(!response.contains("http-equiv=\"refresh\""));
        assert!(!response.contains("window.setInterval"));
        let _ = std::fs::remove_file(log_path);
    }

    #[test]
    fn homepage_display_reads_do_not_increment_commands() {
        let (mut server, log_path) = test_server("display_counter");
        let set = parser::parse_command("SET course Rust").unwrap();
        assert_eq!(server.execute(&set), Some("OK".to_string()));

        let response = handle(&get("/"), &mut server);

        assert!(response.contains("commands=1"));
        let _ = std::fs::remove_file(log_path);
}
    }
