//! 集成测试（成员D负责）
//!
//! 第3天：持久化恢复测试 + 后续网络测试
//!   - Persistence 重启恢复全场景测试
//!   - 日志文件异常场景测试
//!   - 模拟"服务器关闭 → 重启 → 数据完整"的真实流程
//!
//! 说明：B 已经在 persistence 模块内写了单元测试。
//! 这里写的是集成视角的测试——把 Persistence 和 KVStore 合起来测，
//! 模拟真实的"写入→关服→重启→查询"生命周期。

use kvstore::persistence::{LogRecord, Persistence};
use kvstore::store::KVStore;

use std::path::{Path, PathBuf};

// ------------------------------------------------------------
// 辅助函数：生成隔离的临时日志路径
// ------------------------------------------------------------

fn temp_log_path(name: &str) -> PathBuf {
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fname = format!("kvstore_test_{}_{}_{}.log", pid, ts, name);
    std::env::temp_dir().join(fname)
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
}

// ============================================================
// 一、基础恢复：写入 → 关服 → 重启 → 查到
// ============================================================

/// 写入1条 → 重启 → 能查到
#[test]
fn recover_single_set() {
    let path = temp_log_path("single_set");
    let p = Persistence::new(&path);

    // 第一次"启动"：写入数据
    {
        let mut store = KVStore::new();
        p.append(&LogRecord::Set {
            key: "name".into(),
            value: "Alice".into(),
        })
        .unwrap();
        store.set("name", "Alice", None).unwrap();
        assert_eq!(store.get("name"), Ok(Some("Alice")));
    } // store 销毁，模拟服务器关闭

    // 第二次"启动"：从日志恢复
    {
        let mut store = KVStore::new();
        p.recover(&mut store).unwrap();
        assert_eq!(store.get("name"), Ok(Some("Alice")));
        assert_eq!(store.len(), 1);
    }

    cleanup(&path);
}

/// 写入多条 → 重启 → 都能查到，顺序不影响
#[test]
fn recover_multiple_sets() {
    let path = temp_log_path("multi_set");
    let p = Persistence::new(&path);

    // 模拟服务器运行中：写入多条记录
    p.append(&LogRecord::Set {
        key: "a".into(),
        value: "1".into(),
    })
    .unwrap();
    p.append(&LogRecord::Set {
        key: "b".into(),
        value: "2".into(),
    })
    .unwrap();
    p.append(&LogRecord::Set {
        key: "c".into(),
        value: "3".into(),
    })
    .unwrap();

    // 重启恢复
    let mut store = KVStore::new();
    p.recover(&mut store).unwrap();

    assert_eq!(store.len(), 3);
    assert_eq!(store.get("a"), Ok(Some("1")));
    assert_eq!(store.get("b"), Ok(Some("2")));
    assert_eq!(store.get("c"), Ok(Some("3")));

    let keys = store.list();
    assert_eq!(
        keys,
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );

    cleanup(&path);
}

/// 覆盖写 → 重启 → 查到的是最新值
#[test]
fn recover_overwrite() {
    let path = temp_log_path("overwrite");
    let p = Persistence::new(&path);

    // 先写 v1，再覆盖为 v2
    p.append(&LogRecord::Set {
        key: "k".into(),
        value: "v1".into(),
    })
    .unwrap();
    p.append(&LogRecord::Set {
        key: "k".into(),
        value: "v2".into(),
    })
    .unwrap();

    let mut store = KVStore::new();
    p.recover(&mut store).unwrap();

    assert_eq!(store.get("k"), Ok(Some("v2")));
    assert_eq!(store.len(), 1); // 数量还是1

    cleanup(&path);
}

/// 删除 → 重启 → 确认已删除
#[test]
fn recover_delete() {
    let path = temp_log_path("delete");
    let p = Persistence::new(&path);

    p.append(&LogRecord::Set {
        key: "k".into(),
        value: "v".into(),
    })
    .unwrap();
    p.append(&LogRecord::Del { key: "k".into() }).unwrap();

    let mut store = KVStore::new();
    p.recover(&mut store).unwrap();

    assert_eq!(store.get("k"), Ok(None));
    assert!(store.is_empty());

    cleanup(&path);
}

/// 删了又加 → 重启 → 数据还在
#[test]
fn recover_delete_then_set_again() {
    let path = temp_log_path("del_then_set");
    let p = Persistence::new(&path);

    // SET → DEL → SET（同一个key）
    p.append(&LogRecord::Set {
        key: "k".into(),
        value: "v1".into(),
    })
    .unwrap();
    p.append(&LogRecord::Del { key: "k".into() }).unwrap();
    p.append(&LogRecord::Set {
        key: "k".into(),
        value: "v2".into(),
    })
    .unwrap();

    let mut store = KVStore::new();
    p.recover(&mut store).unwrap();

    assert_eq!(store.get("k"), Ok(Some("v2")));
    assert_eq!(store.len(), 1);

    cleanup(&path);
}

