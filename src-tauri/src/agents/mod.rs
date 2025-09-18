use async_trait::async_trait;
use rig::{
    completion::Prompt,
    providers::ollama::{self, CompletionModel},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub mod youtube_search;
pub mod rig_agents;
pub mod rig_agents_v2;
pub mod pipeline_agents;
pub mod rig_coordinator;
pub mod rig_extractors;
pub mod rig_coordinator_v2;
pub mod simple_metadata_extractor;
pub mod gemini_coordinator;
pub mod gemini_direct;

pub use youtube_search::YouTubeSearchTool;
pub use rig_agents::{RigQueryGenerator, RigResultAnalyzer};
pub use rig_agents_v2 as rig_agents_extractor;
pub use pipeline_agents::MusicSearchPipeline;
pub use rig_coordinator::RigMusicSearchCoordinator;
pub use rig_extractors::{QueryExtractor, ResultExtractor};
pub use rig_coordinator_v2::ExtractorBasedCoordinator;
pub use gemini_coordinator::GeminiCoordinator;
pub use gemini_direct::GeminiDirectCoordinator;

use crate::MusicDownloadError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchContext {
    pub original_query: String,
    pub iterations: Vec<SearchIteration>,
    pub max_iterations: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIteration {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub reasoning: String,
    pub selected_result: Option<SearchResult>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub uploader: String,
    pub duration: Option<u32>,
    pub view_count: Option<u64>,
    pub upload_date: Option<String>,
    pub url: String,
}

#[async_trait]
pub trait MusicSearchAgent: Send + Sync {
    async fn process(&self, context: &SearchContext) -> Result<SearchIteration, MusicDownloadError>;
}

// We'll use Ollama-specific agents for now since we're focusing on Ollama integration