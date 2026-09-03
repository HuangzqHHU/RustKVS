# kvstore —— 可持久化网络键值存储系统

基于 Rust 的课程设计项目：支持数据保存、多客户端访问与 **Web 可视化管理**的键值存储系统。

**功能亮点**
- 服务器：TCP 多客户端并发、追加日志持久化、启动恢复、TTL 过期时间、实时状态统计
- 客户端：命令行交互（写入/查询/修改/删除/列表/状态）
- **Web 管理界面**：浏览器可视化查看与操作（数据表、命令执行、TTL 实时过期）
- 零第三方依赖（纯 Rust 标准库），**179 个自动化测试**覆盖

## 一、环境要求

- Rust 工具链（rustc ≥ 1.70，cargo）
- Windows / Linux / macOS 均可

## 二、编译

```bash
cargo build
```

## 三、启动服务器

```bash
cargo run -- server                                  # 默认：TCP 7878 + Web 8080
cargo run -- server --port 9000                      # 自定义命令端口
cargo run -- server --web-port 9001                  # 自定义 Web 端口（0 = 关闭 Web）
cargo run -- server --data mydata.log                # 自定义数据文件
cargo run -- server --local                          # 本地模式（无网络，调试用）
```

启动成功显示：监听地址、数据文件、Web 管理地址、运行状态。停止：`Ctrl+C`。

## 四、命令行客户端

```bash
cargo run -- client                                  # 连接默认服务器
cargo run -- client --local                          # 本地模式（不连网络）
```

### 命令表

| 命令 | 示例 | 说明 |
|---|---|---|
| `SET key value [ttl]` | `SET course Rust 5` | 写入或覆盖（可选 ttl：秒，到期后键消失） |
| `GET key` | `GET course` | 查询 |
| `DEL key` | `DEL course` | 删除 |
| `LIST` | `LIST` | 列出全部键 |
| `STATUS` | `STATUS` | 状态：数据量/连接数/运行时长/命令总数 |
| `PING` | `PING` | 检查连接 |
| `EXIT` | `EXIT` | 退出客户端 |

命令名大小写不敏感；错误命令返回明确中文提示，不影响后续命令。

## 五、Web 管理界面（创新扩展）

服务器启动后，浏览器打开 **`http://127.0.0.1:8080`**：

- **数据表**：实时查看全部键值（自动过滤已过期的 TTL 键）
- **执行命令**：在输入框直接输入 `SET course Rust 5` / `GET key` / `DEL key` 等，即时执行
- **状态行**：数据量、当前连接数、运行时长、命令总数
- **单键查询**：`http://127.0.0.1:8080/get?key=course`
- **数据接口**：`http://127.0.0.1:8080/data`（返回数据表 HTML，可被页面轮询）
- 与 TCP 客户端**共享同一份数据**：网页写入，命令行客户端能查到；反之亦然
- EXIT 在网页上不可用（防止退出服务器）

## 六、演示流程（对照课程验收要求）

1. **启动服务器**：显示监听地址和运行状态
2. **客户端连接**：`cargo run -- client`
3. **基本操作**：`SET course Rust` → `GET course` → `SET course Rust进阶`（覆盖）→ `GET notexist`（错误提示）
4. **多客户端**：同时开两个及以上客户端，各自读写互不影响
5. **重启恢复**：停止服务器 → 重新启动 → 再 `GET course`，数据仍在
6. **容错**：未知命令、缺参数给提示不崩溃；客户端直接关闭不影响服务器
7. **TTL 扩展**：`SET temp hello 5` → 立即查询有值 → 等 5 秒 → 查询返回"键不存在"
8. **Web 管理**：浏览器打开 8080 端口，查看数据、执行命令、观察 TTL 键消失

## 七、设计要点

- **协议**：一行一条消息，`\n` 结尾（见 `PROTOCOL.md`）
- **并发**：每连接一线程，`Arc<Mutex<Server>>` 共享状态；锁只在数据操作期间持有（等待网络 IO 不持锁）
- **持久化**：追加式日志 `data/kv.log`；先写日志再更新内存，启动时重放恢复；文件损坏明确报错不静默清空
- **TTL**：惰性过期（读时检查），`get/list/len` 自动过滤过期键
- **Web 零依赖**：`std::net` 手写极简 HTTP（GET + POST 表单），`execute` 拆三层分离命令计数与页面渲染
- **模块**：`protocol` / `parser` / `store` / `persistence` / `server` / `client` / `web` / `webpage`（见 `DESIGN.md`）

## 八、测试

```bash
cargo test          # 179 个测试：单元 + 集成 + 网络 + 持久化恢复 + 并发压力 + Web
```

## 九、项目结构

```
src/
  main.rs           # 入口（server / client 分发）
  lib.rs            # 库入口（各模块声明）
  protocol/         # 命令与消息格式（契约）
  parser.rs         # 命令解析与校验（含 TTL 解析）
  store/            # 内存键值存储（HashMap + TTL 过期）
  persistence.rs    # 追加日志与启动恢复
  server.rs         # TCP 服务器（并发 + 持久化 + execute 三层）
  client.rs         # 命令行客户端（本地 REPL + TCP REPL）
  web.rs            # Web HTTP 协议层（零依赖手写 HTTP）
  webpage.rs        # Web 页面与路由（数据表 + 命令执行）
tests/
  integration.rs        # 数据操作与解析集成测试
  network_tests.rs      # TCP 网络模式测试
  persistence_tests.rs  # 持久化恢复测试
  concurrency_tests.rs  # 多客户端并发测试
  stress_tests.rs       # 压力测试
  web_verify_tests.rs   # Web 端到端验证测试
data/               # 数据文件目录（运行时生成，不入库）
```
