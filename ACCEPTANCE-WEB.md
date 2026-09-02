# Web 管理界面验收记录（ACCEPTANCE-WEB）

> 验收人（组长）：成员A | 参与：C（页面实现）、B、D
> 目标：PPT 明示扩展项"Web 管理"（读写版）验收

## 一、自动化验收

```
cargo build     → 零警告 ✅
cargo test      → 175 个测试全绿（lib 68 / 并发 8 / 集成 46 / 网络 20 / 持久化 19 / 其他 14）✅
```

## 二、功能验收（curl 模拟浏览器 + TCP 客户端联动）

| # | 验收项 | 操作 | 结果 |
|---|---|---|---|
| 1 | 主页显示 | `GET /` | ✅ 完整 HTML（标题/数据表/命令表单） |
| 2 | 网页执行 SET | `POST /cmd` body: `command=SET perm forever` | ✅ 绿色 OK |
| 3 | 数据实时显示 | `GET /` 表格 | ✅ 出现 `<td>perm</td>` |
| 4 | TTL 可视化 | `POST /cmd` `SET course Rust 5` → 6 秒后 `GET /` | ✅ course 从表格消失（过期） |
| 5 | 单键查询 | `GET /get?key=perm` | ✅ VALUE perm forever |
| 6 | 过期键查询 | `GET /get?key=course`（过期后） | ✅ ERROR 键不存在 |
| 7 | 错误提示 | `POST /cmd` `GET missing` | ✅ 红色 ERROR 键不存在 |
| 8 | **EXIT 拒绝** | `POST /cmd` `EXIT` | ✅ 提示"不允许在网页上执行"，服务器不退出 |
| 9 | 404 | `GET /notexist` | ✅ HTTP 404 |
| 10 | **Web↔TCP 互通** | 网页 SET → TCP 客户端 `GET` | ✅ 能查到（共享同一 Server） |
| 11 | **重启恢复** | 网页写入 perm → 重启服务器 → `GET /` | ✅ perm 恢复 |
| 12 | 中文 | 中文键/值显示 | ✅ 页面 charset=utf-8 正常 |

## 三、设计决策记录（答辩可能被问）

1. **TTL 键重启后转为永久**：日志格式 `SET key value` 不记录 ttl，recover 重放后 ttl=None。
   影响：重启后原 TTL 键不再过期。理由：日志格式保持与协议命令兼容、实现简单；
   如需"重启后仍过期"，需扩展日志格式记录 ttl（列为后续可选改进）。
2. **网页操作必须走 `Server::execute`**：保证"先写日志再更新内存"的持久化语义与 TCP 客户端完全一致。
3. **EXIT 拦截机制**：`execute` 对 EXIT 返回 None（表示退出），Web 层将其转换为错误提示，服务器不退出。
4. **XSS 防护**：页面所有用户数据（键/值/错误）均经 `html_escape`。
5. **零第三方依赖**：HTTP 协议层用 `std::net` 手写（GET + POST 表单，Connection: close）。

## 四、使用方式

```powershell
cargo run -- server                 # 启动（自动带 Web 端口 8080）
# 浏览器打开 http://127.0.0.1:8080
# 自定义：cargo run -- server --web-port 9000（0 = 关闭 Web）
```

## 五、验收结论

**Web 管理（读写版）验收通过**：主页数据可视化、网页读写命令、TTL 实时过期、
Web↔TCP 双向互通、EXIT 防护、重启恢复全部符合 WEB-PLAN.md 验收标准。
可作为课程设计创新扩展项（15%）答辩展示。

验收人签字：________（成员A）  组员确认：B____ C____ D____
