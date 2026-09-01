# 第2天验收记录（ACCEPTANCE day2）

> 验收人（组长）：成员A | 日期：第2天 16:30 集成节点
> 验收标准：对照工作计划"阶段2、3完成标准"——合法与非法输入均得预期结果；内存键值操作完整。

## 一、自动化验收

```
cargo test  →  39（库单元测试）+ 24（集成测试）= 63 个全部通过 ✅
```

覆盖：store CRUD（B）、parser 解析与校验（C）、server 执行逻辑（A）、集成测试（D）。

## 二、手动功能验收（无网络模式：cargo run -- server）

| # | 输入 | 预期 | 实际 | 结果 |
|---|---|---|---|---|
| 1 | `SET name Alice` | OK | OK | ✅ |
| 2 | `GET name` | VALUE name Alice | VALUE name Alice | ✅ |
| 3 | `SET name Bob`（覆盖写） | OK | OK | ✅ |
| 4 | `GET name` | VALUE name Bob（新值） | VALUE name Bob | ✅ |
| 5 | `GET missing` | ERROR 键不存在 | ERROR 键不存在 | ✅ |
| 6 | `DEL name` | OK | OK | ✅ |
| 7 | `DEL name`（重复删除） | ERROR 键不存在 | ERROR 键不存在 | ✅ |
| 8 | `SET a 1` | OK | OK | ✅ |
| 9 | `SET b 2` | OK | OK | ✅ |
| 10 | `LIST` | KEYS a b（有序） | KEYS a b | ✅ |
| 11 | `STATUS` | STATUS count=2 | STATUS count=2 | ✅ |
| 12 | `FOO x`（未知命令） | ERROR 未知命令 | ERROR 未知命令：FOO | ✅ |
| 13 | `SET onlykey`（缺参数） | ERROR 缺少参数 | ERROR 缺少参数：SET 需要 value | ✅ |
| 14 | `GET a extra`（多参数） | ERROR 多余参数 | ERROR 多余参数：GET 只能有一个 key | ✅ |
| 15 | `set x 3`（小写） | OK | OK | ✅ |
| 16 | `Get x`（混合大小写） | VALUE x 3 | VALUE x 3 | ✅ |
| 17 | `SET sp hello world`（值含空格） | OK | OK | ✅ |
| 18 | `GET sp` | VALUE sp hello world | VALUE sp hello world | ✅ |
| 19 | 1108字节超长命令 | ERROR 消息超长 | ERROR 消息超长（经单行/文件方式验证） | ✅ |
| 20 | `EXIT` | 退出 | 服务器已退出 | ✅ |

## 三、容错性验证

- 每条错误后均能继续输入下一条命令（单条命令出错不影响后续）✅
- 空行忽略，不报错 ✅
- EOF（Ctrl+Z 回车）正常退出 ✅

## 四、客户端模式验证（cargo run -- client）

```
kv> SET name Alice   → OK
kv> GET name         → VALUE name Alice
kv> PING             → PONG
kv> EXIT             → BYE（退出）
```

## 五、验收结论

**阶段2（命令与通信规则）+ 阶段3（内存键值存储）完成标准全部满足**：无网络即可完成完整增删改查，合法与非法输入均得到预期结果，单条错误不中断。

验收人签字：________（成员A）  组员确认：B____ C____ D____

## 备注

- 接口变更记录：parser 接口第2天由成员C调整（`parse`→`parse_command`，字段 `{cmd,args}`→`{command,key,value}`），组长确认并适配 server/main/集成测试，详见 DESIGN.md。
- 超长消息在第3天 TCP 模式下由服务器端 `MAX_MSG_LEN` 校验（stdin 管道测试时 PowerShell 会拆分长行，属测试环境现象，程序逻辑正确）。
