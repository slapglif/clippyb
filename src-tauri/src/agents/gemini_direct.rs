// Direct Gemini API implementation for ClippyB
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use schemars::JsonSchema;
use futures::future;

use super::{
    SearchContext, SearchIteration, SearchResult, YouTubeSearchTool,
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

// Gemini API types
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    generation_config: GeminiGenerationConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiGenerationConfig {
    temperature: f32,
    top_k: i32,
    top_p: f32,
    max_output_tokens: i32,
}

#[derive(Serialize, Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize)]
struct GeminiPart {
    text: String,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

pub struct GeminiDirectCoordinator {
    api_key: String,
    model: String,
    client: reqwest::Client,
    youtube_tool: Arc<YouTubeSearchTool>,
    max_iterations: usize,
}

impl GeminiDirectCoordinator {
    pub fn new(api_key: &str, model: &str) -> Self {
        println!("🔗 Creating Direct Gemini client with model: {}", model);
        
        Self {
            api_key: api_key.to_string(),
            model: model.to_string(),
            client: reqwest::Client::new(),
            youtube_tool: Arc::new(YouTubeSearchTool::new()),
            max_iterations: 3,
        }
    }
    
    async fn call_gemini_api(&self, prompt: &str) -> Result<String, MusicDownloadError> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );
        
        let request = GeminiRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: prompt.to_string(),
                }],
            }],
            generation_config: GeminiGenerationConfig {
                temperature: 0.3,
                top_k: 1,
                top_p: 0.95,
                max_output_tokens: 1000,
            },
        };
        
        println!("🔍 DEBUG: Sending request to Gemini API: {}", url);
        println!("🔍 DEBUG: Request body: {}", serde_json::to_string_pretty(&request).unwrap_or_default());
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Gemini API network error: {}", e)))?;
        
        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            println!("🔍 DEBUG: Gemini API error response ({}): {}", status, error_text);
            return Err(MusicDownloadError::LLM(format!("Gemini API error ({}): {}", status, error_text)));
        }
        
        let gemini_response: GeminiResponse = response.json()
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse Gemini response: {}", e)))?;
        
        if let Some(candidate) = gemini_response.candidates.first() {
            if let Some(part) = candidate.content.parts.first() {
                return Ok(part.text.clone());
            }
        }
        
        Err(MusicDownloadError::LLM("No response from Gemini API".to_string()))
    }
    
    pub async fn search_for_song(&self, song_query: &str) -> Result<SearchResult, MusicDownloadError> {
        println!("🚀 Starting fast single-pass search for: {}", song_query);
        
        // Generate comprehensive search queries in one call
        let response = self.call_gemini_api(&format!(
            "Generate 5-8 comprehensive YouTube search queries for this song: {}\n\nInclude variations like:\n- Exact artist and title\n- Artist name only\n- Title with 'official audio' or 'lyrics'\n- Common alternate spellings or formats\n\nReturn ONLY valid JSON: {{\"queries\": [\"query1\", \"query2\", \"query3\", \"query4\", \"query5\"]}}",
            song_query
        )).await?;
        
        // Parse queries
        let json_start = response.find('{').unwrap_or(0);
        let json_end = response.rfind('}').map(|i| i + 1).unwrap_or(response.len());
        let json_str = &response[json_start..json_end];
        
        let query_list: QueryList = serde_json::from_str(json_str)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse queries: {} - Response: {}", e, response)))?;
            
        if query_list.queries.is_empty() {
            return Err(MusicDownloadError::LLM("No queries generated".to_string()));
        }
        
        println!("🔍 Generated {} queries, searching YouTube", query_list.queries.len());
        
        // Execute all searches concurrently
        let search_results = self.youtube_tool.search_multiple(query_list.queries).await?;
        
        println!("📊 Found {} results, analyzing", search_results.len());
        
        if search_results.is_empty() {
            println!("❌ No YouTube results found for: {}", song_query);
            return Err(MusicDownloadError::Download(format!("No YouTube results found for: {}", song_query)));
        }
        
        // Analyze results with Gemini
        let analysis = self.analyze_results(song_query, &search_results).await?;
        
        if let Some(result) = analysis.selected_result {
            println!("✅ Selected: {} by {}", result.title, result.uploader);
            Ok(result)
        } else {
            // This should never happen due to fallback logic, but if it does, force pick first result
            if !search_results.is_empty() {
                println!("🔧 FORCE FALLBACK: Picking first result from {} available", search_results.len());
                Ok(search_results[0].clone())
            } else {
                Err(MusicDownloadError::Download(format!("No suitable match found for: {}", song_query)))
            }
        }
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
        
        let prompt = format!(
            "{}\n\nYou are a music search expert. Generate effective YouTube search queries for the given song.\n\nReturn ONLY valid JSON in exactly this format: {{\"queries\": [\"query1\", \"query2\", \"query3\"]}}. Include 2-3 search query strings.",
            input_text
        );
        
        let response = self.call_gemini_api(&prompt).await?;
        
        println!("🔍 DEBUG: Gemini response: {}", response);
        
        // Clean the response - sometimes LLMs add extra text
        let json_start = response.find('{').unwrap_or(0);
        let json_end = response.rfind('}').map(|i| i + 1).unwrap_or(response.len());
        let json_str = &response[json_start..json_end];
        
        // Parse JSON response
        let result: QueryList = serde_json::from_str(json_str)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse Gemini response: {} - Response: {}", e, response)))?;
            
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
        
        let prompt = format!(
            "{}\n\nYou are a music search result analyzer. You MUST select a result from the list - index -1 is NOT allowed.\n\nCRITICAL: Always pick the result with the highest view count or most professional uploader if you're unsure. Even a partial match is better than no match.\n\nReturn ONLY valid JSON in exactly this format: {{\"query\": \"search query\", \"reasoning\": \"explanation\", \"selected_result_index\": 0, \"confidence\": 0.8}}. If confused, pick index 0.",
            input
        );
        
        let response = self.call_gemini_api(&prompt).await?;
        
        println!("🔍 DEBUG: Result analysis - Gemini response: {}", response);
        
        // Clean the response
        let json_start = response.find('{').unwrap_or(0);
        let json_end = response.rfind('}').map(|i| i + 1).unwrap_or(response.len());
        let json_str = &response[json_start..json_end];
        
        // Parse JSON response
        let mut analysis: ResultAnalysis = serde_json::from_str(json_str)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse analysis response: {} - Response: {}", e, response)))?;
            
        // FORCE: If Gemini still returns -1, override it to 0
        if analysis.selected_result_index < 0 {
            println!("🔧 FORCING: Gemini returned -1, overriding to 0");
            analysis.selected_result_index = 0;
        }
            
        let selected = if analysis.selected_result_index >= 0 
            && (analysis.selected_result_index as usize) < results.len() {
            Some(results[analysis.selected_result_index as usize].clone())
        } else if !results.is_empty() {
            // Fallback: if Gemini returns -1 but we have results, pick the first one
            println!("⚠️ Gemini returned -1 but we have {} results, picking first one", results.len());
            Some(results[0].clone())
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