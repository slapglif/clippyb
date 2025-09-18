// Proper Rig 0.19 Extractor Implementation
use rig::{
    extractor::{Extractor, ExtractorBuilder},
    providers::ollama,
    client::CompletionClient,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{SearchResult, MusicSearchAgent, SearchContext, SearchIteration};
use crate::MusicDownloadError;
use async_trait::async_trait;

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

pub struct QueryExtractor {
    pub client: ollama::Client,
    pub model_name: String,
}

impl QueryExtractor {
    pub fn new(client: &ollama::Client, model_name: &str) -> Self {
        Self { 
            client: client.clone(),
            model_name: model_name.to_string(),
        }
    }
}

#[async_trait]
impl MusicSearchAgent for QueryExtractor {
    async fn process(&self, context: &SearchContext) -> Result<SearchIteration, MusicDownloadError> {
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
        
        // Create extractor with Ollama JSON format parameter and schema
        use schemars::schema_for;
        let schema = schema_for!(QueryList);
        let format_param = serde_json::json!({
            "format": schema
        });
        
        println!("🔍 DEBUG: About to call extractor.extract() with input: '{}'", input_text);
        println!("🔍 DEBUG: Model: {}", self.model_name);
        println!("🔍 DEBUG: Format param: {}", serde_json::to_string_pretty(&format_param).unwrap_or_default());
        
        let extractor = self.client
            .extractor::<QueryList>(&self.model_name)
            .preamble("You are a music search expert. Generate effective YouTube search queries for the given song. You MUST return valid JSON in exactly this format: {\"queries\": [\"query1\", \"query2\", \"query3\"]}. Include 2-3 search query strings.")
            .additional_params(format_param)
            .build();
        
        // Let's panic to see the full stack trace
        let result = extractor
            .extract(&input_text)
            .await;
            
        println!("🔍 DEBUG: Raw extractor result: {:?}", result);
        
        let result = match result {
            Ok(data) => {
                println!("🔍 DEBUG: SUCCESS - Got data: {:?}", data);
                data
            }
            Err(e) => {
                println!("🔍 DEBUG: ERROR - Full error details: {:#?}", e);
                println!("🔍 DEBUG: ERROR - Error source chain:");
                let mut current_error: &dyn std::error::Error = &e;
                let mut level = 0;
                loop {
                    println!("🔍 DEBUG: ERROR [{}]: {}", level, current_error);
                    match current_error.source() {
                        Some(source) => {
                            current_error = source;
                            level += 1;
                        }
                        None => break,
                    }
                }
                // PANIC to see full stack trace
                panic!("DEBUGGING: Rig extractor failed with NoData error. Full error: {:#?}", e);
            }
        };
            
        Ok(SearchIteration {
            query: result.queries.join(" | "),
            results: Vec::new(),
            reasoning: format!("Generated {} search queries", result.queries.len()),
            selected_result: None,
            confidence: 0.0,
        })
    }
}

pub struct ResultExtractor {
    client: ollama::Client,
    model_name: String,
}

impl ResultExtractor {
    pub fn new(client: &ollama::Client, model_name: &str) -> Self {
        Self { 
            client: client.clone(),
            model_name: model_name.to_string(),
        }
    }
    
    pub async fn analyze(
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
        
        // Create extractor with Ollama JSON format parameter and schema
        use schemars::schema_for;
        let schema = schema_for!(ResultAnalysis);
        let format_param = serde_json::json!({
            "format": schema
        });
        
        println!("🔍 DEBUG: Result analysis - About to call extractor.extract() with input: '{}'", input);
        println!("🔍 DEBUG: Result analysis - Model: {}", self.model_name);
        println!("🔍 DEBUG: Result analysis - Format param: {}", serde_json::to_string_pretty(&format_param).unwrap_or_default());
        
        let extractor = self.client
            .extractor::<ResultAnalysis>(&self.model_name)
            .preamble("You are a music search result analyzer. Select the best match for the requested song. You MUST return valid JSON in exactly this format: {\"query\": \"search query\", \"reasoning\": \"explanation\", \"selected_result_index\": 0, \"confidence\": 0.8}. Use -1 for selected_result_index if no good match.")
            .additional_params(format_param)
            .build();
        
        let analysis = extractor
            .extract(&input)
            .await;
            
        println!("🔍 DEBUG: Result analysis - Raw extractor result: {:?}", analysis);
        
        let analysis = analysis.map_err(|e| {
            println!("🔍 DEBUG: Result analysis - Full error details: {:#?}", e);
            MusicDownloadError::LLM(format!("Result analysis error: {:#?} | Query: '{}' | {} results | Model: {}", e, original_query, results.len(), self.model_name))
        })?;
            
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

#[async_trait]
impl MusicSearchAgent for ResultExtractor {
    async fn process(&self, context: &SearchContext) -> Result<SearchIteration, MusicDownloadError> {
        if let Some(last) = context.iterations.last() {
            if !last.results.is_empty() {
                return self.analyze(&context.original_query, &last.results).await;
            }
        }
        
        Err(MusicDownloadError::LLM("No results to analyze".to_string()))
    }
}