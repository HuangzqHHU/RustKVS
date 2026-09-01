//! 内存键值存储模块（成员B负责）
//!
//! 第1天：定义 KVStore 接口与 StoreError 错误类型（骨架，可编译）。
//! 第2天：成员B实现完整功能：SET覆盖写、GET、DEL、LIST（有序）、len，
//!        以及空键、含空格键等非法键校验。
//!
//! 依赖关系：成员A的 server 和成员D的并发改造都依赖本接口。

use std::collections::HashMap;

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

/// 键值存储（第2天已由成员B实现完整 CRUD）
pub struct KVStore {
    data: HashMap<String, String>,
}

impl KVStore {
    /// 创建一个空存储
    pub fn new() -> Self {
        KVStore {
            data: HashMap::new(),
        }
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

    /// 写入或覆盖键值
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), StoreError> {
        Self::validate_key(key)?;
        self.data.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// 查询键值；键不存在时返回 Ok(None)
    pub fn get(&self, key: &str) -> Result<Option<&str>, StoreError> {
        Self::validate_key(key)?;
        Ok(self.data.get(key).map(String::as_str))
    }

    /// 删除键值；返回是否真的删除了某个键
    pub fn delete(&mut self, key: &str) -> Result<bool, StoreError> {
        Self::validate_key(key)?;
        Ok(self.data.remove(key).is_some())
    }

    /// 列出全部键（按键名字典序排列，方便演示）
    pub fn list(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.data.keys().cloned().collect();
        keys.sort();
        keys
    }

    /// 当前数据数量
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 预置 a/b/c 三个键的存储
    fn sample_store() -> KVStore {
        let mut store = KVStore::new();
        store.set("b", "2").unwrap();
        store.set("a", "1").unwrap();
        store.set("c", "3").unwrap();
        store
    }

    #[test]
    fn set_and_get_roundtrip() {
        let mut store = KVStore::new();
        store.set("name", "kvstore").unwrap();
        assert_eq!(store.get("name"), Ok(Some("kvstore")));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn set_overwrites_existing_key() {
        let mut store = KVStore::new();
        store.set("k", "v1").unwrap();
        store.set("k", "v2").unwrap();
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
        store.set("k", "v").unwrap();
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn reject_empty_key() {
        let mut store = KVStore::new();
        assert!(matches!(store.set("", "v"), Err(StoreError::InvalidKey(_))));
        assert!(matches!(store.get(""), Err(StoreError::InvalidKey(_))));
        assert!(matches!(store.delete(""), Err(StoreError::InvalidKey(_))));
    }

    #[test]
    fn reject_key_with_space() {
        let mut store = KVStore::new();
        assert!(matches!(store.set("a b", "v"), Err(StoreError::InvalidKey(_))));
        assert!(matches!(store.get("a b"), Err(StoreError::InvalidKey(_))));
    }

    #[test]
    fn reject_key_with_newline() {
        let mut store = KVStore::new();
        assert!(matches!(store.set("a\nb", "v"), Err(StoreError::InvalidKey(_))));
    }

    #[test]
    fn value_can_contain_spaces() {
        let mut store = KVStore::new();
        store.set("k", "hello world  !").unwrap();
        assert_eq!(store.get("k"), Ok(Some("hello world  !")));
    }
}
