// Rig 0.19 implementation using Pipeline and Extractors
use rig::{
    pipeline::{self, Op},
    providers::ollama,
    client::CompletionClient,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::MusicDownloadError;

// Structured data types for extraction
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct QueryList {
    queries: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct AnalysisResult {
    query: String,
    reasoning: String,
    selected_result_index: i32,
    confidence: f32,
}

pub struct MusicSearchPipeline {
    client: ollama::Client,
    model_name: String,
}

impl MusicSearchPipeline {
    pub fn new(ollama_url: &str, model: &str) -> Self {
        let client = ollama::Client::builder()
            .base_url(ollama_url)
            .build()
            .expect("Failed to create Ollama client");
            
        Self {
            client,
            model_name: model.to_string(),
        }
    }
    
    pub async fn generate_queries(&self, song_query: &str) -> Result<Vec<String>, MusicDownloadError> {
        let prompt = format!(
            r#"Generate 3-4 different YouTube search queries to find this exact song: '{}'

Generate variations like:
- Exact artist and song name
- With "official" or "music video"
- Alternative spellings or formats
- Without extra words that might confuse search

Example for "Never Gonna Give You Up - Rick Astley":
["Rick Astley Never Gonna Give You Up", "Rick Astley Never Gonna Give You Up official", "Never Gonna Give You Up Rick Astley music video"]"#,
            song_query
        );
        
        // Create a pipeline that extracts structured data
        let pipeline = pipeline::new()
            .extract::<_, _, QueryList>(
                self.client.extractor(&self.model_name)
                    .preamble("Extract YouTube search queries as a JSON object with a 'queries' array field.")
                    .build()
            );
        
        // Execute the pipeline
        let result = pipeline.call(prompt).await
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to extract queries: {}", e)))?;
        
        Ok(result.queries)
    }
    
    pub async fn analyze_results(
        &self, 
        results: Vec<(String, String, String)>, // (title, uploader, url)
        original_query: &str
    ) -> Result<AnalysisResult, MusicDownloadError> {
        let results_text = results.iter().enumerate()
            .map(|(i, (title, uploader, url))| {
                format!("{}: {} - {} ({})", i, title, uploader, url)
            })
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
        
        // Create a pipeline that extracts structured analysis
        let pipeline = pipeline::new()
            .extract::<_, _, AnalysisResult>(
                self.client.extractor(&self.model_name)
                    .preamble("Extract analysis result as JSON with fields: query, reasoning, selected_result_index, confidence.")
                    .build()
            );
        
        // Execute the pipeline
        pipeline.call(prompt).await
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to extract analysis: {}", e)))
    }
}