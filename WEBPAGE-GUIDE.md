# webpage.rs 实现指引（成员C）

> 任务：在 `src/webpage.rs` 里实现 `handle()`——Web 管理页面的页面生成与路由。
> HTTP 协议层 `src/web.rs` 已由组长完成（**签名勿改**），你只需调用它。
> 本文含**完整参考实现**，可直接采用，但必须逐行理解（答辩要能讲解）。

## 一、你的契约（web.rs 提供，已实现）

```rust
pub struct HttpRequest {
    pub method: String,                  // "GET" / "POST"
    pub path: String,                    // "/" "/get" "/cmd"
    pub query: Vec<(String, String)>,    // /get?key=x → [("key","x")]（已解码）
    pub body: Vec<(String, String)>,     // POST 表单字段（已解码）
}
pub fn html_escape(s: &str) -> String;      // HTML 转义（防 XSS，必须用）
pub fn http_response(status: u16, content_type: &str, body: &str) -> String;
```

复用（你熟悉）：`parser::parse_command(line)`、`Server::execute(&parsed)`。
**注意**：网页执行命令必须走 `execute`（保证写日志持久化 + TTL + 校验全复用）。

## 二、功能要求

| 路由 | 行为 |
|---|---|
| `GET /` | 主页：STATUS 状态行 + 键值表格 + 命令输入框 + 上次执行结果 |
| `GET /get?key=xxx` | 单键查询结果页 |
| `POST /cmd` | 表单字段 `command`（整条命令文本）→ 执行 → 返回带结果的主页 |
| 其他 | 404 |

**白名单**：EXIT 天然被拦（见参考实现注释）——网页绝不能退出服务器。

## 三、完整参考实现（可直接采用，粘贴到 src/webpage.rs）

```rust
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
fn index_page(server: &mut Server, flash: Option<&str>) -> String {
    let status = run_line(server, "STATUS");

    // 数据表：LIST 拿全部键，逐个 GET 取值
    let list_reply = run_line(server, "LIST");
    let keys: Vec<&str> = match list_reply.strip_prefix("KEYS") {
        Some(rest) => rest.trim().split(' ').filter(|s| !s.is_empty()).collect(),
        None => Vec::new(),
    };
    let mut rows = String::new();
    for key in keys {
        let reply = run_line(server, &format!("GET {}", key));
        // GET 响应形如 "VALUE <key> <value>"；取 value 部分
        let value = reply
            .strip_prefix("VALUE")
            .and_then(|r| r.trim().split_once(' '))
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| reply.clone()); // 可能是过期/错误等
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td></tr>",
            web::html_escape(key),
            web::html_escape(&value)
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
<h2>数据表</h2>
<table><tr><th>Key</th><th>Value</th></tr>{}</table>
<h2>执行命令</h2>
<form method="post" action="/cmd">
  <input type="text" name="command" placeholder="例如: SET course Rust 5" autofocus>
  <button type="submit">执行</button>
</form>
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
```

## 四、必做自测（写完逐项验证）

```powershell
# 1. 编译 + 全量测试（不要破坏现有 175 个测试）
cargo build
cargo test

# 2. 启动服务器（自动带 Web 端口 8080）
cargo run -- server
#    看到 "Web 管理: http://127.0.0.1:8080" 即成功

# 3. 浏览器打开 http://127.0.0.1:8080
#    □ 看到"kvstore Web 管理"+ 状态行 + 空数据表
#    □ 输入 SET course Rust 5 执行 → 绿色 OK
#    □ 表格出现 course（值 Rust）
#    □ 等 6 秒刷新 → course 消失（TTL 可视化！）
#    □ 输入 GET missing → 红色 ERROR 键不存在
#    □ 输入 EXIT → 显示"该命令不允许在网页上执行"，服务器没退
#    □ 再开一个终端 cargo run -- client，客户端 GET course → 能查到（互通）
```

## 五、验收点（对照 WEB-PLAN.md）

1. GET / 显示主页（数据 + 状态）
2. 网页 SET/DEL/GET 执行正确、结果可见
3. TTL 在网页上可视化（6 秒后键消失）
4. 与 TCP 客户端数据互通
5. EXIT 被拒绝、服务器不退出
6. cargo test 全绿（不要改 web.rs/server.rs）

## 六、答辩要能讲的点（理解代码）

1. **为什么网页操作走 execute？** —— 保证"先写日志再更新内存"的持久化语义
2. **为什么值要 html_escape？** —— 用户数据含 `<script>` 会注入 HTML（XSS）
3. **EXIT 怎么被拦住的？** —— execute 返回 None（表示退出），run_line 转成错误
4. **锁在哪？** —— server.rs 的 spawn_web_server 闭包里 `lock`，网页操作与 TCP 操作互斥安全
