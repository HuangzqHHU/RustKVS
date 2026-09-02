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

/// 主页：状态行 + 键值表 + 命令表单 + 执行结果（flash）
///
/// 注意：页面渲染使用 Server 的只读快照（status_snapshot / key_values），
/// 不执行 execute——避免页面刷新把内部读取计入 commands 计数。
fn index_page(server: &mut Server, flash: Option<&str>) -> String {
    // 只读状态行（不增加 commands 计数）
    let status = server.status_snapshot();

    // 只读键值数据（不增加 commands 计数）
    let rows_data = server.key_values();
    let mut rows = String::new();
    for (key, value) in &rows_data {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td></tr>",
            web::html_escape(key),
            web::html_escape(value)
        ));
    }

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

    // 页面更新时间戳（秒）：手动刷新(F5)后数字变化 = 确认拿到服务器最新页面
    let updated_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

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
<p style="color:#888;">更新时间: {}（按 F5 手动刷新后此数字会变化）</p>
{}
<h2>数据表</h2>
<table><tr><th>Key</th><th>Value</th></tr>{}</table>
<h2>执行命令</h2>
<form method="post" action="/cmd">
  <input type="text" name="command" id="cmd" placeholder="例如: SET course Rust 5" autofocus>
  <button type="submit">执行</button>
</form>
<p style="color:#888;">支持: SET key value [ttl] / GET key / DEL key / LIST / STATUS / PING（EXIT 不可用）</p>
</body></html>"#,
        web::html_escape(&status),
        updated_at,
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