// ============================================================
// 二、边界场景
// ============================================================

/// 文件不存在 → 正常启动（空库），不报错
#[test]
fn recover_no_file_starts_empty() {
    let path = temp_log_path("no_file");
    cleanup(&path); // 确保不存在

    let p = Persistence::new(&path);
    let mut store = KVStore::new();
    p.recover(&mut store).unwrap();

    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

/// 空文件 → 根据B的实现：空行会报错
/// （B 的 recover 把空行视为损坏，返回 Err）
#[test]
fn recover_empty_file_errors() {
    let path = temp_log_path("empty_file");
    std::fs::write(&path, "").unwrap();

    let p = Persistence::new(&path);
    let mut store = KVStore::new();
    let result = p.recover(&mut store);

    // 空文件在 B 的实现中会返回 Ok（因为 lines() 不产生任何迭代项）
    // 如果是空行（如 "\n"），才会报错。这里验证行为一致。
    // 空文件 → 没有任何行 → 不进入循环 → 返回 Ok
    assert!(result.is_ok());
    assert!(store.is_empty());

    cleanup(&path);
}

/// 值包含空格 → 重启后完整保留
#[test]
fn recover_value_with_spaces() {
    let path = temp_log_path("value_spaces");
    let p = Persistence::new(&path);

    p.append(&LogRecord::Set {
        key: "msg".into(),
        value: "hello world from kvstore".into(),
    })
    .unwrap();

    let mut store = KVStore::new();
    p.recover(&mut store).unwrap();

    assert_eq!(store.get("msg"), Ok(Some("hello world from kvstore")));

    cleanup(&path);
}

/// 值包含中文 → 重启后正确恢复
#[test]
fn recover_value_with_chinese() {
    let path = temp_log_path("value_chinese");
    let p = Persistence::new(&path);

    p.append(&LogRecord::Set {
        key: "greeting".into(),
        value: "你好，世界".into(),
    })
    .unwrap();

    let mut store = KVStore::new();
    p.recover(&mut store).unwrap();

    assert_eq!(store.get("greeting"), Ok(Some("你好，世界")));

    cleanup(&path);
}

/// 大量数据恢复（100条）→ 全部正确
#[test]
fn recover_many_records() {
    let path = temp_log_path("many_records");
    let p = Persistence::new(&path);
    let n = 100;

    for i in 0..n {
        p.append(&LogRecord::Set {
            key: format!("key{}", i),
            value: format!("value{}", i),
        })
        .unwrap();
    }

    let mut store = KVStore::new();
    p.recover(&mut store).unwrap();

    assert_eq!(store.len(), n);
    for i in 0..n {
        let key = format!("key{}", i);
        let expected = format!("value{}", i);
        assert_eq!(store.get(&key), Ok(Some(expected.as_str())));
    }

    cleanup(&path);
}

// ============================================================
// 三、异常文件：损坏、截断、格式错误
// ============================================================

/// 文件中出现无法识别的行 → 明确报错，不静默忽略
#[test]
fn recover_corrupt_line_returns_error() {
    let path = temp_log_path("corrupt_line");
    std::fs::write(&path, "SET a 1\nGARBAGE DATA\nSET b 2\n").unwrap();

    let p = Persistence::new(&path);
    let mut store = KVStore::new();
    let result = p.recover(&mut store);

    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("格式错误") || err_msg.contains("第 2 行"));

    cleanup(&path);
}

/// SET 缺 value（截断行）→ 报错
#[test]
fn recover_truncated_set_returns_error() {
    let path = temp_log_path("truncated_set");
    std::fs::write(&path, "SET k\n").unwrap();

    let p = Persistence::new(&path);
    let mut store = KVStore::new();
    let result = p.recover(&mut store);

    assert!(result.is_err());

    cleanup(&path);
}

/// DEL 缺 key → 报错
#[test]
fn recover_truncated_del_returns_error() {
    let path = temp_log_path("truncated_del");
    std::fs::write(&path, "DEL\n").unwrap();

    let p = Persistence::new(&path);
    let mut store = KVStore::new();
    let result = p.recover(&mut store);

    assert!(result.is_err());

    cleanup(&path);
}

/// 未知命令 → 报错
#[test]
fn recover_unknown_command_returns_error() {
    let path = temp_log_path("unknown_cmd");
    std::fs::write(&path, "FOOBAR k v\n").unwrap();

    let p = Persistence::new(&path);
    let mut store = KVStore::new();
    let result = p.recover(&mut store);

    assert!(result.is_err());

    cleanup(&path);
}

