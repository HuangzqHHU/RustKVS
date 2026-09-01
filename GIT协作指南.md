# Git 协作指南（全组必读）

> 组长：成员A | 仓库：https://github.com/HuangzqHHU/RustKVS.git
> 规则：main 分支永远保持可编译可运行；每天 16:30 集成节点前必须推送完毕。

## 一、首次使用（每人一次）

```powershell
git clone https://github.com/HuangzqHHU/RustKVS.git
cd RustKVS
cargo build          # 确认本地能编译
```

> 推送认证：push 时若弹出浏览器窗口，用**你自己的 GitHub 账号**登录授权（只弹一次）。

## 二、每天的工作循环（最重要）

```powershell
git pull origin main                    # 1. 先拉最新代码（避免冲突）
# 2. 只修改你自己负责的文件
git add <你改的文件>                     # 3. 只添加自己改的文件（不要 git add -A 全加）
git commit -m "feat: 说明你做了什么"      # 4. 提交
git push origin main                    # 5. 推送
```

提交信息规范（前3个字母+冒号+空格+中文说明）：
- `feat: ` 新功能（如 `feat: 完成内存存储CRUD`）
- `fix: `  修复问题（如 `fix: 修复删除不存在键时崩溃`）
- `test: ` 测试（如 `test: 添加重启恢复测试`）
- `docs: ` 文档（如 `docs: 更新协议说明`）

## 三、推送被拒绝怎么办

提示 `non-fast-forward` / `failed to push some refs` = 别人先推了，执行：

```powershell
git pull --rebase origin main
git push origin main
```

## 四、各成员负责的文件（铁律：只动自己的）

| 成员 | 只能修改这些文件 |
|---|---|
| 成员A（组长） | `src/main.rs`、`src/server.rs` |
| 成员B | `src/store/mod.rs`、`src/persistence.rs` |
| 成员C | `src/parser.rs`、`src/client.rs` |
| 成员D | `tests/integration.rs` |

绝对不要改别人的文件；确需修改时，先在站会提出，由组长协调。

## 五、组长（成员A）集成节点流程（每天 16:30）

```powershell
git pull origin main          # 拉取所有组员的代码
cargo build                   # 编译，有错当场一起解决
cargo run -- --version        # 运行验证
cargo test                    # 测试必须全绿
git tag day<N>                # 当天通过后打标签（day1, day2, ...）
git push origin main --tags   # 推送代码和标签
```

## 六、常见错误速查

| 报错 | 原因 | 解决 |
|---|---|---|
| `failed to push some refs` | 别人先推了 | `git pull --rebase origin main` 再 push |
| `Invalid username or token` | 用户名/令牌错误或缓存了旧凭据 | 用浏览器登录方式重新认证 |
| `nothing to commit` | 没有改动或没 add | 先改文件再 add |
| 文件冲突（`<<<<<<<` 标记） | 两人改了同一文件 | 不要乱删标记，找组长一起解决 |
