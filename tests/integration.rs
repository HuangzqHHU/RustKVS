//! 集成测试（成员D负责）
//!
//! 第2天：无网络模式下的端到端测试
//!   - KVStore 多步骤复杂场景（模拟完整用户操作流）
//!   - 边界与异常场景全覆盖
//!   - parser 测试暂用 #[ignore] 占位，等 C 实现后启用
//!
//! 说明：B 已经在 store 模块内写了单元测试（单方法级）。
//! 这里写的是集成视角的测试——跨方法、多步骤、模拟真实使用流程。

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
    store.set("name", "Alice").unwrap();
    assert_eq!(store.get("name"), Ok(Some("Alice")));
    assert_eq!(store.len(), 1);

    // SET age 20
    store.set("age", "20").unwrap();
    assert_eq!(store.get("age"), Ok(Some("20")));
    assert_eq!(store.len(), 2);

    // SET name Bob（覆盖写）
    store.set("name", "Bob").unwrap();
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
        store.set(&format!("key{}", i), &format!("value{}", i)).unwrap();
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
    // 第0个应该是 key0，最后一个是 key99
    assert_eq!(keys.first().unwrap(), "key0");
    assert_eq!(keys.last().unwrap(), "key99");
}

/// 删除全部键后回到空状态
#[test]
fn e2e_delete_all_back_to_empty() {
    let mut store = KVStore::new();
    store.set("a", "1").unwrap();
    store.set("b", "2").unwrap();
    store.set("c", "3").unwrap();
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
    store.set("k", "").unwrap();
    assert_eq!(store.get("k"), Ok(Some("")));
}

/// 值可以包含空格（协议规定值允许空格）
#[test]
fn value_with_spaces() {
    let mut store = KVStore::new();
    store.set("greeting", "hello world").unwrap();
    assert_eq!(store.get("greeting"), Ok(Some("hello world")));
}

/// 值可以包含特殊字符
#[test]
fn value_with_special_chars() {
    let mut store = KVStore::new();
    let val = "!@#$%^&*()_+-=[]{}|;:'\",.<>?/`~";
    store.set("special", val).unwrap();
    assert_eq!(store.get("special"), Ok(Some(val)));
}

/// 长值正常存取
#[test]
fn value_very_long() {
    let mut store = KVStore::new();
    let long_val = "a".repeat(10000);
    store.set("long", &long_val).unwrap();
    let result = store.get("long").unwrap().unwrap();
    assert_eq!(result.len(), 10000);
    assert_eq!(result, long_val);
}

/// 值可以包含中文
#[test]
fn value_with_chinese() {
    let mut store = KVStore::new();
    store.set("msg", "你好，世界").unwrap();
    assert_eq!(store.get("msg"), Ok(Some("你好，世界")));
}

// ============================================================
// 三、非法键测试（覆盖各种非法场景）
// ============================================================

/// 空键 → SET/GET/DEL 都应报错 InvalidKey
#[test]
fn invalid_key_empty() {
    let mut store = KVStore::new();

    let set_err = store.set("", "v").unwrap_err();
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
    assert!(store.set("hello world", "v").is_err());
    assert!(store.get("hello world").is_err());
    assert!(store.delete("hello world").is_err());
}

/// 含制表符的键
#[test]
fn invalid_key_with_tab() {
    let mut store = KVStore::new();
    assert!(store.set("a\tb", "v").is_err());
}

/// 含换行的键
#[test]
fn invalid_key_with_newline() {
    let mut store = KVStore::new();
    assert!(store.set("a\nb", "v").is_err());
}

/// 含回车的键
#[test]
fn invalid_key_with_carriage_return() {
    let mut store = KVStore::new();
    assert!(store.set("a\rb", "v").is_err());
}

/// 非法键不会写入数据
#[test]
fn invalid_key_does_not_pollute_store() {
    let mut store = KVStore::new();
    let _ = store.set("", "v"); // 忽略错误
    let _ = store.set("a b", "v");
    let _ = store.set("a\nb", "v");

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

/// list 按字典序排序（含数字的键）
#[test]
fn list_sorted_lexicographic() {
    let mut store = KVStore::new();
    store.set("z", "1").unwrap();
    store.set("apple", "2").unwrap();
    store.set("banana", "3").unwrap();
    store.set("Zoo", "4").unwrap(); // 大写字母 ASCII 码在小写前面

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
    store.set("k", "v").unwrap();

    assert_eq!(store.delete("k"), Ok(true));
    assert_eq!(store.delete("k"), Ok(false));
    assert_eq!(store.len(), 0);
}

// ============================================================
// 六、parser 集成测试（等 C 实现后取消 ignore）
// ============================================================

/// SET 命令解析
#[test]
fn parse_set_command() {
    use kvstore::parser::parse_command;
    let result = parse_command("SET name Alice");
    assert!(result.is_ok());
}

/// GET 命令解析
#[test]
fn parse_get_command() {
    use kvstore::parser::parse_command;
    let result = parse_command("GET name");
    assert!(result.is_ok());
}

/// 未知命令 → 错误
#[test]
fn parse_unknown_command() {
    use kvstore::parser::parse_command;
    assert!(parse_command("FOOBAR x").is_err());
}

/// 缺少参数 → 错误
#[test]
fn parse_missing_args() {
    use kvstore::parser::parse_command;
    assert!(parse_command("SET name").is_err());
}

/// 大小写不敏感
#[test]
fn parse_case_insensitive() {
    use kvstore::parser::parse_command;
    assert!(parse_command("set k v").is_ok());
    assert!(parse_command("Get k").is_ok());
}