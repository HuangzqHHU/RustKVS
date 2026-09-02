//! 集成测试（成员D负责）
//!
//! 第2天：无网络模式下的端到端测试
//!   - KVStore 多步骤复杂场景（模拟完整用户操作流）
//!   - 边界与异常场景全覆盖
//!   - parser 集成测试（成员C已交付，启用）
//!   - parser + KVStore 全链路端到端测试
//!
//! 说明：B 已经在 store 模块内写了单元测试（单方法级），
//!       C 已经在 parser 模块内写了单元测试。
//! 这里写的是集成视角的测试——跨方法、跨模块、多步骤、模拟真实使用流程。

use kvstore::parser::parse_command;
use kvstore::protocol::Command;
use kvstore::store::{KVStore, StoreError};

// ============================================================
// 一、端到端操作流测试（模拟真实用户连续操作）
// ============================================================

/// 完整操作流：SET → GET → SET（覆盖）→ GET → DEL → GET → LIST → STATUS
#[test]
fn e2e_full_command_flow() {
    let mut store = KVStore::new();

    // 初始状态：空
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);

    // SET name Alice
    store.set("name", "Alice", None).unwrap();
    assert_eq!(store.get("name"), Ok(Some("Alice")));
    assert_eq!(store.len(), 1);

    // SET age 20
    store.set("age", "20", None).unwrap();
    assert_eq!(store.get("age"), Ok(Some("20")));
    assert_eq!(store.len(), 2);

    // SET name Bob（覆盖写）
    store.set("name", "Bob", None).unwrap();
    assert_eq!(store.get("name"), Ok(Some("Bob")));
    assert_eq!(store.len(), 2); // 数量不变

    // DEL age
    let removed = store.delete("age").unwrap();
    assert!(removed);
    assert_eq!(store.get("age"), Ok(None));
    assert_eq!(store.len(), 1);

    // DEL age 再删一次 → 返回 false
    let removed = store.delete("age").unwrap();
    assert!(!removed);

    // LIST → 只剩 name
    let keys = store.list();
    assert_eq!(keys, vec!["name".to_string()]);

    // STATUS（len）
    assert_eq!(store.len(), 1);
}

/// 写入大量数据后验证正确性（压力小测）
#[test]
fn e2e_many_keys_insert_and_list() {
    let mut store = KVStore::new();
    let n = 100;

    for i in 0..n {
        store
            .set(&format!("key{}", i), &format!("value{}", i), None)
            .unwrap();
    }
    assert_eq!(store.len(), n);

    // 逐条验证都能查到
    for i in 0..n {
        let key = format!("key{}", i);
        let expected = format!("value{}", i);
        assert_eq!(store.get(&key), Ok(Some(expected.as_str())));
    }

    // list 排序正确
    let keys = store.list();
    assert_eq!(keys.len(), n);
    assert_eq!(keys.first().unwrap(), "key0");
    assert_eq!(keys.last().unwrap(), "key99");
}

/// 删除全部键后回到空状态
#[test]
fn e2e_delete_all_back_to_empty() {
    let mut store = KVStore::new();
    store.set("a", "1", None).unwrap();
    store.set("b", "2", None).unwrap();
    store.set("c", "3", None).unwrap();
    assert_eq!(store.len(), 3);

    store.delete("a").unwrap();
    store.delete("b").unwrap();
    store.delete("c").unwrap();

    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
    assert!(store.list().is_empty());
}

// ============================================================
// 二、值的边界测试
// ============================================================

/// 空值可以正常存取
#[test]
fn value_empty_string() {
    let mut store = KVStore::new();
    store.set("k", "", None).unwrap();
    assert_eq!(store.get("k"), Ok(Some("")));
}

/// 值可以包含空格（协议规定值允许空格）
#[test]
fn value_with_spaces() {
    let mut store = KVStore::new();
    store.set("greeting", "hello world", None).unwrap();
    assert_eq!(store.get("greeting"), Ok(Some("hello world")));
}

/// 值可以包含特殊字符
#[test]
fn value_with_special_chars() {
    let mut store = KVStore::new();
    let val = "!@#$%^&*()_+-=[]{}|;:'\",.<>?/`~";
    store.set("special", val, None).unwrap();
    assert_eq!(store.get("special"), Ok(Some(val)));
}

