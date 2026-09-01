//! 内存键值存储模块（成员B负责）
//!
//! 第1天：定义 KVStore 接口与 StoreError 错误类型（骨架，可编译）。
//! 第2天：成员B实现完整功能：SET覆盖写、GET、DEL、LIST（有序）、len，
//!        以及空键、含空格键等非法键校验。
//!
//! 依赖关系：成员A的 server 和成员D的并发改造都依赖本接口。

use std::collections::HashMap;

/// 存储操作错误（成员B可继续扩展）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// 键不合法（空、含空格、含换行等），附带具体说明
    InvalidKey(String),
    /// 键不存在
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

/// 键值存储（第2天由成员B实现；本骨架保证全工程可编译）
pub struct KVStore {
    data: HashMap<String, String>,
}

impl KVStore {
    /// 创建一个空存储
    pub fn new() -> Self {
        KVStore { data: HashMap::new() }
    }

    /// 写入或覆盖键值
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), StoreError> {
        // TODO(成员B): 第2天实现，含非法键校验；成功后插入 data
        let _ = (key, value, &mut self.data);
        Ok(())
    }

    /// 查询键值；键不存在时返回 Ok(None)
    pub fn get(&self, key: &str) -> Result<Option<&str>, StoreError> {
        // TODO(成员B): 第2天实现
        let _ = (key, &self.data);
        Ok(None)
    }

    /// 删除键值；返回是否真的删除了某个键
    pub fn delete(&mut self, key: &str) -> Result<bool, StoreError> {
        // TODO(成员B): 第2天实现
        let _ = (key, &mut self.data);
        Ok(false)
    }

    /// 列出全部键（按键名字典序排列，方便演示）
    pub fn list(&self) -> Vec<String> {
        // TODO(成员B): 第2天实现
        let _ = &self.data;
        Vec::new()
    }

    /// 当前数据数量
    pub fn len(&self) -> usize {
        // TODO(成员B): 第2天实现
        let _ = &self.data;
        0
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
