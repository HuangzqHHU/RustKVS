//! 内存键值存储模块（成员B负责）
//!
//! 第1天：定义 KVStore 接口与 StoreError 错误类型（骨架，可编译）。
//! 第2天：成员B实现完整功能：SET覆盖写、GET、DEL、LIST（有序）、len，
//!        以及空键、含空格键等非法键校验。
//! 第4天（冲刺优秀）：支持 TTL 过期时间（接口契约 v2）：
//!   - 数据结构 HashMap<String, (String, Option<Instant>)>（值 + 过期时间点）；
//!   - set 增加 ttl 参数，Some(秒) 表示过期时间，None 表示永不过期；
//!   - get/list/len 惰性过期检查（访问时检查并清理）；
//!   - 覆盖写时新 ttl 生效；不带 ttl 的覆盖写 = 改为永不过期。
//!
//! 依赖关系：成员A的 server 和成员D的并发改造都依赖本接口。

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 存储操作错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// 键不合法（空、含空白、含换行等），附带具体说明
    InvalidKey(String),
    /// 键不存在（保留给上层使用；当前 get 缺失返回 Ok(None)，由调用方按协议输出"键不存在"）
    #[allow(dead_code)]
    KeyNotFound,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::InvalidKey(msg) => write!(f, "非法键: {}", msg),
            StoreError::KeyNotFound => write!(f, "键不存在"),
        }
    }
}

impl std::error::Error for StoreError {}

/// 键值存储（第2天已由成员B实现完整 CRUD；第4天加入 TTL）
pub struct KVStore {
    /// key -> (value, 过期时间点)；expire_at 为 None 表示永不过期
    data: HashMap<String, (String, Option<Instant>)>,
}

impl KVStore {
    /// 创建一个空存储
    pub fn new() -> Self {
        KVStore { data: HashMap::new() }
    }

    /// 校验键合法性：非空、不含空白字符（空格/制表符）、不含换行
    fn validate_key(key: &str) -> Result<(), StoreError> {
        if key.is_empty() {
            return Err(StoreError::InvalidKey("键不能为空".to_string()));
        }
        if key.chars().any(|c| c == ' ' || c == '\t') {
            return Err(StoreError::InvalidKey("键不能包含空白字符".to_string()));
        }
        if key.contains('\n') || key.contains('\r') {
            return Err(StoreError::InvalidKey("键不能包含换行".to_string()));
        }
        Ok(())
    }

    /// 判断某个过期时间点是否已过期（None 表示永不过期）
    fn is_expired(exp: Option<Instant>, now: Instant) -> bool {
        exp.map_or(false, |t| t <= now)
    }

    /// 惰性清理：物理移除所有已过期的键（在写操作时调用）
    fn sweep_expired(&mut self) {
        let now = Instant::now();
        self.data.retain(|_, entry| !Self::is_expired(entry.1, now));
    }

    /// 写入或覆盖键值
    ///
    /// - `ttl: Some(秒)` 表示该键在指定秒数后过期；
    /// - `ttl: None` 表示永不过期；
    /// - 覆盖写时新 ttl 生效；不带 ttl 的覆盖写 = 改为永不过期。
    pub fn set(&mut self, key: &str, value: &str, ttl: Option<u64>) -> Result<(), StoreError> {
        Self::validate_key(key)?;
        self.sweep_expired();
        let expire_at = ttl.map(|secs| Instant::now() + Duration::from_secs(secs));
        self.data.insert(key.to_string(), (value.to_string(), expire_at));
        Ok(())
    }

    /// 查询键值；键不存在或已过期时返回 Ok(None)
    pub fn get(&self, key: &str) -> Result<Option<&str>, StoreError> {
        Self::validate_key(key)?;
        let now = Instant::now();
        Ok(self.data.get(key).and_then(|entry| {
            if Self::is_expired(entry.1, now) {
                None
            } else {
                Some(entry.0.as_str())
            }
        }))
    }

    /// 删除键值；返回是否真的删除了某个未过期的键
    ///
    /// 已过期或缺失的键均视为"键不存在"，返回 false（并顺带移除残留条目）。
    pub fn delete(&mut self, key: &str) -> Result<bool, StoreError> {
        Self::validate_key(key)?;
        let now = Instant::now();
        let existed = match self.data.get(key) {
            Some(entry) => !Self::is_expired(entry.1, now),
            None => false,
        };
        self.data.remove(key);
        Ok(existed)
    }

    /// 列出全部未过期键（按键名字典序排列，方便演示）
    pub fn list(&self) -> Vec<String> {
        let now = Instant::now();
        let mut keys: Vec<String> = self
            .data
            .iter()
            .filter(|(_, entry)| !Self::is_expired(entry.1, now))
            .map(|(k, _)| k.clone())
            .collect();
        keys.sort();
        keys
    }

    /// 当前未过期数据数量
    pub fn len(&self) -> usize {
        let now = Instant::now();
        self.data
            .values()
            .filter(|entry| !Self::is_expired(entry.1, now))
            .count()
    }

    /// 是否为空（无未过期数据）
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 预置 a/b/c 三个永久键的存储
    fn sample_store() -> KVStore {
        let mut store = KVStore::new();
        store.set("b", "2", None).unwrap();
        store.set("a", "1", None).unwrap();
        store.set("c", "3", None).unwrap();
        store
    }