/// 长值正常存取
#[test]
fn value_very_long() {
    let mut store = KVStore::new();
    let long_val = "a".repeat(10000);
    store.set("long", &long_val, None).unwrap();
    let result = store.get("long").unwrap().unwrap();
    assert_eq!(result.len(), 10000);
    assert_eq!(result, long_val);
}

/// 值可以包含中文
#[test]
fn value_with_chinese() {
    let mut store = KVStore::new();
    store.set("msg", "你好，世界", None).unwrap();
    assert_eq!(store.get("msg"), Ok(Some("你好，世界")));
}

// ============================================================
// 三、非法键测试（覆盖各种非法场景）
// ============================================================

/// 空键 → SET/GET/DEL 都应报错 InvalidKey
#[test]
fn invalid_key_empty() {
    let mut store = KVStore::new();

    let set_err = store.set("", "v", None).unwrap_err();
    assert!(matches!(set_err, StoreError::InvalidKey(_)));

    let get_err = store.get("").unwrap_err();
    assert!(matches!(get_err, StoreError::InvalidKey(_)));

    let del_err = store.delete("").unwrap_err();
    assert!(matches!(del_err, StoreError::InvalidKey(_)));
}

/// 含空格的键
#[test]
fn invalid_key_with_space() {
    let mut store = KVStore::new();
    assert!(store.set("hello world", "v", None).is_err());
    assert!(store.get("hello world").is_err());
    assert!(store.delete("hello world").is_err());
}

/// 含制表符的键
#[test]
fn invalid_key_with_tab() {
    let mut store = KVStore::new();
    assert!(store.set("a\tb", "v", None).is_err());
}

/// 含换行的键
#[test]
fn invalid_key_with_newline() {
    let mut store = KVStore::new();
    assert!(store.set("a\nb", "v", None).is_err());
}

/// 含回车的键
#[test]
fn invalid_key_with_carriage_return() {
    let mut store = KVStore::new();
    assert!(store.set("a\rb", "v", None).is_err());
}

