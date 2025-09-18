// Durable Download Queue with Resume Capability
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use chrono::{DateTime, Utc};
use anyhow::Result;

use crate::agents::SearchResult;
use crate::MusicDownloader;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: String,
    pub song_query: String,
    pub search_result: SearchResult,
    pub status: DownloadStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub retry_count: u32,
    pub output_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DownloadStatus {
    Pending,
    Downloading,
    Completed,
    Failed,
    Retrying,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadHistory {
    pub tasks: HashMap<String, DownloadTask>,
    pub completed_downloads: Vec<String>, // Song URLs that have been successfully downloaded
}

pub struct DownloadQueue {
    queue: Arc<Mutex<VecDeque<DownloadTask>>>,
    history: Arc<Mutex<DownloadHistory>>,
    active_downloads: Arc<Mutex<HashMap<String, DownloadTask>>>,
    downloader: Arc<MusicDownloader>,
    persist_path: PathBuf,
    max_concurrent: usize,
    max_retries: u32,
}

impl DownloadQueue {
    pub fn new(persist_path: PathBuf, downloader: Arc<MusicDownloader>) -> Self {
        let history = Self::load_history(&persist_path).unwrap_or_else(|_| DownloadHistory {
            tasks: HashMap::new(),
            completed_downloads: Vec::new(),
        });
        
        // Restore pending tasks from history
        let mut queue = VecDeque::new();
        for (_, task) in &history.tasks {
            if matches!(task.status, DownloadStatus::Pending | DownloadStatus::Downloading | DownloadStatus::Retrying) {
                let mut restored_task = task.clone();
                restored_task.status = DownloadStatus::Pending; // Reset to pending
                queue.push_back(restored_task);
            }
        }
        
        Self {
            queue: Arc::new(Mutex::new(queue)),
            history: Arc::new(Mutex::new(history)),
            active_downloads: Arc::new(Mutex::new(HashMap::new())),
            downloader,
            persist_path,
            max_concurrent: 3,
            max_retries: 3,
        }
    }
    
    fn load_history(path: &PathBuf) -> Result<DownloadHistory> {
        let history_file = path.join("download_history.json");
        if history_file.exists() {
            let data = std::fs::read_to_string(&history_file)?;
            Ok(serde_json::from_str(&data)?)
        } else {
            Ok(DownloadHistory {
                tasks: HashMap::new(),
                completed_downloads: Vec::new(),
            })
        }
    }
    
    async fn save_history(&self) -> Result<()> {
        let history = self.history.lock().await;
        let history_file = self.persist_path.join("download_history.json");
        
        // Ensure directory exists
        std::fs::create_dir_all(&self.persist_path)?;
        
        let data = serde_json::to_string_pretty(&*history)?;
        std::fs::write(&history_file, data)?;
        Ok(())
    }
    
    pub async fn add_task(&self, song_query: String, search_result: SearchResult) -> Result<String> {
        let mut history = self.history.lock().await;
        
        // Check if already downloaded
        if history.completed_downloads.contains(&search_result.url) {
            println!("✅ Song already in download history: {}", search_result.title);
            return Ok("already_downloaded".to_string());
        }
        
        let task = DownloadTask {
            id: uuid::Uuid::new_v4().to_string(),
            song_query,
            search_result,
            status: DownloadStatus::Pending,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error: None,
            retry_count: 0,
            output_path: None,
        };
        
        let task_id = task.id.clone();
        
        // Add to history
        history.tasks.insert(task_id.clone(), task.clone());
        drop(history); // Release lock before saving
        
        // Add to queue
        let mut queue = self.queue.lock().await;
        queue.push_back(task);
        drop(queue);
        
        // Save state
        self.save_history().await?;
        
        Ok(task_id)
    }
    
    pub async fn start_processing(&self) -> mpsc::Receiver<DownloadTask> {
        let (tx, rx) = mpsc::channel(100);
        
        // Spawn processing tasks
        for _ in 0..self.max_concurrent {
            let queue = Arc::clone(&self.queue);
            let history = Arc::clone(&self.history);
            let active = Arc::clone(&self.active_downloads);
            let downloader = Arc::clone(&self.downloader);
            let tx = tx.clone();
            let persist_path = self.persist_path.clone();
            let max_retries = self.max_retries;
            
            tokio::spawn(async move {
                loop {
                    // Get next task
                    let task = {
                        let mut q = queue.lock().await;
                        q.pop_front()
                    };
                    
                    if let Some(mut task) = task {
                        // Update status
                        task.status = DownloadStatus::Downloading;
                        task.started_at = Some(Utc::now());
                        
                        // Add to active downloads
                        active.lock().await.insert(task.id.clone(), task.clone());
                        
                        // Process download
                        println!("📥 Downloading: {} by {}", task.search_result.title, task.search_result.uploader);
                        
                        match downloader.download_from_youtube(&task.search_result.url).await {
                            Ok(()) => {
                                task.status = DownloadStatus::Completed;
                                task.completed_at = Some(Utc::now());
                                task.output_path = Some(PathBuf::from("downloaded")); // TODO: Get actual path
                                
                                // Update history
                                let mut hist = history.lock().await;
                                hist.tasks.insert(task.id.clone(), task.clone());
                                hist.completed_downloads.push(task.search_result.url.clone());
                                drop(hist);
                                
                                // Save state
                                if let Ok(hist) = history.lock().await.clone().try_into() {
                                    let _ = Self::save_history_static(&persist_path, hist).await;
                                }
                                
                                println!("✅ Downloaded: {}", task.search_result.title);
                            }
                            Err(e) => {
                                task.error = Some(e.to_string());
                                task.retry_count += 1;
                                
                                if task.retry_count < max_retries {
                                    task.status = DownloadStatus::Retrying;
                                    println!("🔄 Retrying download ({}/{}): {}", task.retry_count, max_retries, task.search_result.title);
                                    
                                    // Re-queue for retry
                                    queue.lock().await.push_back(task.clone());
                                } else {
                                    task.status = DownloadStatus::Failed;
                                    task.completed_at = Some(Utc::now());
                                    println!("❌ Failed to download after {} retries: {}", max_retries, task.search_result.title);
                                }
                                
                                // Update history
                                let mut hist = history.lock().await;
                                hist.tasks.insert(task.id.clone(), task.clone());
                                drop(hist);
                            }
                        }
                        
                        // Remove from active
                        active.lock().await.remove(&task.id);
                        
                        // Send update
                        let _ = tx.send(task).await;
                    } else {
                        // No tasks, wait a bit
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    }
                }
            });
        }
        
        rx
    }
    
    async fn save_history_static(path: &PathBuf, history: DownloadHistory) -> Result<()> {
        let history_file = path.join("download_history.json");
        std::fs::create_dir_all(path)?;
        let data = serde_json::to_string_pretty(&history)?;
        std::fs::write(&history_file, data)?;
        Ok(())
    }
    
    pub async fn get_status(&self) -> (Vec<DownloadTask>, Vec<DownloadTask>, Vec<DownloadTask>) {
        let queue = self.queue.lock().await;
        let active = self.active_downloads.lock().await;
        let history = self.history.lock().await;
        
        let pending: Vec<_> = queue.iter().cloned().collect();
        let downloading: Vec<_> = active.values().cloned().collect();
        let completed: Vec<_> = history.tasks.values()
            .filter(|t| matches!(t.status, DownloadStatus::Completed | DownloadStatus::Failed))
            .cloned()
            .collect();
            
        (pending, downloading, completed)
    }
    
    pub async fn clear_completed(&self) -> Result<()> {
        let mut history = self.history.lock().await;
        history.tasks.retain(|_, task| !matches!(task.status, DownloadStatus::Completed));
        drop(history);
        self.save_history().await
    }
    
    pub async fn retry_failed(&self) -> Result<usize> {
        let mut history = self.history.lock().await;
        let mut queue = self.queue.lock().await;
        
        let mut retry_count = 0;
        for (_, task) in history.tasks.iter_mut() {
            if matches!(task.status, DownloadStatus::Failed) {
                task.status = DownloadStatus::Pending;
                task.retry_count = 0;
                task.error = None;
                queue.push_back(task.clone());
                retry_count += 1;
            }
        }
        
        drop(history);
        drop(queue);
        
        self.save_history().await?;
        Ok(retry_count)
    }
}