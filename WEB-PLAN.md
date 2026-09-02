# Web 管理界面——四人并行工作安排（读写版）

> 目标：实现 PPT 明示扩展项"Web 管理"——浏览器可视化查看 + 操作键值存储
> 技术路线：**纯 std 零依赖**，手写极简 HTTP；与现有 TCP 命令服务共享同一 Server
> 组长：成员A | 预计：4-6 小时（半天冲刺）

## 〇、功能范围（读写版）

| 路由 | 功能 |
|---|---|
| `GET /` | HTML 主页：STATUS 状态行 + 键值数据表 + 命令执行表单 |
| `GET /get?key=xxx` | 查询单个键（返回文本） |
| `POST /cmd` | 执行命令（表单字段 `command=SET course Rust 5`），返回结果页 |
| 其他 | 404 提示 |

- 支持命令：`SET key value [ttl]` / `GET key` / `DEL key` / `LIST` / `STATUS` / `PING`
- **禁止**：`EXIT`（网页不能退出服务器）——收到 EXIT 返回错误提示
- 网页操作**必须走 `Server::execute`**（保证先写日志再更新内存的持久化语义）
- 与 TCP 客户端**共享同一份数据**：网页写的数据，客户端能查到；反之亦然

## 一、架构（复用现有 Server）

```
浏览器 ──HTTP──> web线程(8080) ──┐
TCP客户端 ──> TCP线程(7878) ─────┼──► Arc<Mutex<Server>>
                                 │      ├─ execute（写日志+更新内存，锁内）
                                 └──────┴─ 完全共享
```

新增端口 **8080**（HTTP），与 7878（TCP 命令）并存。`run_network` 里 spawn 一个 web 线程，`Arc::clone(&server)` 传过去。

## 二、接口契约（先定死，防冲突！）

### A 的 `src/web.rs`（HTTP 协议层，纯函数，可单测）

```rust
pub struct HttpRequest {
    pub method: String,          // "GET" / "POST"
    pub path: String,            // "/" "/get" "/cmd" 等
    pub query: Vec<(String, String)>,  // URL 查询参数（已解码）
    pub body: Vec<(String, String)>,   // POST 表单字段（已解码）
}
pub fn parse_request(reader: &mut impl BufRead) -> Option<HttpRequest>
    // 读请求行 → 读 header（遇空行）→ 按 Content-Length 读 body → 解析
pub fn url_decode(s: &str) -> String      // %XX 解码，+ 转空格
pub fn parse_form(body: &str) -> Vec<(String, String)>  // key=value&...
pub fn html_escape(s: &str) -> String     // & < > " ' → 实体（防 XSS）
pub fn http_response(status: u16, content_type: &str, body: &str) -> String
    // 组装：HTTP/1.1 200 OK\r\nContent-Type...\r\nContent-Length...\r\n\r\nbody
pub fn serve_loop(listener: TcpListener, server: Arc<Mutex<Server>>)
    // accept → parse_request → 调 webpage::handle → 写回响应 → 关闭连接
```

### C 的 `src/webpage.rs`（页面 + 路由，依赖 web.rs 的契约）

```rust
pub fn handle(req: &web::HttpRequest, server: &mut Server) -> String
    // 返回 http_response(...) 生成的完整响应字符串
    // GET /     → 主页：STATUS + 数据表（LIST 后逐个 GET）+ 命令表单 + 上次结果
    // GET /get  → 查询单键
    // POST /cmd → 取 command 字段 → parser::parse_command → server.execute
    //             → EXIT 拒绝；结果嵌回页面
pub fn index_html(status_line: &str, rows: &[(String, String)], result: Option<&str>) -> String
    // HTML 模板函数（便于单测）
```

### server.rs（A 集成）

- 新增参数 `--web-port <端口>`（默认 8080，常量 `DEFAULT_WEB_PORT` 放 protocol）
- `run_network` 里 spawn web 线程：
  `let web_server = Arc::clone(&server); thread::spawn(move || web::serve_loop(web_listener, web_server));`