/// 非法键不会写入数据
#[test]
fn invalid_key_does_not_pollute_store() {
    let mut store = KVStore::new();
    let _ = store.set("", "v", None);
    let _ = store.set("a b", "v", None);
    let _ = store.set("a\nb", "v", None);

    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

// ============================================================
// 四、查询与列表
// ============================================================

/// 不存在的键 → get 返回 Ok(None)，不报错
#[test]
fn get_nonexistent_returns_none() {
    let store = KVStore::new();
    assert_eq!(store.get("nothing"), Ok(None));
}

/// list 在空存储时返回空向量
#[test]
fn list_empty_store() {
    let store = KVStore::new();
    assert!(store.list().is_empty());
    assert_eq!(store.list().len(), 0);
}

/// list 按字典序排序
#[test]
fn list_sorted_lexicographic() {
    let mut store = KVStore::new();
    store.set("z", "1", None).unwrap();
    store.set("apple", "2", None).unwrap();
    store.set("banana", "3", None).unwrap();
    store.set("Zoo", "4", None).unwrap();

    let keys = store.list();
    assert_eq!(
        keys,
        vec![
            "Zoo".to_string(),
            "apple".to_string(),
            "banana".to_string(),
            "z".to_string(),
        ]
    );
}

// ============================================================
// 五、删除操作的细节
// ============================================================

/// 删除不存在的键 → 返回 false，不报错
#[test]
fn delete_nonexistent_returns_false() {
    let mut store = KVStore::new();
    assert_eq!(store.delete("nope"), Ok(false));
}

/// 连续删除同一键 → 第一次 true，第二次 false
#[test]
fn delete_twice_same_key() {
    let mut store = KVStore::new();
    store.set("k", "v", None).unwrap();

    assert_eq!(store.delete("k"), Ok(true));
    assert_eq!(store.delete("k"), Ok(false));
    assert_eq!(store.len(), 0);
}

// ============================================================
// 六、parser 集成测试（成员C已交付）
// ============================================================

// --- 基本命令解析 ---

#[test]
fn parse_set_basic() {
    let result = parse_command("SET name Alice").unwrap();
    assert_eq!(result.command, Command::Set);
    assert_eq!(result.key, Some("name".to_string()));
    assert_eq!(result.value, Some("Alice".to_string()));
}

#[test]
fn parse_get_basic() {
    let result = parse_command("GET name").unwrap();
    assert_eq!(result.command, Command::Get);
    assert_eq!(result.key, Some("name".to_string()));
    assert_eq!(result.value, None);
}

#[test]
fn parse_del_basic() {
    let result = parse_command("DEL name").unwrap();
    assert_eq!(result.command, Command::Del);
    assert_eq!(result.key, Some("name".to_string()));
    assert_eq!(result.value, None);
}

#[test]
fn parse_list() {
    let result = parse_command("LIST").unwrap();
    assert_eq!(result.command, Command::List);
    assert_eq!(result.key, None);
    assert_eq!(result.value, None);
}

#[test]
fn parse_status() {
    let result = parse_command("STATUS").unwrap();
    assert_eq!(result.command, Command::Status);
}

#[test]
fn parse_ping() {
    let result = parse_command("PING").unwrap();
    assert_eq!(result.command, Command::Ping);
}

#[test]
fn parse_exit() {
    let result = parse_command("EXIT").unwrap();
    assert_eq!(result.command, Command::Exit);
}

// --- 大小写不敏感 ---

#[test]
fn parse_case_insensitive_lower() {
    assert_eq!(parse_command("set k v").unwrap().command, Command::Set);
    assert_eq!(parse_command("get k").unwrap().command, Command::Get);
    assert_eq!(parse_command("del k").unwrap().command, Command::Del);
    assert_eq!(parse_command("list").unwrap().command, Command::List);
}

#[test]
fn parse_case_insensitive_mixed() {
    assert_eq!(parse_command("SeT k v").unwrap().command, Command::Set);
    assert_eq!(parse_command("Get k").unwrap().command, Command::Get);
    assert_eq!(parse_command("PiNg").unwrap().command, Command::Ping);
}

// --- SET 的 value 可以包含空格 ---

#[test]
fn parse_set_value_with_spaces() {
    let result = parse_command("SET sentence Rust is easy").unwrap();
    assert_eq!(result.command, Command::Set);
    assert_eq!(result.key, Some("sentence".to_string()));
    assert_eq!(result.value, Some("Rust is easy".to_string()));
}

#[test]
fn parse_set_value_with_special_chars() {
    let result = parse_command("SET special !@#$%^&*()").unwrap();
    assert_eq!(result.value, Some("!@#$%^&*()".to_string()));
}

// --- 错误场景 ---

#[test]
fn parse_unknown_command() {
    let err = parse_command("FOOBAR").unwrap_err();
    assert!(err.message.contains("未知命令"));
}

#[test]
fn parse_empty_line() {
    let err = parse_command("").unwrap_err();
    assert!(err.message.contains("未知命令"));
}

#[test]
fn parse_missing_arg_set() {
    // SET 没有 key
    let err = parse_command("SET").unwrap_err();
    assert!(err.message.contains("缺少参数"));
}

#[test]
fn parse_set_missing_value() {
    // SET 有 key 但没有 value
    let err = parse_command("SET name").unwrap_err();
    assert!(err.message.contains("缺少参数"));
}

#[test]
fn parse_missing_arg_get() {
    let err = parse_command("GET").unwrap_err();
    assert!(err.message.contains("缺少参数"));
}

#[test]
fn parse_missing_arg_del() {
    let err = parse_command("DEL").unwrap_err();
    assert!(err.message.contains("缺少参数"));
}

#[test]
fn parse_extra_arg_list() {
    let err = parse_command("LIST extra").unwrap_err();
    assert!(err.message.contains("多余参数"));
}

#[test]
fn parse_extra_arg_get() {
    let err = parse_command("GET name extra").unwrap_err();
    assert!(err.message.contains("多余参数"));
}

#[test]
fn parse_invalid_key_empty() {
    // SET 后面两个空格再加 value → key 为空的情况
    let err = parse_command("SET  value").unwrap_err();
    assert!(err.message.contains("缺少参数") || err.message.contains("非法键"));
}

// 含空格 key 的非法校验在 store 层做（store 层已测试），
// parser 层只按空格切分，不做语义级的 key 合法性判断。

// --- 消息超长 ---

#[test]
fn parse_message_too_long() {
    let long_cmd = "SET k ".to_string() + &"a".repeat(2000);
    let err = parse_command(&long_cmd).unwrap_err();
    assert!(err.message.contains("消息超长") || err.message.contains("不能超过"));
}

// --- 前后空白自动 trim ---

#[test]
fn parse_trims_leading_spaces() {
    let result = parse_command("   SET k v").unwrap();
    assert_eq!(result.command, Command::Set);
}

#[test]
fn parse_trims_trailing_spaces() {
    let result = parse_command("SET k v   ").unwrap();
    assert_eq!(result.command, Command::Set);
    assert_eq!(result.value, Some("v".to_string()));
}

// ============================================================
// 七、parser + KVStore 全链路端到端测试
// ============================================================

/// 用 parse_command 解析命令后，真正执行到 KVStore 上
/// 模拟服务器内部的处理流程：解析 → 执行 → 断言结果
#[test]
fn e2e_parse_and_execute_set_get() {
    let mut store = KVStore::new();

    // 解析 SET name Alice
    let cmd = parse_command("SET name Alice").unwrap();
    assert_eq!(cmd.command, Command::Set);

    // 执行 SET
    let key = cmd.key.as_deref().unwrap();
    let value = cmd.value.as_deref().unwrap();
    store.set(key, value, None).unwrap();

    // 解析 GET name
    let cmd = parse_command("GET name").unwrap();
    assert_eq!(cmd.command, Command::Get);

    // 执行 GET
    let result = store.get(cmd.key.as_deref().unwrap()).unwrap();
    assert_eq!(result, Some("Alice"));
}

/// 全链路：解析 SET → 覆盖 → 解析 DEL → 验证删除
#[test]
fn e2e_parse_and_execute_full_flow() {
    let mut store = KVStore::new();

    // SET a 1
    let cmd = parse_command("SET a 1").unwrap();
    store
        .set(cmd.key.as_deref().unwrap(), cmd.value.as_deref().unwrap(), None)
        .unwrap();

    // SET b 2
    let cmd = parse_command("SET b 2").unwrap();
    store
        .set(cmd.key.as_deref().unwrap(), cmd.value.as_deref().unwrap(), None)
        .unwrap();

    // 数量
    assert_eq!(store.len(), 2);

    // SET a 999（覆盖）
    let cmd = parse_command("SET a 999").unwrap();
    store
        .set(cmd.key.as_deref().unwrap(), cmd.value.as_deref().unwrap(), None)
        .unwrap();
    assert_eq!(store.len(), 2);
    assert_eq!(store.get("a"), Ok(Some("999")));

    // DEL b
    let cmd = parse_command("DEL b").unwrap();
    let removed = store.delete(cmd.key.as_deref().unwrap()).unwrap();
    assert!(removed);
    assert_eq!(store.len(), 1);

    // LIST
    let cmd = parse_command("LIST").unwrap();
    assert_eq!(cmd.command, Command::List);
    let keys = store.list();
    assert_eq!(keys, vec!["a".to_string()]);

    // STATUS
    let cmd = parse_command("STATUS").unwrap();
    assert_eq!(cmd.command, Command::Status);
    assert_eq!(store.len(), 1);
}

/// 解析非法命令 → 返回错误 → 不影响存储状态
#[test]
fn e2e_parse_error_does_not_affect_store() {
    let mut store = KVStore::new();
    store.set("good", "value", None).unwrap();

    // 尝试解析一堆错误命令
    let _ = parse_command("");
    let _ = parse_command("FOOBAR");
    let _ = parse_command("SET");
    let _ = parse_command("GET");

    // 存储状态不变
    assert_eq!(store.len(), 1);
    assert_eq!(store.get("good"), Ok(Some("value")));
}

/// PING 和 EXIT 不操作数据，只解析成功即可
#[test]
fn e2e_ping_exit_no_side_effects() {
    let store = KVStore::new();

    let ping = parse_command("PING").unwrap();
    assert_eq!(ping.command, Command::Ping);

    let exit = parse_command("EXIT").unwrap();
    assert_eq!(exit.command, Command::Exit);

    // 存储仍然为空
    assert!(store.is_empty());
}
