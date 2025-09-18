// REAL Rig 0.19 implementation - NO PLACEHOLDERS
use async_trait::async_trait;
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::{CompletionModel, Completion},
    providers::ollama,
};
use serde::{Deserialize, Serialize};
use serde_json;

use super::{MusicSearchAgent, SearchContext, SearchIteration, SearchResult};
use crate::MusicDownloadError;
use crate::utils::llm_utils::sanitize_llm_json_response;

pub struct RigQueryGenerator {
    model: Agent<ollama::CompletionModel>,
}

impl RigQueryGenerator {
    pub fn new(client: &ollama::Client, model_name: &str) -> Self {
        let model = client.agent(model_name)
            .preamble("You are a music search query generator expert. Generate effective YouTube search queries to find specific songs. Always respond with a JSON array of search query strings.")
            .build();
            
        Self { model }
    }
    
    async fn generate_queries(&self, song_query: &str, is_refinement: bool, context: Option<&SearchContext>) -> Result<Vec<String>, MusicDownloadError> {
        let prompt = if is_refinement && context.is_some() {
            let ctx = context.unwrap();
            let previous_context = ctx.iterations
                .iter()
                .map(|iter| format!("Query: {} | Reasoning: {}", iter.query, iter.reasoning))
                .collect::<Vec<_>>()
                .join("\n");
            
            format!(
                r#"Generate 2-3 NEW refined YouTube search queries for: '{}'

Previous attempts:
{}

Return ONLY a JSON array of search query strings. No markdown, no code blocks, just the array.

Try different approaches:
- More specific terms
- Different word order
- Add year, genre, or album info
- Try alternate artist/song spellings
- Focus on official sources"#,
                song_query, previous_context
            )
        } else {
            format!(
                r#"Generate 3-4 different YouTube search queries to find this exact song: '{}'

Output format: JSON array only. Example:
["query 1", "query 2", "query 3"]

Generate variations like:
- Exact artist and song name
- With "official" or "music video"
- Alternative spellings or formats
- Without extra words that might confuse search

Example for "Never Gonna Give You Up - Rick Astley":
["Rick Astley Never Gonna Give You Up", "Rick Astley Never Gonna Give You Up official", "Never Gonna Give You Up Rick Astley music video"]"#,
                song_query
            )
        };
        
        let response = self.model.completion(&prompt, vec![])
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Rig error: {}", e)))?
            .send()
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Rig completion error: {}", e)))?;
            
        // Extract text from OneOrMany<AssistantContent>
        let response_text = match response.choice.into_iter().next() {
            Some(rig::completion::AssistantContent::Text(text)) => text.text,
            _ => return Err(MusicDownloadError::LLM("Unexpected response format".to_string())),
        };
        // Sanitize the response to extract pure JSON
        let sanitized_response = sanitize_llm_json_response(&response_text);
        
        let queries: Vec<String> = serde_json::from_str(&sanitized_response)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse queries: {} - Response: {}", e, response_text)))?;
        
        Ok(queries)
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

#[derive(Debug, Serialize, Deserialize)]
struct AnalysisResult {
    query: String,
    reasoning: String,
    selected_result_index: i32,
    confidence: f64,
}

pub struct RigResultAnalyzer {
    model: Agent<ollama::CompletionModel>,
}

impl RigResultAnalyzer {
    pub fn new(client: &ollama::Client, model_name: &str) -> Self {
        let model = client.agent(model_name)
            .preamble("You are a music search result analyzer expert. Analyze YouTube search results and identify the best match for a given song. Always respond with a JSON object containing your analysis.")
            .build();
            
        Self { model }
    }
    
    pub async fn analyze(
        &self,
        original_query: &str,
        results: &[SearchResult],
        previous_iterations: &[SearchIteration],
    ) -> Result<SearchIteration, MusicDownloadError> {
        let results_summary = results
            .iter()
            .take(10)
            .enumerate()
            .map(|(i, result)| {
                format!(
                    "{}. Title: \"{}\" | Uploader: {} | Duration: {}s | Views: {} | URL: {}",
                    i + 1,
                    result.title,
                    result.uploader,
                    result.duration.map(|d| d.to_string()).unwrap_or("N/A".to_string()),
                    result.view_count.map(|v| v.to_string()).unwrap_or("N/A".to_string()),
                    result.url
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        
        let previous_context = if !previous_iterations.is_empty() {
            format!(
                "\nPrevious iterations:\n{}",
                previous_iterations
                    .iter()
                    .map(|iter| format!("- {}: {}", iter.query, iter.reasoning))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        } else {
            String::new()
        };
        
        let prompt = format!(
            r#"Analyze these YouTube search results for the song: "{}"

Results:
{}
{}

Output format: JSON object only. Example:
{{
  "query": "search query used",
  "reasoning": "why this result was selected or why no good match was found",
  "selected_result_index": N,
  "confidence": 0.XX
}}

Prioritize:
1. Official artist/label uploads
2. Exact title match
3. High view count
4. Normal song duration (2-5 min)

Set index to -1 if no good match found.
Set confidence between 0.0 and 1.0 based on how sure you are."#,
            original_query, results_summary, previous_context
        );
        
        let response = self.model.completion(&prompt, vec![])
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Rig analysis error: {}", e)))?
            .send()
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Rig completion error: {}", e)))?;
            
        // Extract text from OneOrMany<AssistantContent>
        let response_text = match response.choice.into_iter().next() {
            Some(rig::completion::AssistantContent::Text(text)) => text.text,
            _ => return Err(MusicDownloadError::LLM("Unexpected response format".to_string())),
        };
        // Sanitize the response to extract pure JSON
        let sanitized_response = sanitize_llm_json_response(&response_text);
        
        let analysis: AnalysisResult = serde_json::from_str(&sanitized_response)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse analysis: {} - Response: {}", e, response_text)))?;
        
        let selected_result = if analysis.selected_result_index >= 0 && (analysis.selected_result_index as usize) < results.len() {
            Some(results[analysis.selected_result_index as usize].clone())
        } else {
            None
        };
        
        Ok(SearchIteration {
            query: analysis.query,
            results: results.to_vec(),
            reasoning: analysis.reasoning,
            selected_result,
            confidence: analysis.confidence as f32,
        })
    }
}

#[async_trait]
impl MusicSearchAgent for RigResultAnalyzer {
    async fn process(&self, context: &SearchContext) -> Result<SearchIteration, MusicDownloadError> {
        if let Some(last_iteration) = context.iterations.last() {
            if !last_iteration.results.is_empty() {
                return self.analyze(
                    &context.original_query,
                    &last_iteration.results,
                    &context.iterations[..context.iterations.len()-1],
                ).await;
            }
        }
        
        Err(MusicDownloadError::LLM("No search results to analyze".to_string()))
    }
}