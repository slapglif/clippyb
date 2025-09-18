use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum QueueStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped, // For duplicates
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: String,
    pub url: String,
    pub item_type: String, // "spotify_playlist", "spotify_track", "soundcloud_track", etc.
    pub status: QueueStatus,
    pub created_at: u64,
    pub started_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub error_message: Option<String>,
    pub retry_count: u32,
    pub metadata: Option<QueueItemMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItemMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub playlist_name: Option<String>,
    pub total_tracks: Option<usize>, // For playlists
    pub track_index: Option<usize>,  // For individual tracks in playlists
}

impl QueueItem {
    pub fn new(url: String, item_type: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
            
        Self {
            id: Uuid::new_v4().to_string(),
            url,
            item_type,
            status: QueueStatus::Pending,
            created_at: now,
            started_at: None,
            completed_at: None,
            error_message: None,
            retry_count: 0,
            metadata: None,
        }
    }
    
    pub fn with_metadata(mut self, metadata: QueueItemMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
    
    pub fn start_processing(&mut self) {
        self.status = QueueStatus::InProgress;
        self.started_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
    
    pub fn complete(&mut self) {
        self.status = QueueStatus::Completed;
        self.completed_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
    
    pub fn fail(&mut self, error: String) {
        self.status = QueueStatus::Failed;
        self.error_message = Some(error);
        self.retry_count += 1;
        self.completed_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
    
    pub fn skip(&mut self, reason: String) {
        self.status = QueueStatus::Skipped;
        self.error_message = Some(reason);
        self.completed_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
    }
    
    pub fn reset_for_retry(&mut self) {
        self.status = QueueStatus::Pending;
        self.started_at = None;
        self.completed_at = None;
        self.error_message = None;
    }
    
    pub fn display_name(&self) -> String {
        if let Some(metadata) = &self.metadata {
            if let (Some(artist), Some(title)) = (&metadata.artist, &metadata.title) {
                return format!("{} - {}", artist, title);
            }
            if let Some(title) = &metadata.title {
                return title.clone();
            }
            if let Some(playlist) = &metadata.playlist_name {
                return format!("Playlist: {}", playlist);
            }
        }
        
        // Fallback to URL
        if self.url.len() > 50 {
            format!("{}...", &self.url[..47])
        } else {
            self.url.clone()
        }
    }
}