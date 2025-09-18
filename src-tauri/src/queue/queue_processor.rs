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
        println!("🚀 Queue processor started");
        
        loop {
            // Get next pending item
            if let Some(mut item) = self.queue.peek_pending().await {
                // Acquire permit for concurrency control
                let _permit = self.limiter.acquire().await;
                
                // Remove from queue and start processing
                item.start_processing();
                if let Err(e) = self.queue.update_item(item.clone()).await {
                    eprintln!("❌ Failed to update item status: {}", e);
                    continue;
                }
                
                // Send progress update
                self.send_progress_update(Some(item.clone())).await;
                
                println!("🎵 Processing: {}", item.display_name());
                
                // Process the item
                let result = self.process_item(&item).await;
                
                // Update item based on result
                match result {
                    Ok(()) => {
                        item.complete();
                        println!("✅ Completed: {}", item.display_name());
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                            item.skip(format!("Duplicate: {}", error_msg));
                            println!("⏭️ Skipped (duplicate): {}", item.display_name());
                        } else {
                            item.fail(error_msg);
                            println!("❌ Failed: {} - {}", item.display_name(), e);
                        }
                    }
                }
                
                // Save updated item
                if let Err(e) = self.queue.update_item(item).await {
                    eprintln!("❌ Failed to update item after processing: {}", e);
                }
                
                // Send final progress update
                self.send_progress_update(None).await;
            } else {
                // No pending items, sleep and check again
                sleep(Duration::from_millis(1000)).await;
                
                // Send status update even when idle
                self.send_progress_update(None).await;
            }
        }
    }
    
    async fn process_item(&self, item: &QueueItem) -> Result<()> {
        match item.item_type.as_str() {
            "spotify_playlist" => {
                self.downloader.process_spotify_url(&item.url).await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
            "spotify_track" => {
                self.downloader.process_spotify_url(&item.url).await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
            "soundcloud_track" => {
                self.downloader.process_soundcloud_url(&item.url).await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
            "youtube_url" => {
                // For now, treat YouTube URLs as song names
                self.downloader.process_song_name(&item.url).await
                    .map_err(|e| anyhow::anyhow!("{}", e))
            }
            "song_name" => {
                self.downloader.process_song_name(&item.url).await
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