    // ---------- 基础 CRUD ----------

    #[test]
    fn set_and_get_roundtrip() {
        let mut store = KVStore::new();
        store.set("name", "kvstore", None).unwrap();
        assert_eq!(store.get("name"), Ok(Some("kvstore")));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn set_overwrites_existing_key() {
        let mut store = KVStore::new();
        store.set("k", "v1", None).unwrap();
        store.set("k", "v2", None).unwrap();
        assert_eq!(store.get("k"), Ok(Some("v2")));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn get_missing_key_returns_none() {
        let store = KVStore::new();
        assert_eq!(store.get("nope"), Ok(None));
    }

    #[test]
    fn delete_returns_whether_removed() {
        let mut store = sample_store();
        assert!(store.delete("a").unwrap());
        assert!(!store.delete("a").unwrap()); // 已删除，再删返回 false
        assert!(!store.delete("missing").unwrap());
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn list_returns_sorted_keys() {
        let store = sample_store();
        assert_eq!(
            store.list(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn list_empty_store() {
        let store = KVStore::new();
        assert!(store.list().is_empty());
    }

    #[test]
    fn len_and_is_empty() {
        let mut store = KVStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        store.set("k", "v", None).unwrap();
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn reject_empty_key() {
        let mut store = KVStore::new();
        assert!(matches!(store.set("", "v", None), Err(StoreError::InvalidKey(_))));
        assert!(matches!(store.get(""), Err(StoreError::InvalidKey(_))));
        assert!(matches!(store.delete(""), Err(StoreError::InvalidKey(_))));
    }

    #[test]
    fn reject_key_with_space() {
        let mut store = KVStore::new();
        assert!(matches!(store.set("a b", "v", None), Err(StoreError::InvalidKey(_))));
        assert!(matches!(store.get("a b"), Err(StoreError::InvalidKey(_))));
    }

    #[test]
    fn reject_key_with_newline() {
        let mut store = KVStore::new();
        assert!(matches!(store.set("a\nb", "v", None), Err(StoreError::InvalidKey(_))));
    }

    #[test]
    fn value_can_contain_spaces() {
        let mut store = KVStore::new();
        store.set("k", "hello world  !", None).unwrap();
        assert_eq!(store.get("k"), Ok(Some("hello world  !")));
    }

    // ---------- TTL 过期（接口契约 v2） ----------

    #[test]
    fn expired_key_get_returns_none() {
        let mut store = KVStore::new();
        store.set("k", "v", Some(0)).unwrap(); // 0 秒 → 立即过期
        assert_eq!(store.get("k"), Ok(None));
    }

    #[test]
    fn unexpired_key_get_returns_value() {
        let mut store = KVStore::new();
        store.set("k", "v", Some(3600)).unwrap(); // 1 小时后才过期
        assert_eq!(store.get("k"), Ok(Some("v")));
    }

    #[test]
    fn no_ttl_is_permanent() {
        let mut store = KVStore::new();
        store.set("k", "v", None).unwrap();
        assert_eq!(store.get("k"), Ok(Some("v")));
    }

    #[test]
    fn overwrite_with_new_ttl_takes_effect() {
        let mut store = KVStore::new();
        store.set("k", "v1", Some(3600)).unwrap();
        store.set("k", "v2", Some(0)).unwrap(); // 覆盖为立即过期
        assert_eq!(store.get("k"), Ok(None));
    }

    #[test]
    fn overwrite_without_ttl_becomes_permanent() {
        let mut store = KVStore::new();
        store.set("k", "v1", Some(0)).unwrap();
        store.set("k", "v2", None).unwrap(); // 不带 ttl 的覆盖写 → 永不过期
        assert_eq!(store.get("k"), Ok(Some("v2")));
    }

    #[test]
    fn list_excludes_expired_keys() {
        let mut store = KVStore::new();
        store.set("a", "1", None).unwrap();
        store.set("b", "2", Some(0)).unwrap(); // 过期
        store.set("c", "3", Some(3600)).unwrap();
        assert_eq!(store.list(), vec!["a".to_string(), "c".to_string()]);
    }

    #[test]
    fn len_excludes_expired_keys() {
        let mut store = KVStore::new();
        store.set("a", "1", None).unwrap();
        store.set("b", "2", Some(0)).unwrap(); // 过期
        assert_eq!(store.len(), 1);
        assert!(!store.is_empty());
    }

    #[test]
    fn delete_expired_key_returns_false_and_removes() {
        let mut store = KVStore::new();
        store.set("k", "v", Some(0)).unwrap();
        assert!(!store.delete("k").unwrap()); // 视为"键不存在"
        assert!(!store.data.contains_key("k")); // 已物理移除
    }

    #[test]
    fn set_sweeps_expired_entries() {
        let mut store = KVStore::new();
        store.set("a", "1", Some(0)).unwrap(); // 过期
        store.set("b", "2", Some(3600)).unwrap();
        // 第二次 set 触发惰性清理，a 已被物理移除
        assert_eq!(store.data.len(), 1);
        assert!(store.data.contains_key("b"));
        assert!(!store.data.contains_key("a"));
    }
}
