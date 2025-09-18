// Simple metadata extractor using Rig extractors
use rig::{
    extractor::Extractor,
    providers::ollama,
    client::CompletionClient,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::MusicDownloadError;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SongInfo {
    #[schemars(description = "Artist name(s)")]
    artist: String,
    #[schemars(description = "Song title")]
    title: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct MetadataInfo {
    artist: String,
    title: String,
    album: Option<String>,
    year: Option<i32>,
    youtube_url: String,
}

pub struct MetadataExtractor {
    client: ollama::Client,
    model_name: String,
}

impl MetadataExtractor {
    pub fn new(client: &ollama::Client, model_name: &str) -> Self {
        Self { 
            client: client.clone(),
            model_name: model_name.to_string(),
        }
    }
    
    pub async fn extract_song_info(&self, text: &str) -> Result<String, MusicDownloadError> {
        let extractor = self.client
            .extractor::<SongInfo>(&self.model_name)
            .preamble("Extract the artist and song title. Be concise and accurate.")
            .build();
            
        let result = extractor
            .extract(text)
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to extract song info: {}", e)))?;
            
        Ok(format!("{} - {}", result.artist, result.title))
    }
    
    pub async fn extract_metadata(&self, text: &str, youtube_url: &str) -> Result<MetadataInfo, MusicDownloadError> {
        let input = format!("Song: {}\nYouTube URL: {}", text, youtube_url);
        
        let extractor = self.client
            .extractor::<MetadataInfo>(&self.model_name)
            .preamble("Extract complete song metadata from the given information.")
            .build();
            
        extractor
            .extract(&input)
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to extract metadata: {}", e)))
    }
}