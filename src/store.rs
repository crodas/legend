//! Core storage types and traits for persisting paused executions.

use std::collections::HashMap;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Get the current Unix timestamp in milliseconds.
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Unique identifier for an execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecutionId(pub Uuid);

impl ExecutionId {
    /// Create a new random execution ID.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create an execution ID from an existing UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the underlying UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Get the execution ID as bytes (for storage keys).
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}

impl Default for ExecutionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ExecutionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// Store Trait
// ============================================================================

/// Errors that can occur during storage operations.
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum StoreError {
    /// The requested execution was not found.
    #[error("Execution not found: {0}")]
    NotFound(ExecutionId),

    /// A storage backend error occurred.
    #[error("Storage error: {0}")]
    Backend(String),
}

/// A record of a paused execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PausedRecord {
    /// The execution ID.
    pub id: ExecutionId,

    /// Serialized execution data.
    pub data: Vec<u8>,

    /// When the execution was paused (Unix millis).
    pub paused_at: u64,
}

/// Storage backend for paused executions.
///
/// Implementations must be thread-safe (`Send + Sync`).
/// Methods are async to support both sync and async backends.
#[async_trait::async_trait]
pub trait Store: Send + Sync {
    /// Save a paused execution.
    async fn save(&self, id: ExecutionId, data: Vec<u8>) -> Result<(), StoreError>;

    /// Get a paused execution by ID.
    async fn get(&self, id: ExecutionId) -> Result<PausedRecord, StoreError>;

    /// Delete a paused execution (e.g., after resuming).
    async fn delete(&self, id: ExecutionId) -> Result<(), StoreError>;

    /// Check if an execution exists.
    async fn exists(&self, id: ExecutionId) -> Result<bool, StoreError>;
}

// ============================================================================
// In-Memory Store
// ============================================================================

/// In-memory storage backend for testing and single-process use.
///
/// Uses `parking_lot::RwLock` for thread-safe access.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    records: RwLock<HashMap<ExecutionId, PausedRecord>>,
}

impl InMemoryStore {
    /// Create a new empty in-memory store.
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
        }
    }

    /// Get the number of stored records.
    pub fn len(&self) -> usize {
        self.records.read().len()
    }

    /// Check if the store is empty.
    pub fn is_empty(&self) -> bool {
        self.records.read().is_empty()
    }
}

#[async_trait::async_trait]
impl Store for InMemoryStore {
    async fn save(&self, id: ExecutionId, data: Vec<u8>) -> Result<(), StoreError> {
        let record = PausedRecord {
            id,
            data,
            paused_at: now_millis(),
        };
        self.records.write().insert(id, record);
        Ok(())
    }

    async fn get(&self, id: ExecutionId) -> Result<PausedRecord, StoreError> {
        self.records
            .read()
            .get(&id)
            .cloned()
            .ok_or(StoreError::NotFound(id))
    }

    async fn delete(&self, id: ExecutionId) -> Result<(), StoreError> {
        self.records
            .write()
            .remove(&id)
            .map(|_| ())
            .ok_or(StoreError::NotFound(id))
    }

    async fn exists(&self, id: ExecutionId) -> Result<bool, StoreError> {
        Ok(self.records.read().contains_key(&id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_id_display() {
        let id = ExecutionId::new();
        let display = format!("{}", id);
        assert!(!display.is_empty());
    }

    #[tokio::test]
    async fn in_memory_store_save_and_get() {
        let store = InMemoryStore::new();
        let id = ExecutionId::new();
        let data = vec![1, 2, 3, 4];

        store
            .save(id, data.clone())
            .await
            .expect("save should succeed");

        let record = store.get(id).await.expect("get should succeed");
        assert_eq!(record.id, id);
        assert_eq!(record.data, data);
        assert!(record.paused_at > 0);
    }

    #[tokio::test]
    async fn in_memory_store_get_not_found() {
        let store = InMemoryStore::new();
        let id = ExecutionId::new();

        let result = store.get(id).await;
        assert!(matches!(result, Err(StoreError::NotFound(i)) if i == id));
    }

    #[tokio::test]
    async fn in_memory_store_delete() {
        let store = InMemoryStore::new();
        let id = ExecutionId::new();

        store
            .save(id, vec![1, 2, 3])
            .await
            .expect("save should succeed");
        assert!(store.exists(id).await.expect("exists should succeed"));

        store.delete(id).await.expect("delete should succeed");
        assert!(!store.exists(id).await.expect("exists should succeed"));
    }

    #[tokio::test]
    async fn in_memory_store_delete_not_found() {
        let store = InMemoryStore::new();
        let id = ExecutionId::new();

        let result = store.delete(id).await;
        assert!(matches!(result, Err(StoreError::NotFound(i)) if i == id));
    }

    #[tokio::test]
    async fn in_memory_store_exists() {
        let store = InMemoryStore::new();
        let id = ExecutionId::new();

        assert!(!store.exists(id).await.expect("exists should succeed"));

        store.save(id, vec![]).await.expect("save should succeed");
        assert!(store.exists(id).await.expect("exists should succeed"));
    }

    #[test]
    fn in_memory_store_len_and_is_empty() {
        let store = InMemoryStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        // Use blocking approach for sync test
        let id = ExecutionId::new();
        store.records.write().insert(
            id,
            PausedRecord {
                id,
                data: vec![],
                paused_at: now_millis(),
            },
        );

        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);
    }
}
