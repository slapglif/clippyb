// Rig 0.19 implementation using Extractors for structured data
use async_trait::async_trait;
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::{CompletionModel, Completion},
    providers::ollama,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{MusicSearchAgent, SearchContext, SearchIteration, SearchResult};
use crate::MusicDownloadError;

// Structured data types for extraction
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct QueryList {
    queries: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct AnalysisResult {
    query: String,
    reasoning: String,
    selected_result_index: i32,
    confidence: f32,
}

pub struct RigQueryGenerator {
    extractor: rig::extractor::Extractor<ollama::CompletionModel, QueryList>,
}

impl RigQueryGenerator {
    pub fn new(client: &ollama::Client, model_name: &str) -> Self {
        let extractor = client.extractor::<QueryList>(model_name)
            .preamble("You are a music search query generator. When given a song name, generate effective YouTube search queries.")
            .build();
            
        Self { extractor }
    }
    
    async fn generate_queries(&self, song_query: &str, is_refinement: bool, context: Option<&SearchContext>) -> Result<Vec<String>, MusicDownloadError> {
        let input_text = if is_refinement && context.is_some() {
            let previous_context = context.unwrap().iterations
                .iter()
                .map(|iter| format!("Previous query: {} ({})", iter.query, iter.reasoning))
                .collect::<Vec<_>>()
                .join("; ");
            
            format!("Song: {}. Previous attempts: {}. Generate new search variations.", song_query, previous_context)
        } else {
            format!("Song to search on YouTube: {}", song_query)
        };
        
        // The extractor will handle prompting and structured extraction
        let result = self.extractor.extract(&input_text).await
            .map_err(|e| MusicDownloadError::LLM(format!("Query extractor error: {} | Input: '{}'", e, input_text.chars().take(200).collect::<String>())))?;
        
        Ok(result.queries)
    }
}

#[async_trait]
impl MusicSearchAgent for RigQueryGenerator {
    async fn process(&self, context: &SearchContext) -> Result<SearchIteration, MusicDownloadError> {
        let is_refinement = !context.iterations.is_empty();
        let queries = self.generate_queries(&context.original_query, is_refinement, Some(context)).await?;
        
        println!("🔍 Generated {} search queries", queries.len());
        for (i, query) in queries.iter().enumerate() {
            println!("  {}. {}", i + 1, query);
        }
        
        Ok(SearchIteration {
            query: queries.join(" | "),
            results: Vec::new(),
            reasoning: format!("Generated {} search queries", queries.len()),
            selected_result: None,
            confidence: 0.0,
        })
    }
}

pub struct RigResultAnalyzer {
    extractor: rig::extractor::Extractor<ollama::CompletionModel, AnalysisResult>,
}

impl RigResultAnalyzer {
    pub fn new(client: &ollama::Client, model_name: &str) -> Self {
        let extractor = client.extractor::<AnalysisResult>(model_name)
            .preamble("You are a music search result analyzer. Extract analysis of YouTube search results.")
            .build();
            
        Self { extractor }
    }
    
    async fn analyze_results(&self, query: &str, results: &[super::SearchResult], original_query: &str) -> Result<AnalysisResult, MusicDownloadError> {
        let results_text = results.iter().enumerate()
            .map(|(i, r)| format!("{}: {} - {} ({})", i, r.title, r.uploader, r.url))
            .collect::<Vec<_>>()
            .join("\n");
        
        let prompt = format!(
            r#"Analyze these YouTube search results for the song: "{}"

Results:
{}

Prioritize:
- Exact artist and title matches
- Official artist channels
- High-quality audio sources
- Avoid covers, karaoke, live performances (unless requested)

If no good match found, explain why and set selected_result_index to -1."#,
            original_query, results_text
        );
        
        // Use the extractor to get structured data
        self.extractor.extract(&prompt).await
            .map_err(|e| MusicDownloadError::LLM(format!("Analysis extractor error: {} | Query: '{}' | {} results", e, original_query, results.len())))
    }
}

#[async_trait]
impl MusicSearchAgent for RigResultAnalyzer {
    async fn process(&self, context: &SearchContext) -> Result<SearchIteration, MusicDownloadError> {
        let last_iteration = context.iterations.last()
            .ok_or_else(|| MusicDownloadError::Agent("No previous iteration found".to_string()))?;
        
        // Results are already in the correct format
        let results = &last_iteration.results;
        
        let analysis = self.analyze_results(&last_iteration.query, &results, &context.original_query).await?;
        
        let selected_result = if analysis.selected_result_index >= 0 && (analysis.selected_result_index as usize) < results.len() {
            Some(results[analysis.selected_result_index as usize].clone())
        } else {
            None
        };
        
        println!("✨ Analysis complete: {}", analysis.reasoning);
        if let Some(ref result) = selected_result {
            println!("✅ Selected: {} - {}", result.title, result.uploader);
        } else {
            println!("❌ No suitable result found");
        }
        
        Ok(SearchIteration {
            query: last_iteration.query.clone(),
            results: last_iteration.results.clone(),
            reasoning: analysis.reasoning,
            selected_result,
            confidence: analysis.confidence,
        })
    }
}