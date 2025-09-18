use async_trait::async_trait;
use serde_json;
use tokio::process::Command as TokioCommand;
use crate::utils::smart_limiter::SmartLimiter;

use super::SearchResult;
use crate::MusicDownloadError;

#[async_trait]
pub trait SearchTool: Send + Sync {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, MusicDownloadError>;
}

#[derive(Clone)]
pub struct YouTubeSearchTool;

impl YouTubeSearchTool {
    pub fn new() -> Self {
        Self
    }
    
    async fn search_youtube_with_ytdlp(&self, query: &str) -> Result<Vec<SearchResult>, MusicDownloadError> {
        println!("🔍 Searching YouTube: {}", query);
        
        let output = TokioCommand::new("yt-dlp")
            .arg("--dump-json")
            .arg("--playlist-end")
            .arg("10")  // Limit to top 10 results
            .arg("--no-download")
            .arg(&format!("ytsearch10:{}", query))
            .output()
            .await
            .map_err(|e| MusicDownloadError::Download(format!("Failed to run yt-dlp search: {}", e)))?;
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(MusicDownloadError::Download(format!("yt-dlp search failed: {}", error_msg)));
        }
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();
        
        // Parse each JSON line
        for line in output_str.lines() {
            if line.trim().is_empty() {
                continue;
            }
            
            let json_value: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse yt-dlp JSON: {}", e)))?;
            
            let result = SearchResult {
                id: json_value["id"].as_str().unwrap_or("").to_string(),
                title: json_value["title"].as_str().unwrap_or("").to_string(),
                uploader: json_value["uploader"].as_str().unwrap_or("").to_string(),
                duration: json_value["duration"].as_u64().map(|d| d as u32),
                view_count: json_value["view_count"].as_u64(),
                upload_date: json_value["upload_date"].as_str().map(|s| s.to_string()),
                url: format!("https://youtube.com/watch?v={}", json_value["id"].as_str().unwrap_or("")),
            };
            
            results.push(result);
        }
        
        println!("🔍 Found {} search results", results.len());
        Ok(results)
    }
    
    pub async fn search_multiple(&self, queries: Vec<String>) -> Result<Vec<SearchResult>, MusicDownloadError> {
        use futures::future::join_all;
        
        // Smart limiting for YouTube searches - use half your cores to be nice to YouTube
        let search_limit = (num_cpus::get() / 2).max(2); // At least 2, max half your cores (11 for you)
        let limiter = SmartLimiter::with_limit(search_limit);
        
        println!("🚀 Starting {} YouTube searches with {} concurrent limit", queries.len(), search_limit);
        
        // Create tasks with smart rate limiting
        let mut tasks = Vec::new();
        for query in queries {
            let self_clone = self.clone();
            let limiter_clone = limiter.clone();
            let task = tokio::spawn(async move {
                let _permit = limiter_clone.acquire().await.ok()?;
                self_clone.search(&query).await.ok()
            });
            tasks.push(task);
        }
        
        // Wait for all searches to complete concurrently
        let results = join_all(tasks).await;
        
        // Collect all results
        let mut all_results = Vec::new();
        for result in results {
            if let Ok(Some(search_results)) = result {
                all_results.extend(search_results);
            }
        }
        
        println!("📊 Collected {} total results from YouTube searches", all_results.len());
        
        // Deduplicate results by video ID
        let mut seen_ids = std::collections::HashSet::new();
        let unique_results: Vec<SearchResult> = all_results
            .into_iter()
            .filter(|result| seen_ids.insert(result.id.clone()))
            .collect();
        
        println!("✅ Returning {} unique results after deduplication", unique_results.len());
        Ok(unique_results)
    }
}

#[async_trait]
impl SearchTool for YouTubeSearchTool {
    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, MusicDownloadError> {
        self.search_youtube_with_ytdlp(query).await
    }
}