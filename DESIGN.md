# 设计说明 v1（模块划分与数据流，第1天定稿）

## 1. 模块划分

```
kvstore/
├── src/
│   ├── main.rs          # 入口：按参数启动 server / client / --version
│   ├── protocol/        # 命令与消息格式定义（契约，全组共享）
│   ├── parser.rs        # 用户输入 → ParsedCommand（成员C）
│   ├── store/           # 内存键值存储 KVStore（成员B）
│   ├── persistence.rs   # 追加日志与启动恢复（成员B）
│   ├── server.rs        # 服务器：主循环 + TCP监听 + 连接线程（成员A）
│   └── client.rs        # 命令行客户端：REPL + TCP连接（成员C）
├── tests/
│   └── integration.rs   # 集成测试（成员D）
├── data/                # 数据文件目录（运行时生成，不入Git）
├── PROTOCOL.md          # 协议契约（第1天定稿）
└── DESIGN.md            # 本文件
```

## 2. 数据流

### 第2天（无网络，全链路在服务器进程内）：
```
用户输入 → parser.parse() → KVStore.set/get/delete/list/len → 打印结果
```

### 第3天起（网络模式）：
```
客户端: 用户输入 → parser.parse() → TcpStream 发送请求行
服务器: TcpListener accept → 逐行读取 → parser.parse() → KVStore 操作
       → Persistence.append()（写日志）→ 写回响应行
```

### 数据持久化（第3天）：
```
写操作成功判定顺序: 写日志文件 → 更新内存 → 返回 OK
启动恢复: Persistence.recover() 逐行重放 → KVStore
```

## 3. 调用关系（谁依赖谁）

| 模块 | 依赖 | 说明 |
|---|---|---|
| parser | protocol | 用 Command::from_str / required_args 校验 |
| store | 无（std） | 独立，不依赖任何业务模块 |
| persistence | store | 重放日志写入 KVStore |
| server | protocol, parser, store, persistence | 组装全链路 |
| client | protocol, parser | 本地校验后发送 |

## 4. 关键接口（第1天定稿）

| 接口 | 定义位置 | 签名 |
|---|---|---|
| 命令解析 | `parser::parse` | `fn parse(line: &str) -> Result<ParsedCommand, ParseError>` |
| 存储 | `store::KVStore` | `set/get/delete/list/len` |
| 日志记录 | `persistence::LogRecord` | `to_line / from_line` |
| 持久化 | `persistence::Persistence` | `append / recover` |
| 服务器 | `server::run` | `fn run()` |
| 客户端 | `client::run` | `fn run()` |

## 5. 各模块里程碑（对照工作计划）

| 模块 | 第1天 | 第2天 | 第3天 | 第4天 |
|---|---|---|---|---|
| protocol | ✅ 定稿 | 不变 | 不变 | 不变 |
| parser | 骨架 | ✅ 完整解析 | 不变 | 边界完善 |
| store | 骨架 | ✅ 完整CRUD | 不变 | 并发安全 |
| persistence | 骨架 | 接口 | ✅ 日志+恢复 | 并发验证 |
| server | 骨架 | stdin主循环 | TCP单连接 | 并发+参数化 |
| client | 骨架 | REPL | TCP连接 | 异常完善 |
| tests | 骨架 | 单元测试 | 网络测试 | 并发测试全覆盖 |
