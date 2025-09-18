// Rig 0.19 Coordinator with Proper Extractors
use std::sync::Arc;
use rig::providers::ollama;
use rig::client::CompletionClient;

use super::{
    SearchContext, SearchIteration, SearchResult, YouTubeSearchTool,
    rig_extractors::{QueryExtractor, ResultExtractor},
    MusicSearchAgent,
};
use async_trait::async_trait;
use crate::MusicDownloadError;

pub struct ExtractorBasedCoordinator {
    query_extractor: Arc<QueryExtractor>,
    result_extractor: Arc<ResultExtractor>,
    youtube_tool: Arc<YouTubeSearchTool>,
    max_iterations: usize,
}

impl ExtractorBasedCoordinator {
    pub fn new(ollama_url: &str, model: &str) -> Self {
        println!("🔗 Creating Ollama client for URL: {} with model: {}", ollama_url, model);
        
        let client = ollama::Client::builder()
            .base_url(ollama_url)
            .build()
            .expect("Failed to create Ollama client");
        
        Self {
            query_extractor: Arc::new(QueryExtractor::new(&client, model)),
            result_extractor: Arc::new(ResultExtractor::new(&client, model)),
            youtube_tool: Arc::new(YouTubeSearchTool::new()),
            max_iterations: 3,
        }
    }
    
    pub async fn search_for_song(&self, song_query: &str) -> Result<SearchResult, MusicDownloadError> {
        println!("🚀 Starting fast single-pass search for: {}", song_query);
        
        let context = SearchContext {
            original_query: song_query.to_string(),
            iterations: Vec::new(),
            max_iterations: 1,
        };
        
        // Generate queries using extractor
        let query_iteration = self.query_extractor.process(&context).await?;
        let queries: Vec<String> = query_iteration.query
            .split(" | ")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        if queries.is_empty() {
            return Err(MusicDownloadError::LLM("No queries generated".to_string()));
        }
        
        // Execute all searches concurrently
        println!("🔍 Generated {} queries, searching YouTube concurrently", queries.len());
        let search_results = self.youtube_tool.search_multiple(queries.clone()).await?;
        
        if search_results.is_empty() {
            return Err(MusicDownloadError::Download("No search results found".to_string()));
        }
        
        println!("📊 Found {} results, analyzing", search_results.len());
        
        // Create context for analysis
        let mut analysis_context = SearchContext {
            original_query: song_query.to_string(),
            iterations: vec![SearchIteration {
                query: queries.join(", "),
                results: search_results.clone(),
                reasoning: String::new(),
                selected_result: None,
                confidence: 0.0,
            }],
            max_iterations: 1,
        };
        
        // Analyze results using extractor
        let analysis = self.result_extractor.process(&analysis_context).await?;
        
        if let Some(result) = analysis.selected_result {
            println!("✅ Selected: {} by {}", result.title, result.uploader);
            Ok(result)
        } else {
            Err(MusicDownloadError::Download(format!("No suitable match found for: {}", song_query)))
        }
    }
}