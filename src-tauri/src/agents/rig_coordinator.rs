// REAL Rig 0.19 ReAct Coordinator - NO PLACEHOLDERS
use std::sync::Arc;
use rig::providers::ollama;

use super::{
    SearchContext, SearchIteration, SearchResult, YouTubeSearchTool,
    rig_agents::{RigQueryGenerator, RigResultAnalyzer},
    MusicSearchAgent,
};
use crate::MusicDownloadError;

pub struct RigMusicSearchCoordinator {
    query_generator: Arc<RigQueryGenerator>,
    result_analyzer: Arc<RigResultAnalyzer>,
    youtube_tool: Arc<YouTubeSearchTool>,
    max_iterations: usize,
}

impl RigMusicSearchCoordinator {
    pub fn new(ollama_url: &str, model: &str) -> Self {
        let client = ollama::Client::builder()
            .base_url(ollama_url)
            .build()
            .expect("Failed to create Ollama client");
        
        Self {
            query_generator: Arc::new(RigQueryGenerator::new(&client, model)),
            result_analyzer: Arc::new(RigResultAnalyzer::new(&client, model)),
            youtube_tool: Arc::new(YouTubeSearchTool::new()),
            max_iterations: 3,
        }
    }
    
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }
    
    pub async fn search_for_song(&self, song_query: &str) -> Result<SearchResult, MusicDownloadError> {
        let mut context = SearchContext {
            original_query: song_query.to_string(),
            iterations: Vec::new(),
            max_iterations: self.max_iterations,
        };
        
        for iteration in 0..1 {
            println!("🚀 DEPRECATED: Using old RigMusicSearchCoordinator - switch to ExtractorBasedCoordinator for single-pass!");
            
            // Step 1: Generate search queries using Rig
            let query_iteration = self.query_generator.process(&context).await?;
            let queries: Vec<String> = query_iteration.query
                .split(" | ")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            
            if queries.is_empty() {
                return Err(MusicDownloadError::LLM("No queries generated".to_string()));
            }
            
            // Step 2: Execute searches with YouTube tool
            let query_str = queries.join(", ");
            let search_results = self.youtube_tool.search_multiple(queries).await?;
            
            if search_results.is_empty() {
                // Try to generate new queries in the next iteration
                context.iterations.push(SearchIteration {
                    query: query_str,
                    results: Vec::new(),
                    reasoning: "No results found for these queries".to_string(),
                    selected_result: None,
                    confidence: 0.0,
                });
                continue;
            }
            
            // Update context with search results
            context.iterations.push(SearchIteration {
                query: query_str,
                results: search_results.clone(),
                reasoning: String::new(),
                selected_result: None,
                confidence: 0.0,
            });
            
            // Step 3: Analyze results using Rig
            let analysis = self.result_analyzer.process(&context).await?;
            
            println!("📝 Reasoning: {}", analysis.reasoning);
            println!("🎯 Confidence: {:.1}%", analysis.confidence * 100.0);
            
            // Update the last iteration with analysis results
            if let Some(last) = context.iterations.last_mut() {
                last.reasoning = analysis.reasoning.clone();
                last.selected_result = analysis.selected_result.clone();
                last.confidence = analysis.confidence;
            }
            
            // Check if we have a high-confidence result
            if let Some(result) = &analysis.selected_result {
                if analysis.confidence > 0.5 || iteration == self.max_iterations - 1 {
                    println!("✅ Selected: {} by {}", result.title, result.uploader);
                    return Ok(result.clone());
                }
            }
            
            println!("🔄 Confidence too low ({:.1}%), refining search...", analysis.confidence * 100.0);
        }
        
        // Fallback: return the best result from all iterations
        let best_result = context.iterations
            .iter()
            .filter_map(|iter| {
                iter.selected_result.as_ref().map(|result| (result, iter.confidence))
            })
            .max_by(|(_, conf_a), (_, conf_b)| conf_a.partial_cmp(conf_b).unwrap())
            .map(|(result, _)| result);
        
        if let Some(result) = best_result {
            println!("⚠️ Returning best available result after {} iterations", self.max_iterations);
            Ok(result.clone())
        } else {
            Err(MusicDownloadError::Download(
                format!("Could not find suitable match for: {}", song_query)
            ))
        }
    }
}