/// 损坏文件时，不静默清空已有数据（恢复失败但不删除文件）
#[test]
fn recover_corrupt_does_not_delete_file() {
    let path = temp_log_path("corrupt_no_delete");
    let bad_content = "SET a 1\nBADLINE\n";
    std::fs::write(&path, bad_content).unwrap();

    let p = Persistence::new(&path);
    let mut store = KVStore::new();
    let _ = p.recover(&mut store); // 恢复失败

    // 文件应该还在，内容没变（不被静默清空）
    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content, bad_content);

    cleanup(&path);
}

// ============================================================
// 四、完整生命周期模拟
// ============================================================

/// 完整模拟：启动（空）→ 写入多条 → 关服 → 重启 → 验证 → 再写入 → 再重启 → 验证累加
#[test]
fn full_lifecycle_two_restarts() {
    let path = temp_log_path("full_lifecycle");
    cleanup(&path);
    let p = Persistence::new(&path);

    // ===== 第1次启动：空库，写入数据 =====
    {
        let mut store = KVStore::new();
        p.recover(&mut store).unwrap(); // 空恢复
        assert!(store.is_empty());

        p.append(&LogRecord::Set {
            key: "a".into(),
            value: "1".into(),
        })
        .unwrap();
        store.set("a", "1", None).unwrap();
        p.append(&LogRecord::Set {
            key: "b".into(),
            value: "2".into(),
        })
        .unwrap();
        store.set("b", "2", None).unwrap();
    } // 关服

    // ===== 第2次启动：恢复并继续写入 =====
    {
        let mut store = KVStore::new();
        p.recover(&mut store).unwrap();
        assert_eq!(store.len(), 2);
        assert_eq!(store.get("a"), Ok(Some("1")));
        assert_eq!(store.get("b"), Ok(Some("2")));

        // 继续操作：修改 a、删除 b、新增 c
        p.append(&LogRecord::Set {
            key: "a".into(),
            value: "999".into(),
        })
        .unwrap();
        store.set("a", "999", None).unwrap();
        p.append(&LogRecord::Del { key: "b".into() }).unwrap();
        store.delete("b").unwrap();
        p.append(&LogRecord::Set {
            key: "c".into(),
            value: "3".into(),
        })
        .unwrap();
        store.set("c", "3", None).unwrap();
    } // 再次关服

    // ===== 第3次启动：验证最终状态 =====
    {
        let mut store = KVStore::new();
        p.recover(&mut store).unwrap();

        assert_eq!(store.len(), 2);
        assert_eq!(store.get("a"), Ok(Some("999"))); // 覆盖后的值
        assert_eq!(store.get("b"), Ok(None)); // 已删除
        assert_eq!(store.get("c"), Ok(Some("3"))); // 第2次新增
        assert_eq!(store.list(), vec!["a".to_string(), "c".to_string()]);
    }

    cleanup(&path);
}

/// 日志大小写不敏感：set/SET 都能正确重放
#[test]
fn recover_case_insensitive() {
    let path = temp_log_path("case_insensitive");
    // 混合大小写写入
    std::fs::write(&path, "set a 1\nDEL b\nSet c 3\n").unwrap();

    let p = Persistence::new(&path);
    let mut store = KVStore::new();
    p.recover(&mut store).unwrap();

    assert_eq!(store.get("a"), Ok(Some("1")));
    assert_eq!(store.get("b"), Ok(None)); // DEL b 删掉了（本来就没有，返回None）
    assert_eq!(store.get("c"), Ok(Some("3")));

    cleanup(&path);
}

// ============================================================
// 五、append 的独立验证
// ============================================================

/// 多次 append 后，文件内容行数正确
#[test]
fn append_writes_correct_lines() {
    let path = temp_log_path("append_lines");
    cleanup(&path);
    let p = Persistence::new(&path);

    p.append(&LogRecord::Set {
        key: "a".into(),
        value: "1".into(),
    })
    .unwrap();
    p.append(&LogRecord::Set {
        key: "b".into(),
        value: "2".into(),
    })
    .unwrap();
    p.append(&LogRecord::Del { key: "a".into() }).unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = content.lines().collect();

    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "SET a 1");
    assert_eq!(lines[1], "SET b 2");
    assert_eq!(lines[2], "DEL a");

    cleanup(&path);
}

/// append 后立即 flush，数据真的落盘了
#[test]
fn append_flushes_to_disk() {
    let path = temp_log_path("flush_check");
    cleanup(&path);
    let p = Persistence::new(&path);

    p.append(&LogRecord::Set {
        key: "k".into(),
        value: "v".into(),
    })
    .unwrap();

    // 不用关 Persistence，直接读文件——如果没 flush 就会是空的
    let metadata = std::fs::metadata(&path).unwrap();
    assert!(metadata.len() > 0, "append 后文件应该有内容");

    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("SET k v"));

    cleanup(&path);
}
