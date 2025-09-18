use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use anyhow::Result;

use super::persistent_queue::PersistentQueue;
use super::queue_item::{QueueItem, QueueStatus};
use crate::MusicDownloader;
use crate::utils::smart_limiter::SmartLimiter;

pub struct QueueProcessor {
    queue: Arc<PersistentQueue>,
    downloader: Arc<MusicDownloader>,
    limiter: SmartLimiter,
    progress_tx: Option<mpsc::UnboundedSender<QueueProgress>>,
}

#[derive(Debug, Clone)]
pub struct QueueProgress {
    pub current_item: Option<QueueItem>,
    pub pending_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub total_processed: usize,
}

impl QueueProcessor {
    pub fn new(
        queue: Arc<PersistentQueue>, 
        downloader: Arc<MusicDownloader>
    ) -> Self {
        Self {
            queue,
            downloader,
            limiter: SmartLimiter::new(),
            progress_tx: None,
        }
    }
    
    pub fn with_progress_channel(mut self, tx: mpsc::UnboundedSender<QueueProgress>) -> Self {
        self.progress_tx = Some(tx);
        self
    }
    
    pub async fn start_processing(&self) {
        println!("🚀 Fully async queue processor started");
        
        loop {
            // Get ALL pending items
            let pending_items = self.queue.get_pending_items().await;
            
            if !pending_items.is_empty() {
                println!("🚀 Starting {} async tasks for parallel processing", pending_items.len());
                
                // Spawn async task for each pending item
                for mut item in pending_items {
                    // Clone necessary components for the task
                    let queue_clone = self.queue.clone();
                    let downloader_clone = self.downloader.clone();
                    let limiter_clone = self.limiter.clone();
                    let progress_tx_clone = self.progress_tx.clone();
                    
                    let task = tokio::spawn(async move {
                        // Acquire permit for concurrency control
                        let _permit = limiter_clone.acquire().await;
                        
                        // Mark as in progress
                        item.start_processing();
                        if let Err(e) = queue_clone.update_item(item.clone()).await {
                            eprintln!("❌ Failed to update item status: {}", e);
                            return;
                        }
                        
                        println!("🎵 [ASYNC] Processing: {}", item.display_name());
                        
                        // Process the item
                        let result = Self::process_item_async(&downloader_clone, &item).await;
                        
                        // Update item based on result
                        match result {
                            Ok(()) => {
                                item.complete();
                                println!("✅ [ASYNC] Completed: {}", item.display_name());
                            }
                            Err(e) => {
                                let error_msg = e.to_string();
                                if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                                    item.skip(format!("Duplicate: {}", error_msg));
                                    println!("⏭️ [ASYNC] Skipped (duplicate): {}", item.display_name());
                                } else {
                                    item.fail(error_msg);
                                    println!("❌ [ASYNC] Failed: {} - {}", item.display_name(), e);
                                }
                            }
                        }
                        
                        // Save updated item
                        if let Err(e) = queue_clone.update_item(item).await {
                            eprintln!("❌ Failed to update item after processing: {}", e);
                        }
                        
                        // Send progress update
                        if let Some(tx) = &progress_tx_clone {
                            let (pending, in_progress, completed, failed, skipped) = queue_clone.get_status_counts().await;
                            let progress = QueueProgress {
                                current_item: None,
                                pending_count: pending,
                                completed_count: completed + skipped,
                                failed_count: failed,
                                total_processed: completed + failed + skipped,
                            };
                            let _ = tx.send(progress);
                        }
                    });
                }
                
                // Don't wait for all tasks to complete - let them run in background
                // Wait longer before checking for new items to avoid re-spawning same items
                sleep(Duration::from_millis(5000)).await;
            } else {
                // No pending items, sleep and check again
                sleep(Duration::from_millis(1000)).await;
                
                // Send status update even when idle
                self.send_progress_update(None).await;
            }
        }
    }
    
    // Static method for async processing
    async fn process_item_async(downloader: &Arc<MusicDownloader>, item: &QueueItem) -> Result<()> {
        match item.item_type.as_str() {
            "spotify_playlist" => {
                downloader.process_spotify_url(&item.url).await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
            "spotify_track" => {
                downloader.process_spotify_url(&item.url).await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
            "soundcloud_track" => {
                downloader.process_soundcloud_url(&item.url).await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
            "youtube_url" => {
                // For now, treat YouTube URLs as song names
                downloader.process_song_name(&item.url).await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
            "song_name" => {
                downloader.process_song_name(&item.url).await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
            _ => {
                Err(anyhow::anyhow!("Unknown item type: {}", item.item_type))
            }
        }
    }
    
    
    async fn send_progress_update(&self, current_item: Option<QueueItem>) {
        if let Some(tx) = &self.progress_tx {
            let (pending, in_progress, completed, failed, skipped) = self.queue.get_status_counts().await;
            
            let progress = QueueProgress {
                current_item,
                pending_count: pending,
                completed_count: completed + skipped, // Count skipped as completed
                failed_count: failed,
                total_processed: completed + failed + skipped,
            };
            
            let _ = tx.send(progress);
        }
    }
    
    pub async fn get_queue_summary(&self) -> String {
        let (pending, in_progress, completed, failed, skipped) = self.queue.get_status_counts().await;
        let total = pending + in_progress + completed + failed + skipped;
        
        if total == 0 {
            "📭 Queue is empty".to_string()
        } else {
            format!(
                "📊 Queue: {} total | {} pending | {} in progress | {} completed | {} failed | {} skipped",
                total, pending, in_progress, completed, failed, skipped
            )
        }
    }
}