# kvstore —— 可持久化网络键值存储系统

基于 Rust 的课程设计项目：支持数据保存和多客户端访问的键值存储系统。

- 服务器：TCP 监听、多客户端并发、追加日志持久化、启动恢复
- 客户端：命令行交互（写入/查询/修改/删除/列表/状态）
- 全程 128 个自动化测试覆盖

## 一、环境要求

- Rust 工具链（rustc ≥ 1.70，cargo）
- Windows / Linux / macOS 均可

## 二、编译

```bash
cargo build
```

## 三、启动服务器

```bash
cargo run -- server                          # 默认 127.0.0.1:7878，数据 data/kv.log
cargo run -- server --port 9000              # 自定义端口
cargo run -- server --data mydata.log        # 自定义数据文件
cargo run -- server --local                  # 本地模式（无网络，调试用）
```

启动成功显示监听地址、数据文件和运行状态。停止：`Ctrl+C`。

## 四、启动客户端

```bash
cargo run -- client                          # 连接默认服务器
cargo run -- client --local                  # 本地模式（不连网络）
```

### 命令表

| 命令 | 示例 | 说明 |
|---|---|---|
| `SET key value` | `SET course Rust` | 写入或覆盖（值可含空格） |
| `GET key` | `GET course` | 查询 |
| `DEL key` | `DEL course` | 删除 |
| `LIST` | `LIST` | 列出全部键 |
| `STATUS` | `STATUS` | 查看数据数量 |
| `PING` | `PING` | 检查连接 |
| `EXIT` | `EXIT` | 退出客户端 |

命令名大小写不敏感（`set`/`SET`/`Set` 均可）；错误命令返回明确中文提示，不影响后续命令。

## 五、演示流程（对照课程验收要求）

1. **启动服务器**：显示监听地址和运行状态
2. **客户端连接**：`cargo run -- client`
3. **基本操作**：`SET course Rust` → `GET course` → `SET course Rust进阶`（覆盖）→ `GET notexist`（错误提示）
4. **多客户端**：同时开两个及以上客户端，各自读写互不影响
5. **重启恢复**：停止服务器 → 重新启动 → 再 `GET course`，数据仍在
6. **容错**：未知命令、缺参数给提示不崩溃；客户端直接关闭不影响服务器

## 六、设计要点

- **协议**：一行一条消息，`\n` 结尾；命令+空格分隔参数（见 `PROTOCOL.md`）
- **并发**：每连接一线程，`Arc<Mutex<Server>>` 共享状态；锁只在数据操作期间持有
- **持久化**：追加式日志 `data/kv.log`；先写日志再更新内存，启动时重放恢复；文件损坏明确报错不静默清空
- **模块**：`protocol` / `parser` / `store` / `persistence` / `server` / `client`（见 `DESIGN.md`）

## 七、测试

```bash
cargo test          # 128 个测试：单元 + 集成 + 网络 + 持久化恢复
```

## 八、项目结构

```
src/
  main.rs           # 入口（server / client 分发）
  lib.rs            # 库入口（各模块声明）
  protocol/         # 命令与消息格式（契约）
  parser.rs         # 命令解析与校验
  store/            # 内存键值存储（HashMap）
  persistence.rs    # 追加日志与启动恢复
  server.rs         # TCP 服务器（并发 + 持久化）
  client.rs         # 命令行客户端（本地 REPL + TCP REPL）
tests/
  integration.rs        # 数据操作与解析集成测试
  network_tests.rs      # TCP 网络模式测试
  persistence_tests.rs  # 持久化恢复测试
data/               # 数据文件目录（运行时生成，不入库）
```
