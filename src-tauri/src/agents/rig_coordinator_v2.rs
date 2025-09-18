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
        let mut context = SearchContext {
            original_query: song_query.to_string(),
            iterations: Vec::new(),
            max_iterations: self.max_iterations,
        };
        
        for iteration in 0..self.max_iterations {
            println!("🤔 Iteration {}/{}", iteration + 1, self.max_iterations);
            
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
            
            // Execute searches
            println!("🔍 Searching with {} queries", queries.len());
            let search_results = self.youtube_tool.search_multiple(queries.clone()).await?;
            
            if search_results.is_empty() {
                context.iterations.push(SearchIteration {
                    query: queries.join(", "),
                    results: Vec::new(),
                    reasoning: "No results found".to_string(),
                    selected_result: None,
                    confidence: 0.0,
                });
                continue;
            }
            
            // Update context
            context.iterations.push(SearchIteration {
                query: queries.join(", "),
                results: search_results.clone(),
                reasoning: String::new(),
                selected_result: None,
                confidence: 0.0,
            });
            
            // Analyze results using extractor
            let analysis = self.result_extractor.process(&context).await?;
            
            println!("📝 Reasoning: {}", analysis.reasoning);
            println!("🎯 Confidence: {:.1}%", analysis.confidence * 100.0);
            
            // Update iteration with analysis
            if let Some(last) = context.iterations.last_mut() {
                last.reasoning = analysis.reasoning.clone();
                last.selected_result = analysis.selected_result.clone();
                last.confidence = analysis.confidence;
            }
            
            // Return if confident
            if let Some(result) = &analysis.selected_result {
                if analysis.confidence > 0.5 || iteration == self.max_iterations - 1 {
                    println!("✅ Selected: {} by {}", result.title, result.uploader);
                    return Ok(result.clone());
                }
            }
        }
        
        // Return best result
        context.iterations
            .iter()
            .filter_map(|iter| {
                iter.selected_result.as_ref()
                    .map(|result| (result.clone(), iter.confidence))
            })
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(result, _)| result)
            .ok_or_else(|| MusicDownloadError::Download(
                format!("No suitable match found for: {}", song_query)
            ))
    }
}