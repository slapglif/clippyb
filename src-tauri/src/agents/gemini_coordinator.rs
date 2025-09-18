// Gemini-based Coordinator for ClippyB
use std::sync::Arc;
use rig::providers::gemini;
use rig::completion::Prompt;
use rig::client::{CompletionClient, ProviderClient};
use serde::{Serialize, Deserialize};
use schemars::JsonSchema;

use super::{
    SearchContext, SearchIteration, SearchResult, YouTubeSearchTool,
    MusicSearchAgent,
};
use crate::MusicDownloadError;

// Schema for query extraction
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct QueryList {
    #[schemars(description = "List of YouTube search queries to find the song")]
    queries: Vec<String>,
}

// Schema for result analysis
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct ResultAnalysis {
    #[schemars(description = "The search query that was used")]
    query: String,
    #[schemars(description = "Reasoning for the selection or why no match was found")]
    reasoning: String,
    #[schemars(description = "Index of the selected result (-1 if no good match)")]
    selected_result_index: i32,
    #[schemars(description = "Confidence score between 0.0 and 1.0")]
    confidence: f64,
}

pub struct GeminiCoordinator {
    client: gemini::Client,
    model: String,
    youtube_tool: Arc<YouTubeSearchTool>,
    max_iterations: usize,
}

impl GeminiCoordinator {
    pub fn new(api_key: &str, model: &str) -> Self {
        println!("🔗 Creating Gemini client with model: {}", model);
        
        // Set the API key in environment for rig to use
        std::env::set_var("GEMINI_API_KEY", api_key);
        let client = gemini::Client::from_env();
        
        Self {
            client,
            model: model.to_string(),
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
        
        for iteration in 0..1 {
            println!("🚀 DEPRECATED: Using old GeminiCoordinator - switch to GeminiDirectCoordinator for single-pass!");
            
            // Generate queries using Gemini
            let query_iteration = self.generate_queries(&context).await?;
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
            
            // Analyze results using Gemini
            let analysis = self.analyze_results(&context.original_query, &search_results).await?;
            
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
    
    async fn generate_queries(&self, context: &SearchContext) -> Result<SearchIteration, MusicDownloadError> {
        let is_refinement = !context.iterations.is_empty();
        
        let input_text = if is_refinement {
            let previous = context.iterations
                .iter()
                .map(|iter| format!("Tried: {} ({})", iter.query, iter.reasoning))
                .collect::<Vec<_>>()
                .join("\n");
                
            format!(
                "Find song: {}\n\nPrevious attempts:\n{}\n\nGenerate NEW search queries with different approaches.",
                context.original_query, previous
            )
        } else {
            format!("Find this song on YouTube: {}", context.original_query)
        };
        
        println!("🔍 DEBUG: About to call Gemini with input: '{}'", input_text);
        println!("🔍 DEBUG: Model: {}", self.model);
        
        // Use regular completion instead of extractor
        let agent = self.client
            .agent(&self.model)
            .preamble("You are a music search expert. Generate effective YouTube search queries for the given song.")
            .temperature(0.3)
            .build();
            
        let prompt = format!(
            "{}\n\nReturn ONLY valid JSON in exactly this format: {{\"queries\": [\"query1\", \"query2\", \"query3\"]}}. Include 2-3 search query strings.",
            input_text
        );
        
        let response = agent
            .prompt(&prompt)
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Gemini completion error: {}", e)))?;
        
        // The response is already a String
        let response_text = response;
        
        println!("🔍 DEBUG: Gemini response: {}", response_text);
        
        // Parse JSON response
        let result: QueryList = serde_json::from_str(&response_text)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse Gemini response: {} - Response: {}", e, response_text)))?;
            
        Ok(SearchIteration {
            query: result.queries.join(" | "),
            results: Vec::new(),
            reasoning: format!("Generated {} search queries", result.queries.len()),
            selected_result: None,
            confidence: 0.0,
        })
    }
    
    async fn analyze_results(
        &self,
        original_query: &str,
        results: &[SearchResult],
    ) -> Result<SearchIteration, MusicDownloadError> {
        let results_text = results
            .iter()
            .take(10)
            .enumerate()
            .map(|(i, r)| {
                format!(
                    "{}. {} by {} ({}s, {} views)",
                    i,
                    r.title,
                    r.uploader,
                    r.duration.unwrap_or(0),
                    r.view_count.unwrap_or(0)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
            
        let input = format!(
            "Find the best match for: {}\n\nResults:\n{}",
            original_query, results_text
        );
        
        println!("🔍 DEBUG: Result analysis - About to call Gemini with input: '{}'", input);
        
        let agent = self.client
            .agent(&self.model)
            .preamble("You are a music search result analyzer. Select the best match for the requested song.")
            .temperature(0.3)
            .build();
            
        let prompt = format!(
            "{}\n\nReturn ONLY valid JSON in exactly this format: {{\"query\": \"search query\", \"reasoning\": \"explanation\", \"selected_result_index\": 0, \"confidence\": 0.8}}. Use -1 for selected_result_index if no good match.",
            input
        );
        
        let response = agent
            .prompt(&prompt)
            .await
            .map_err(|e| {
                println!("🔍 DEBUG: Result analysis - Full error details: {:#?}", e);
                MusicDownloadError::LLM(format!("Result analysis error: {} | Query: '{}' | {} results | Model: {}", e, original_query, results.len(), self.model))
            })?;
        
        // The response is already a String
        let response_text = response;
        
        println!("🔍 DEBUG: Result analysis - Gemini response: {}", response_text);
        
        // Parse JSON response
        let analysis: ResultAnalysis = serde_json::from_str(&response_text)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse analysis response: {} - Response: {}", e, response_text)))?;
            
        let selected = if analysis.selected_result_index >= 0 
            && (analysis.selected_result_index as usize) < results.len() {
            Some(results[analysis.selected_result_index as usize].clone())
        } else {
            None
        };
        
        Ok(SearchIteration {
            query: analysis.query,
            results: results.to_vec(),
            reasoning: analysis.reasoning,
            selected_result: selected,
            confidence: analysis.confidence as f32,
        })
    }
}