### lib.rs（A）：注册 `pub mod web; pub mod webpage;`

## 三、四人任务分配

| 成员 | 负责文件 | 任务 | 验收点 |
|---|---|---|---|
| **A（组长）** | `src/web.rs`（新建）、`src/server.rs`、`src/lib.rs`、`src/protocol/mod.rs` | ① HTTP 协议层（请求解析/URL解码/表单/转义/响应组装/serve_loop）② server 集成（--web-port 参数 + spawn web 线程）③ protocol 加 `DEFAULT_WEB_PORT` ④ lib 注册模块 | `cargo build` 通过；手工浏览器打开 8080 能看到页面 |
| **C** | `src/webpage.rs`（新建） | HTML 页面（状态行+数据表+命令表单）+ 路由分发；复用 `parser::parse_command` 与 `Server::execute`；EXIT 拒绝 | 网页能执行 SET/GET/DEL/LIST/STATUS；结果正确显示 |
| **B** | `src/store/mod.rs`（如需辅助）、验证 | ① Web 与 TCP 数据互通验证（网页 SET → 客户端 GET）② 网页操作后**重启恢复**验证 ③ 并发下 Web 线程锁安全复核（与 TCP 线程同时读写） | 互通/恢复/并发三个验证脚本通过 |
| **D** | `tests/web_tests.rs`（新建） | ① web.rs 单元测试：url_decode / parse_form / html_escape / http_response ② 端到端：起服务器 → HTTP GET / 断言含数据 → POST /cmd 执行 SET → 再 GET 验证 → TTL 操作 | 新增测试全绿（不依赖浏览器，用 std TCP 发 HTTP 请求） |

## 四、开发顺序与集成节点

```
14:00 A 定 web.rs 全部函数签名（契约）并实现 HTTP 层；C 同步按契约写 webpage.rs
      （两人并行，互不阻塞：A 不依赖 C，C 只依赖 A 已公布的签名）
15:30 第一次集成：cargo build → 修签名不一致 → 浏览器手工测试 GET /
16:00 B 开始互通/恢复验证；D 写测试（先单测后端到端）
17:00 集成节点：
  cargo build + cargo test 全绿
  浏览器走查：打开页面 → 网页 SET course Rust 5 → 表格出现 → TCP 客户端 GET 能查到
  → 等 6 秒刷新 → course 消失（TTL 可视化）→ 重启服务器 → 网页数据仍在
17:30 提交 tag: day4-web → push
```

## 五、验收标准

1. `cargo run -- server` 后浏览器打开 `http://127.0.0.1:8080` 显示主页
2. 网页执行 `SET course Rust 5` → OK，表格立即显示 course
3. TCP 客户端 `GET course` → 能查到（**双向互通**）
4. 6 秒后刷新网页 → course 消失（TTL 生效）
5. 网页 `DEL` → 表格消失；`GET missing` → 错误提示
6. 网页输入 `EXIT` → 被拒绝，服务器不退出
7. 网页写入数据 → 重启服务器 → 数据恢复（持久化）
8. `cargo test` 全绿（含新增 web 测试）

## 六、风险与应对

| 风险 | 应对 |
|---|---|
| 手写 HTTP 解析 bug（读 body、Content-Length） | 只支持 GET + POST 表单；每连接处理一个请求后关闭（Connection: close），不做 keep-alive/chunked |
| 中文乱码 | 响应头 `Content-Type: text/html; charset=utf-8`；测试用 UTF-8 |
| 网页执行 EXIT 退出服务器 | webpage 层白名单：只允许 SET/GET/DEL/LIST/STATUS/PING |
| 两个文件接口不一致 | A 先定签名（第 14:00 契约），C 严格按契约写；15:30 提前集成 |
| 时间不够 | 降级：只做 GET / 主页 + 数据表（不做 POST），保住"可视化查看"展示点 |
