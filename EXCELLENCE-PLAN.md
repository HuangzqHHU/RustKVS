# 冲刺优秀——扩展功能工作安排（四人并行）

> 目标：补齐创新扩展分（15%）+ 增强质量与展示分
> 四项任务：**① TTL 过期时间（核心加分）② STATUS 增强 ③ 压力测试 ④ DEMO 文档**
> 组长：成员A | 时间：第4天（半天冲刺）+ 答辩前微调

## 〇、接口契约 v2（全组必须先读，防止接口冲突！）

以下接口变更由各组员在**自己负责的文件**内实现，**禁止改动他人文件**：

1. **SET 命令格式**：`SET key value [ttl]`
   - 可选第三参数 ttl：正整数秒，缺省 = 永不过期
   - 例：`SET course Rust 5` → 5 秒后该键视为不存在
2. **`ParsedCommand` 新增字段**：`ttl: Option<u64>`（仅 SET 有意义，其他命令为 None）
3. **`KVStore::set` 签名变更**：`set(&mut self, key: &str, value: &str, ttl: Option<u64>)`
4. **STATUS 输出格式**：`STATUS count=N connections=M uptime=Ss commands=C`
5. **过期键行为**（惰性删除）：
   - GET / DEL 过期键 → 视为"键不存在"
   - LIST / STATUS 统计 → 不含过期键（访问时检查并清理）
   - 覆盖写时新 ttl 生效；不带 ttl 的覆盖写 = 改为永不过期
6. **ttl 校验**：非数字 / 负数 / 0 / 超长（> 86400*365）→ 解析错误（ERROR），不静默忽略

## 一、四人任务分配

| 成员 | 负责文件（只能改这些） | 任务 | 交付/验收点 |
|---|---|---|---|
| **A（组长）** | `src/protocol/mod.rs`、`src/server.rs`、`DEMO.md` | ① 协议文档更新（SET ttl 语义、STATUS 新格式）② server 适配：execute 读取 `parsed.ttl` 传给 store；STATUS 增强（连接数、运行时长、命令计数）；execute 测试适配新字段 ③ 集成负责人 ④ 写 DEMO.md | 编译通过；端到端 TTL 演示（SET course Rust 3 → 3秒后 GET 返回键不存在）；STATUS 显示 connections/uptime/commands |
| **B** | `src/store/mod.rs` | store 数据结构改造：`HashMap<String, (String, Option<Instant>)>`；set 签名加 ttl；get/list 惰性过期检查；单元测试 | store 单元测试全绿（过期 GET 返回 None、未过期正常、覆盖更新 ttl、无 ttl 永久） |
| **C** | `src/parser.rs` | parser 支持 SET 第三参数：解析 ttl 为 Option\<u64\>；ParsedCommand 加 ttl 字段；ttl 校验（非数字/0/负数/超长 → 明确错误）；单元测试 | parser 单元测试全绿（含 ttl 合法/非法用例） |
| **D** | `tests/`（新增 stress_tests.rs + 更新 network_tests.rs） | ① 压力测试：100 并发连接各 10 次操作全部成功 ② TTL 集成测试（过期/未过期/覆盖）③ DEMO.md 校对 | 新增测试全绿；压力测试 1000 次操作无丢失无崩溃 |

## 二、开发顺序与集成节点（防冲突关键）

```
09:00 站会：A 宣读接口契约 v2（本文件第〇节），全员确认无异议
09:15-12:00 四人并行开发（各改各的文件）
  B: store 结构改造（最先动，其他模块依赖它）
  C: parser ttl 解析（依赖契约，不依赖 B）
  A: protocol 文档 + server 适配（等 B 的 set 签名）
  D: 先写压力测试（不依赖 TTL），TTL 集成测试等接口定稿
12:00-14:00 午休（B/C 可先提交各自分支）
14:00-16:00 下午并行 + A 集成准备（cargo build 试编译，记录错误清单）
16:00-16:30 集成节点（全员）：
  git pull → cargo build → 逐个解决接口不匹配 → cargo test
16:30-17:00 端到端验收：
  1) TTL 演示：SET course Rust 3 → 立即 GET 有值 → 等 4 秒 → GET 返回"键不存在"
  2) STATUS 演示：显示 count/connections/uptime/commands
  3) 压力测试：cargo test --test stress_tests
  4) 重启恢复复核：TTL 键重启后仍按剩余时间过期（可选，加分项）
17:00 提交：git tag day4-extension → git push origin main --tags
```

## 三、接口适配责任表（谁改了谁适配）

| 接口变更 | 改的人 | 受影响方（需同步适配） |
|---|---|---|
| `set()` 签名 + ttl | B | A（server execute 调用处 + 测试） |
| `ParsedCommand.ttl` 字段 | C | A（server execute 读取 + 测试）、D（构造 ParsedCommand 的测试） |
| STATUS 输出格式 | A | D（network_tests 中断言 STATUS 的用例）、C（client 无需改，透传） |

**规则**：改接口的人负责在群里@受影响方；集成节点统一验证。

## 四、风险与应对

| 风险 | 应对 |
|---|---|
| 接口不匹配导致编译失败（前两次的教训） | 契约先定死（第〇节）；集成节点 16:00 提前试编译；只改自己文件 |
| store 结构改造影响 server | B 上午最先完成 set 签名，A 下午适配；B 提交后 A 立即 build |
| TTL 时间精度（测试等待几秒变慢） | 单元测试用毫秒级/直接构造过期 Instant；集成测试 ttl=1 秒 |
| 压力测试拖慢整体 | 用线程池/减少到 50 并发；确保不写同一日志文件（各连接独立操作不同键） |

## 五、答辩加分点（做完这些可以讲）

1. TTL 是 PPT 明示扩展项 → 现场演示"键 5 秒后消失"
2. 锁最小化 + 惰性过期 → 讲并发安全设计
3. 压力测试 1000 次操作 → 展示稳定性数据
4. STATUS 实时状态 → 展示工程化细节
5. DEMO.md 演示脚本 → 展示流程规范
