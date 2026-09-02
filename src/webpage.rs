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

use crate::server::Server;
use crate::web::{self, HttpRequest};

/// 路由入口：根据请求返回完整 HTTP 响应字符串
///
/// TODO(成员C)：实现主页 HTML（数据表 + 表单）、/get 查询、/cmd 执行。
/// 提示：
///   1. 数据表：先 LIST 拿全部键，再逐个 GET 取值，拼成 <table>；
///   2. 所有用户数据（键/值/错误信息）必须过 web::html_escape；
///   3. POST /cmd：取 body 中 command 字段 → parser::parse_command(line)
///      → server.execute(&parsed) → 结果嵌回主页；
///      command 为 EXIT 时返回错误提示（不得退出服务器）；
///   4. 可先用 GET / 输出固定 HTML 跑通浏览器链路，再补数据表与 /cmd。
pub fn handle(req: &HttpRequest, server: &mut Server) -> String {
    // 骨架实现：GET / 返回占位主页（保证链路可通）；其余 404/501
    let _ = server;
    match req.method.as_str() {
        "GET" => match req.path.as_str() {
            "/" => web::http_response(
                200,
                "text/html; charset=utf-8",
                "<html><head><meta charset=\"utf-8\"></head><body>\
                 <h1>kvstore Web 管理</h1>\
                 <p>主页待实现（成员C）：将显示数据表与命令表单</p>\
                 </body></html>",
            ),
            _ => web::http_response(404, "text/html; charset=utf-8", "<h1>404 Not Found</h1>"),
        },
        _ => web::http_response(501, "text/html; charset=utf-8", "<h1>501 Not Implemented</h1>"),
    }
}
