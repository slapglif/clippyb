use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use anyhow::Result;

use super::queue_item::{QueueItem, QueueStatus};

#[derive(Debug, Serialize, Deserialize)]
struct QueueSnapshot {
    items: Vec<QueueItem>,
    version: u32,
}

pub struct PersistentQueue {
    items: Arc<RwLock<VecDeque<QueueItem>>>,
    file_path: PathBuf,
    save_mutex: Arc<Mutex<()>>, // Prevent concurrent saves
}

impl PersistentQueue {
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let queue = Self {
            items: Arc::new(RwLock::new(VecDeque::new())),
            file_path,
            save_mutex: Arc::new(Mutex::new(())),
        };
        
        // Load existing queue if it exists
        if let Err(e) = queue.load() {
            println!("⚠️ Could not load existing queue: {}, starting fresh", e);
        }
        
        Ok(queue)
    }
    
    pub async fn enqueue(&self, item: QueueItem) -> Result<()> {
        {
            let mut items = self.items.write().await;
            println!("📥 Queued: {} (ID: {})", item.display_name(), item.id);
            items.push_back(item);
        }
        self.save().await?;
        Ok(())
    }
    
    pub async fn enqueue_multiple(&self, items: Vec<QueueItem>) -> Result<()> {
        println!("📥 Queuing {} items...", items.len());
        {
            let mut queue_items = self.items.write().await;
            for item in items {
                println!("  + {}", item.display_name());
                queue_items.push_back(item);
            }
        }
        self.save().await?;
        println!("✅ All {} items queued successfully", self.len().await);
        Ok(())
    }
    
    pub async fn dequeue(&self) -> Option<QueueItem> {
        let mut items = self.items.write().await;
        items.pop_front()
    }
    
    pub async fn peek_pending(&self) -> Option<QueueItem> {
        let items = self.items.read().await;
        items.iter()
            .find(|item| item.status == QueueStatus::Pending)
            .cloned()
    }
    
    pub async fn update_item(&self, updated_item: QueueItem) -> Result<()> {
        {
            let mut items = self.items.write().await;
            if let Some(pos) = items.iter().position(|item| item.id == updated_item.id) {
                items[pos] = updated_item;
            }
        }
        self.save().await?;
        Ok(())
    }
    
    pub async fn len(&self) -> usize {
        self.items.read().await.len()
    }
    
    pub async fn is_empty(&self) -> bool {
        self.items.read().await.is_empty()
    }
    
    pub async fn get_status_counts(&self) -> (usize, usize, usize, usize, usize) {
        let items = self.items.read().await;
        let mut pending = 0;
        let mut in_progress = 0;
        let mut completed = 0;
        let mut failed = 0;
        let mut skipped = 0;
        
        for item in items.iter() {
            match item.status {
                QueueStatus::Pending => pending += 1,
                QueueStatus::InProgress => in_progress += 1,
                QueueStatus::Completed => completed += 1,
                QueueStatus::Failed => failed += 1,
                QueueStatus::Skipped => skipped += 1,
            }
        }
        
        (pending, in_progress, completed, failed, skipped)
    }
    
    pub async fn get_all_items(&self) -> Vec<QueueItem> {
        self.items.read().await.clone().into()
    }
    
    pub async fn clear_completed(&self) -> Result<usize> {
        let removed_count = {
            let mut items = self.items.write().await;
            let original_len = items.len();
            items.retain(|item| !matches!(item.status, QueueStatus::Completed | QueueStatus::Skipped));
            original_len - items.len()
        };
        
        if removed_count > 0 {
            self.save().await?;
            println!("🧹 Cleared {} completed/skipped items from queue", removed_count);
        }
        
        Ok(removed_count)
    }
    
    pub async fn retry_failed(&self) -> Result<usize> {
        let retry_count = {
            let mut items = self.items.write().await;
            let mut count = 0;
            for item in items.iter_mut() {
                if item.status == QueueStatus::Failed {
                    item.reset_for_retry();
                    count += 1;
                }
            }
            count
        };
        
        if retry_count > 0 {
            self.save().await?;
            println!("🔄 Reset {} failed items for retry", retry_count);
        }
        
        Ok(retry_count)
    }
    
    async fn save(&self) -> Result<()> {
        let _lock = self.save_mutex.lock().await;
        
        let items = self.items.read().await;
        let snapshot = QueueSnapshot {
            items: items.clone().into(),
            version: 1,
        };
        
        let json = serde_json::to_string_pretty(&snapshot)?;
        
        // Atomic write: write to temp file then rename
        let temp_path = self.file_path.with_extension("tmp");
        fs::write(&temp_path, json)?;
        fs::rename(temp_path, &self.file_path)?;
        
        Ok(())
    }
    
    fn load(&self) -> Result<()> {
        if !self.file_path.exists() {
            return Ok(()); // No existing queue
        }
        
        let json = fs::read_to_string(&self.file_path)?;
        let snapshot: QueueSnapshot = serde_json::from_str(&json)?;
        
        // Reset in-progress items to pending on restart
        let mut items = VecDeque::new();
        for mut item in snapshot.items {
            if item.status == QueueStatus::InProgress {
                println!("🔄 Resetting in-progress item to pending: {}", item.display_name());
                item.status = QueueStatus::Pending;
                item.started_at = None;
            }
            items.push_back(item);
        }
        
        // Load into queue
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let mut queue_items = self.items.write().await;
            *queue_items = items;
        });
        
        let (pending, in_progress, completed, failed, skipped) = rt.block_on(self.get_status_counts());
        println!("📂 Loaded queue: {} pending, {} completed, {} failed, {} skipped", 
                pending, completed, failed, skipped);
        
        Ok(())
    }
}