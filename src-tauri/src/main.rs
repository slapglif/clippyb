use clipboard_win::{formats, get_clipboard};
use notify_rust::Notification;
use regex::Regex;
use reqwest::Client;
use rspotify::prelude::*;
use rspotify::{ClientCredsSpotify, Credentials};
use serde::{Deserialize, Serialize};
// use soundcloud_rs::SoundCloudApi; // TODO: Fix SoundCloud integration
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tokio::process::Command as TokioCommand;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::TrayIconBuilder;
use winit::event::Event;
use winit::event_loop::{ControlFlow, EventLoop};
use anyhow::Result;
use thiserror::Error;
use dirs;
use futures;

mod agents;
mod utils;
mod download_queue;
mod queue;

use agents::{SearchResult as AgentSearchResult, SearchIteration as AgentSearchIteration};
use download_queue::{DownloadQueue, DownloadTask};
use utils::fuzzy_match::FuzzyMatcher;
use queue::{PersistentQueue, QueueItem, QueueStatus, QueueProcessor};

#[derive(Clone, Debug)]
struct MusicItem {
    content: String,
    item_type: MusicItemType,
    timestamp: SystemTime,
    processed: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum MusicItemType {
    SongName(String),
    YoutubeUrl(String),
    SpotifyUrl(String),
    SoundCloudUrl(String),
    SongList(Vec<String>),
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SongMetadata {
    artist: String,
    title: String,
    album: Option<String>,
    year: Option<u32>,
    youtube_url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct SearchResult {
    id: String,
    title: String,
    uploader: String,
    duration: Option<u32>,
    view_count: Option<u64>,
    upload_date: Option<String>,
    url: String,
}

#[derive(Clone, Debug)]
struct SearchIteration {
    query: String,
    results: Vec<SearchResult>,
    reasoning: String,
    selected_result: Option<SearchResult>,
    confidence: f32,
}

#[derive(Clone, Debug)]
enum LLMProvider {
    Ollama { url: String, model: String, num_context: Option<u32> },
    OpenAI { api_key: String, model: String },
    Gemini { api_key: String },
    Claude { api_key: String },
}

#[derive(Serialize, Deserialize)]
struct LLMConfig {
    provider: String,
    url: Option<String>,
    model: Option<String>,
    num_context: Option<u32>,
    api_key: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
}

#[derive(Serialize, Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize)]
struct GeminiPart {
    text: String,
}

#[derive(Serialize, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Serialize, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Error, Debug)]
enum MusicDownloadError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("LLM error: {0}")]
    LLM(String),
    #[error("Download error: {0}")]
    Download(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Metadata error: {0}")]
    Metadata(String),
    #[error("Agent error: {0}")]
    Agent(String),
}

#[derive(Clone)]
struct MusicDownloader {
    history: Arc<Mutex<Vec<MusicItem>>>,
    last_clipboard: Arc<Mutex<String>>,
    client: Client,
    spotify_client: Option<Arc<ClientCredsSpotify>>,
    llm_provider: Arc<LLMProvider>,
    music_folder: Arc<PathBuf>,
    download_tx: mpsc::UnboundedSender<MusicItem>,
    auto_download: Arc<RwLock<bool>>,
    pending_downloads: Arc<Mutex<Vec<MusicItem>>>,
    active_processes: Arc<Mutex<Vec<u32>>>, // Track active yt-dlp process IDs
    persistent_queue: Arc<PersistentQueue>,
}

impl MusicDownloader {
    async fn new() -> Result<(Self, mpsc::UnboundedReceiver<MusicItem>), MusicDownloadError> {
        let llm_provider = Self::load_llm_config();
        
        let music_folder = dirs::audio_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap().join("Music"))
            .join("ClippyB Downloads");
        
        // Create music folder if it doesn't exist
        fs::create_dir_all(&music_folder)?;
        
        let (download_tx, download_rx) = mpsc::unbounded_channel();
        
        // Try to create Spotify client (optional)
        let spotify_client = Self::create_spotify_client().await.map(Arc::new);
        if spotify_client.is_some() {
            println!("✅ Spotify API client initialized");
        } else {
            println!("⚠️ Spotify API not available (set SPOTIFY_CLIENT_ID and SPOTIFY_CLIENT_SECRET for better metadata)");
        }
        
        // Initialize persistent queue
        let queue_path = music_folder.join("clippyb_queue.json");
        let persistent_queue = Arc::new(PersistentQueue::new(queue_path)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to initialize queue: {}", e)))?);

        let downloader = Self {
            history: Arc::new(Mutex::new(Vec::new())),
            last_clipboard: Arc::new(Mutex::new(String::new())),
            client: Client::new(),
            spotify_client,
            llm_provider: Arc::new(llm_provider),
            music_folder: Arc::new(music_folder),
            download_tx,
            auto_download: Arc::new(RwLock::new(true)),
            pending_downloads: Arc::new(Mutex::new(Vec::new())),
            active_processes: Arc::new(Mutex::new(Vec::new())),
            persistent_queue,
        };
        
        Ok((downloader, download_rx))
    }
    
    /// Abort all active downloads by killing yt-dlp processes
    fn abort_all_downloads(&self) {
        println!("🛑 Aborting all active downloads...");
        
        let mut processes = self.active_processes.lock().unwrap();
        let process_count = processes.len();
        
        if process_count == 0 {
            // Don't spam - self.show_notification("ℹ️ No Active Downloads", "No downloads to abort");
            return;
        }
        
        // Kill all active processes
        for pid in processes.iter() {
            #[cfg(windows)]
            {
                use std::process::Command;
                let _ = Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .output();
            }
            #[cfg(not(windows))]
            {
                use std::process::Command;
                let _ = Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output();
            }
        }
        
        processes.clear();
        // Don't spam - self.show_notification(
        //     "🛑 Downloads Aborted", 
        //     &format!("Killed {} active download processes", process_count)
        // );
        println!("🛑 Aborted {} download processes", process_count);
    }
    
    async fn create_spotify_client() -> Option<ClientCredsSpotify> {
        let client_id = env::var("SPOTIFY_CLIENT_ID").ok()?;
        let client_secret = env::var("SPOTIFY_CLIENT_SECRET").ok()?;
        
        let creds = Credentials::new(&client_id, &client_secret);
        let spotify = ClientCredsSpotify::new(creds);
        
        // Test the connection
        match spotify.request_token().await {
            Ok(_) => Some(spotify),
            Err(e) => {
                println!("⚠️ Failed to authenticate with Spotify: {}", e);
                None
            }
        }
    }
    
    fn load_llm_config() -> LLMProvider {
        // Try to load config from file, fall back to environment variables, then defaults
        let config_path = dirs::config_dir()
            .map(|p| p.join("clippyb").join("config.json"))
            .unwrap_or_else(|| PathBuf::from("clippyb_config.json"));
        
        if let Ok(config_content) = fs::read_to_string(&config_path) {
            if let Ok(config) = serde_json::from_str::<LLMConfig>(&config_content) {
                return Self::create_provider_from_config(config);
            }
        }
        
        println!("📁 No config found at: {:?}", config_path);
        println!("🔧 Using default configuration (Ollama)");
        
        // Default to Gemini with provided API key
        LLMProvider::Gemini {
            api_key: "AIzaSyDepY_ZOJPQCmz62H8K23LB_TH2CVGyoT4".to_string(),
        }
    }
    
    fn create_provider_from_config(config: LLMConfig) -> LLMProvider {
        match config.provider.to_lowercase().as_str() {
            "ollama" => LLMProvider::Ollama {
                url: config.url.unwrap_or_else(|| "http://98.87.166.97:11434".to_string()),
                model: config.model.unwrap_or_else(|| "granite3.3:latest".to_string()),
                num_context: config.num_context.or(Some(3200)),
            },
            "openai" => LLMProvider::OpenAI {
                api_key: config.api_key.unwrap_or_else(|| env::var("OPENAI_API_KEY").unwrap_or_default()),
                model: config.model.unwrap_or_else(|| "gpt-4o-mini".to_string()),
            },
            "gemini" => LLMProvider::Gemini {
                api_key: config.api_key.unwrap_or_else(|| env::var("GEMINI_API_KEY").unwrap_or_default()),
            },
            "claude" => LLMProvider::Claude {
                api_key: config.api_key.unwrap_or_else(|| env::var("ANTHROPIC_API_KEY").unwrap_or_default()),
            },
            _ => {
                println!("⚠️ Unknown provider '{}', defaulting to Ollama", config.provider);
                LLMProvider::Ollama {
                    url: "http://98.87.166.97:11434".to_string(),
                    model: "granite3.3:latest".to_string(),
                    num_context: Some(12000),
                }
            }
        }
    }
    
    fn check_clipboard(&self) {
        if let Ok(current) = get_clipboard::<String, _>(formats::Unicode) {
            let mut last = self.last_clipboard.lock().unwrap();
            if current != *last && !current.trim().is_empty() {
                println!("🔍 New clipboard content: {}", current.chars().take(50).collect::<String>());
                
                // Classify the content
                let item_type = self.classify_content(&current);
                
                if !matches!(item_type, MusicItemType::Unknown) {
                    let item = MusicItem {
                        content: current.clone(),
                        item_type: item_type.clone(),
                        timestamp: SystemTime::now(),
                        processed: false,
                    };
                    
                    // Add to history
                    let mut history = self.history.lock().unwrap();
                    history.insert(0, item.clone());
                    if history.len() > 100 {
                        history.pop();
                    }
                    
                    // Log to console only, no notifications for detection
                    match &item_type {
                        MusicItemType::SongList(songs) => {
                            println!("🎵 Music playlist detected: {} tracks", songs.len());
                        },
                        _ => {
                            let preview = match &item_type {
                                MusicItemType::SongName(name) => name.chars().take(40).collect::<String>(),
                                MusicItemType::SpotifyUrl(_) => "Spotify track".to_string(),
                                MusicItemType::YoutubeUrl(_) => "YouTube video".to_string(),
                                MusicItemType::SoundCloudUrl(_) => "SoundCloud track".to_string(),
                                _ => "Music".to_string(),
                            };
                            println!("🎵 Music detected: {}", preview);
                        }
                    }
                    
                    // Send for processing
                    if let Err(e) = self.download_tx.send(item) {
                        eprintln!("Failed to send item for processing: {}", e);
                    }
                }
                
                *last = current;
            }
        }
    }
    
    fn classify_content(&self, content: &str) -> MusicItemType {
        let content = content.trim();
        println!("🔍 DEBUG: Content length: {}, first 100 chars: {}", content.len(), content.chars().take(100).collect::<String>());
        
        // Check for Spotify URLs FIRST - handle both web URLs and URI format
        let spotify_web_pattern = Regex::new(r"(?i)(?:https?://)?(?:open\.)?spotify\.com/(?:track|album|playlist)/([a-zA-Z0-9]+)").unwrap();
        let spotify_uri_pattern = Regex::new(r"(?i)spotify:(?:track|album|playlist):([a-zA-Z0-9]+)").unwrap();
        
        // Split by any newline type (\n, \r\n, or \r)
        let lines: Vec<&str> = content.split(|c| c == '\n' || c == '\r')
            .filter(|l| !l.trim().is_empty())
            .collect();
        
        println!("📋 DEBUG: Found {} non-empty lines after splitting", lines.len());
        
        // Count Spotify URLs (both web and URI formats)
        let spotify_urls: Vec<String> = lines.iter()
            .filter(|line| spotify_web_pattern.is_match(line) || spotify_uri_pattern.is_match(line))
            .map(|line| line.trim().to_string())
            .collect();
        
        if spotify_urls.len() > 1 {
            println!("📋 Detected {} Spotify URLs as SongList", spotify_urls.len());
            return MusicItemType::SongList(spotify_urls);
        } else if spotify_urls.len() == 1 {
            println!("🔗 Detected single Spotify URL");
            return MusicItemType::SpotifyUrl(spotify_urls[0].clone());
        }
        
        // YouTube URL patterns
        let youtube_patterns = [
            Regex::new(r"(?i)(?:https?://)?(?:www\.)?(?:youtube\.com/watch\?v=|youtu\.be/)([a-zA-Z0-9_-]{11})").unwrap(),
            Regex::new(r"(?i)(?:https?://)?(?:www\.)?youtube\.com/playlist\?list=([a-zA-Z0-9_-]+)").unwrap(),
        ];
        
        for pattern in &youtube_patterns {
            if pattern.is_match(content) {
                println!("🔗 DEBUG: Detected YouTube URL");
                return MusicItemType::YoutubeUrl(content.to_string());
            }
        }
        
        // Check for SoundCloud URLs
        let soundcloud_pattern = Regex::new(r"(?i)(?:https?://)?(?:www\.)?soundcloud\.com/[\w-]+/[\w-]+").unwrap();
        
        let soundcloud_urls: Vec<String> = lines.iter()
            .filter(|line| soundcloud_pattern.is_match(line))
            .map(|line| line.trim().to_string())
            .collect();
        
        if soundcloud_urls.len() > 1 {
            println!("📋 Detected {} SoundCloud URLs as SongList", soundcloud_urls.len());
            return MusicItemType::SongList(soundcloud_urls);
        } else if soundcloud_urls.len() == 1 {
            println!("🎵 Detected single SoundCloud URL");
            return MusicItemType::SoundCloudUrl(soundcloud_urls[0].clone());
        }
        
        // For everything else that's not a URL, only consider it for music classification if:
        // 1. It's reasonably short (typical song names/lists)
        // 2. It doesn't contain obvious programming/technical patterns
        if !content.is_empty() && content.len() < 500 {
            // Quick check for obvious non-music patterns
            let lower_content = content.to_lowercase();
            let non_music_indicators = [
                "error", "exception", "debug", "warning",
                "function", "class", "import", "export",
                "const ", "let ", "var ", "def ",
                "http://", "https://", "www.",
                "{", "}", "[", "]", "<", ">",
                "num_ctx", "model", "config", "api",
                "null", "undefined", "true", "false",
                "::", "=>", "->>", "```",
                "│", "┌", "└", "├", "─", "║", "╔", "╚", "╠", "═",
                "agent", "middleware", "architecture", "intelligent",
                "system", "server", "client", "database", "network",
                "implementation", "development", "framework", "library"
            ];
            
            for indicator in &non_music_indicators {
                if lower_content.contains(indicator) {
                    println!("❌ Contains non-music indicator: '{}', skipping", indicator);
                    return MusicItemType::Unknown;
                }
            }
            
            // Check if it might be a song format (e.g., "Artist - Song" or "Song by Artist")
            let music_indicators = [
                " - ",  // Common artist-song separator
                " by ", // Song by artist format
                " feat", " ft.", // Featuring
                "remix", "acoustic", "live",
                "album", "single", "ep",
            ];
            
            let mut has_music_pattern = false;
            for indicator in &music_indicators {
                if lower_content.contains(indicator) {
                    has_music_pattern = true;
                    break;
                }
            }
            
            // Only send to LLM if it has clear music-like patterns
            if has_music_pattern {
                println!("🎵 Potential music content, will verify with LLM");
                return MusicItemType::SongName(content.to_string());
            }
            
            // For multi-line content, be much more strict
            if content.lines().count() > 1 {
                // Only consider as potential music if ALL lines look like song names
                let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
                if lines.len() > 20 {
                    println!("❌ Too many lines ({}) for a song list, likely not music", lines.len());
                    return MusicItemType::Unknown;
                }
                
                // Check if all lines could be song names (simple format check)
                let all_song_like = lines.iter().all(|line| {
                    let line = line.trim();
                    // Must be reasonable length and format
                    line.len() > 3 && line.len() < 80 
                    && !line.contains("|") && !line.contains("│") 
                    && !line.contains("─") && !line.contains("┌")
                    && !line.contains("└") && !line.contains("├")
                    && !line.starts_with("#") && !line.starts_with("//")
                    && !line.contains("=") && !line.contains(":")
                });
                
                if all_song_like {
                    println!("🎵 Multi-line content looks like song list, will verify with LLM");
                    return MusicItemType::SongName(content.to_string());
                }
            }
        }
        
        println!("⏭️ Not music-related, ignoring");
        MusicItemType::Unknown
    }
    
    
    fn show_notification(&self, title: &str, body: &str) {
        if let Err(e) = Notification::new()
            .summary(title)
            .body(body)
            .timeout(3000)
            .show() {
            eprintln!("Failed to show notification: {}", e);
        }
    }
    
    async fn is_music_related(&self, content: &str) -> Result<bool, MusicDownloadError> {
        match &*self.llm_provider {
            LLMProvider::Gemini { api_key } => {
                let prompt = format!(
                    "Is this text related to music, songs, artists, or albums? Answer with ONLY 'YES' or 'NO'.

Text: '{}'

Look for:
- Song titles (like 'Bohemian Rhapsody' or 'Stairway to Heaven')
- Artist names with songs (like 'Beatles - Hey Jude' or 'Taylor Swift Love Story')
- Album names
- Music-related lists
- Anything that could be a music query

DO NOT consider as music:
- File paths, URLs, error messages
- Programming code, configuration text
- General conversation, instructions
- Random text, clipboard artifacts

Answer: ",
                    content.chars().take(500).collect::<String>() // Limit content length
                );

                let response = self.call_gemini_api(api_key, &prompt).await?;
                let response = response.trim().to_uppercase();
                Ok(response.contains("YES"))
            },
            LLMProvider::Ollama { url, model, .. } => {
                use rig::{providers::ollama, client::CompletionClient, completion::Completion};

                let client = ollama::Client::builder()
                    .base_url(url)
                    .build()
                    .map_err(|e| MusicDownloadError::LLM(format!("Failed to create Ollama client: {}", e)))?;
                
                let agent = client.agent(model)
                    .preamble("You are a music content classifier. Respond with ONLY 'YES' or 'NO'.")
                    .build();

                let prompt = format!(
                    "Is this text related to music, songs, artists, or albums? Answer with ONLY 'YES' or 'NO'.

Text: '{}'

Look for:
- Song titles (like 'Bohemian Rhapsody' or 'Stairway to Heaven')
- Artist names with songs (like 'Beatles - Hey Jude' or 'Taylor Swift Love Story')
- Album names
- Music-related lists
- Anything that could be a music query

DO NOT consider as music:
- File paths, URLs, error messages
- Programming code, configuration text
- General conversation, instructions
- Random text, clipboard artifacts

Answer: ",
                    content.chars().take(500).collect::<String>() // Limit content length
                );

                let response = agent.completion(&prompt, vec![])
                    .await
                    .map_err(|e| MusicDownloadError::LLM(format!("Rig error: {}", e)))?
                    .send()
                    .await
                    .map_err(|e| MusicDownloadError::LLM(format!("Rig completion error: {}", e)))?;

                // Extract text from OneOrMany<AssistantContent>
                let response_text = match response.choice.into_iter().next() {
                    Some(rig::completion::AssistantContent::Text(text)) => text.text,
                    _ => return Ok(false), // If unexpected format, assume not music
                };

                let response = response_text.trim().to_uppercase();
                Ok(response.contains("YES"))
            },
            LLMProvider::Gemini { api_key } => {
                use rig::{providers::gemini, client::CompletionClient, completion::Completion};
                
                let client = gemini::Client::new(api_key);
                let agent = client.agent("gemini-2.5-flash-lite")
                    .preamble("You are a music content classifier. Respond with ONLY 'YES' or 'NO'.")
                    .build();

                let prompt = format!(
                    "Is this text related to music, songs, artists, or albums? Answer with ONLY 'YES' or 'NO'.

Text: '{}'

Look for:
- Song titles (like 'Bohemian Rhapsody' or 'Stairway to Heaven')
- Artist names with songs (like 'Beatles - Hey Jude' or 'Taylor Swift Love Story')
- Album names
- Music-related lists
- Spotify URLs (like 'spotify:track:...')
- YouTube music URLs
- Anything that could be a music query

DO NOT consider as music:
- File paths, URLs (except music-specific ones), error messages
- Programming code, configuration text
- General conversation, instructions
- Random text, clipboard artifacts

Answer: ",
                    content.chars().take(500).collect::<String>() // Limit content length
                );

                let response = agent.completion(&prompt, vec![])
                    .await
                    .map_err(|e| MusicDownloadError::LLM(format!("Rig error: {}", e)))?
                    .send()
                    .await
                    .map_err(|e| MusicDownloadError::LLM(format!("Rig completion error: {}", e)))?;

                // Extract text from OneOrMany<AssistantContent>
                let response_text = match response.choice.into_iter().next() {
                    Some(rig::completion::AssistantContent::Text(text)) => text.text,
                    _ => return Ok(false), // If unexpected format, assume not music
                };

                let response = response_text.trim().to_uppercase();
                Ok(response.contains("YES"))
            },
            _ => {
                eprintln!("Provider not supported for music classification, defaulting to true");
                Ok(true) // Default to true for other providers
            }
        }
    }
    
    async fn process_music_item(&self, item: MusicItem) -> Result<(), MusicDownloadError> {
        println!("🎧 Processing music item: {:?}", item.item_type);
        
        match item.item_type {
            MusicItemType::SongName(song) => {
                self.process_song_name(&song).await?
            },
            MusicItemType::YoutubeUrl(url) => {
                self.download_from_youtube(&url).await?
            },
            MusicItemType::SpotifyUrl(url) => {
                self.process_spotify_url(&url).await?
            },
            MusicItemType::SoundCloudUrl(url) => {
                self.process_soundcloud_url(&url).await?
            },
            MusicItemType::SongList(songs) => {
                println!("📥 Queuing {} songs to persistent queue", songs.len());
                
                // Create queue items for all songs
                let mut queue_items = Vec::new();
                for (index, song) in songs.iter().enumerate() {
                    let item_type = if song.contains("spotify.com") {
                        if song.contains("/playlist/") {
                            "spotify_playlist".to_string()
                        } else {
                            "spotify_track".to_string()
                        }
                    } else if song.contains("soundcloud.com") {
                        "soundcloud_track".to_string()
                    } else if song.contains("youtube.com") || song.contains("youtu.be") {
                        "youtube_url".to_string()
                    } else {
                        "song_name".to_string()
                    };
                    
                    let queue_item = QueueItem::new(song.clone(), item_type)
                        .with_metadata(queue::queue_item::QueueItemMetadata {
                            title: None, // Will be populated during processing
                            artist: None,
                            playlist_name: Some(format!("Clipboard Batch {}", 
                                SystemTime::now()
                                    .duration_since(SystemTime::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs())),
                            total_tracks: Some(songs.len()),
                            track_index: Some(index + 1),
                        });
                    
                    queue_items.push(queue_item);
                }
                
                // Add all items to persistent queue
                if let Err(e) = self.persistent_queue.enqueue_multiple(queue_items).await {
                    eprintln!("❌ Failed to queue songs: {}", e);
                    return Err(MusicDownloadError::LLM(format!("Failed to queue songs: {}", e)));
                }
                
                // Only log to console, no notification spam
                println!("📥 {} tracks queued for background processing", songs.len());
                
                // Print queue status
                let (pending, in_progress, completed, failed, skipped) = self.persistent_queue.get_status_counts().await;
                println!("📊 Queue status: {} pending | {} in progress | {} completed | {} failed | {} skipped", 
                        pending, in_progress, completed, failed, skipped);
            },
            MusicItemType::Unknown => {}
        }
        
        Ok(())
    }
    
    async fn process_song_name(&self, song: &str) -> Result<(), MusicDownloadError> {
        // First check if this is actually music-related using LLM
        println!("🤔 Checking if content is music-related...");
        let is_music = self.is_music_related(song).await?;
        if !is_music {
            println!("❌ Not music-related, skipping: {}", song.chars().take(50).collect::<String>());
            return Ok(());
        }
        
        println!("✅ Confirmed as music-related, processing...");
        let metadata = self.get_song_metadata_from_llm(song).await?;
        self.download_and_tag_song(metadata).await?;
        Ok(())
    }
    
    async fn process_spotify_url(&self, url: &str) -> Result<(), MusicDownloadError> {
        let metadata = self.extract_spotify_metadata_with_llm(url).await?;
        self.download_and_tag_song(metadata).await?;
        Ok(())
    }
    
    async fn process_soundcloud_url(&self, url: &str) -> Result<(), MusicDownloadError> {
        let metadata = self.extract_soundcloud_metadata_with_llm(url).await?;
        self.download_and_tag_song(metadata).await?;
        Ok(())
    }
    
    async fn get_song_metadata_from_llm(&self, song_query: &str) -> Result<SongMetadata, MusicDownloadError> {
        println!("🔍 Starting ReAct search for: {}", song_query);
        
        // Use ReAct pattern to iteratively search and find the best match
        let search_result = self.react_search_for_song(song_query).await?;
        
        // Extract final metadata from the selected result
        self.extract_metadata_from_search_result(&search_result, song_query).await
    }
    
    async fn extract_spotify_metadata_with_llm(&self, spotify_url: &str) -> Result<SongMetadata, MusicDownloadError> {
        // First extract song info from Spotify URL
        let song_info = self.extract_song_info_from_spotify_url(spotify_url).await?;
        println!("🔍 Extracted from Spotify: {}", song_info);
        
        // Early duplicate check - parse artist and title from song_info
        if let Some((artist, title)) = self.parse_artist_title(&song_info) {
            if FuzzyMatcher::song_exists(&artist, &title, &self.music_folder) {
                println!("✅ Song already exists, skipping: {} - {}", artist, title);
                return Ok(SongMetadata {
                    artist: artist,
                    title: title,
                    album: Some("Already Downloaded".to_string()),
                    year: None,
                    youtube_url: "".to_string(),
                });
            }
        }
        
        // Then use ReAct search to find the best YouTube match
        let search_result = self.react_search_for_song(&song_info).await?;
        
        // Extract final metadata
        self.extract_metadata_from_search_result(&search_result, &song_info).await
    }
    
    async fn extract_soundcloud_metadata_with_llm(&self, soundcloud_url: &str) -> Result<SongMetadata, MusicDownloadError> {
        // First extract song info from SoundCloud URL
        let song_info = self.extract_song_info_from_soundcloud_url(soundcloud_url).await?;
        println!("🔍 Extracted from SoundCloud: {}", song_info);
        
        // Early duplicate check - parse artist and title from song_info
        if let Some((artist, title)) = self.parse_artist_title(&song_info) {
            if FuzzyMatcher::song_exists(&artist, &title, &self.music_folder) {
                println!("✅ Song already exists, skipping expensive search: {} - {}", artist, title);
                // Create dummy metadata to indicate already exists
                return Ok(SongMetadata {
                    artist: artist,
                    title: title,
                    album: Some("Already Downloaded".to_string()),
                    year: None,
                    youtube_url: "".to_string(),
                });
            }
        }
        
        // Then use ReAct search to find the best YouTube match
        let search_result = self.react_search_for_song(&song_info).await?;
        
        // Extract final metadata
        self.extract_metadata_from_search_result(&search_result, &song_info).await
    }
    
    async fn react_search_for_song(&self, song_query: &str) -> Result<SearchResult, MusicDownloadError> {
        // Use the appropriate coordinator based on LLM provider
        match &*self.llm_provider {
            LLMProvider::Ollama { url, model, .. } => {
                // Use the extractor-based coordinator with JSON format for granite3.3
                let coordinator = agents::ExtractorBasedCoordinator::new(url, model);
                
                // Get result from Rig coordinator
                let agent_result = coordinator.search_for_song(song_query).await?;
                
                // Convert back to our SearchResult type
                Ok(SearchResult {
                    id: agent_result.id,
                    title: agent_result.title,
                    uploader: agent_result.uploader,
                    duration: agent_result.duration,
                    view_count: agent_result.view_count,
                    upload_date: agent_result.upload_date,
                    url: agent_result.url,
                })
            },
            LLMProvider::Gemini { api_key } => {
                // Use the direct Gemini implementation with exact model name
                let coordinator = agents::GeminiDirectCoordinator::new(api_key, "gemini-2.5-flash-lite");
                
                // Get result from Gemini coordinator
                let agent_result = coordinator.search_for_song(song_query).await?;
                
                // Convert back to our SearchResult type
                Ok(SearchResult {
                    id: agent_result.id,
                    title: agent_result.title,
                    uploader: agent_result.uploader,
                    duration: agent_result.duration,
                    view_count: agent_result.view_count,
                    upload_date: agent_result.upload_date,
                    url: agent_result.url,
                })
            },
            _ => {
                // Ollama and Gemini are supported with Rig for now
                Err(MusicDownloadError::LLM("Only Ollama and Gemini providers are supported with Rig integration".to_string()))
            }
        }
    }
    
    // Keep the old implementation as a fallback
    async fn react_search_for_song_legacy(&self, song_query: &str) -> Result<SearchResult, MusicDownloadError> {
        // TODO: Implement legacy search for OpenAI/Claude/Gemini
        Err(MusicDownloadError::LLM("Legacy search not implemented. Please use Ollama provider.".to_string()))
    }
    
    async fn generate_initial_search_queries(&self, song_query: &str) -> Result<Vec<String>, MusicDownloadError> {
        let prompt = format!(
            "Generate 3-4 different YouTube search queries to find the exact song: '{}'

Return ONLY a JSON array of search query strings, like:
[\"query 1\", \"query 2\", \"query 3\"]

Generate variations like:
- Exact artist and song name
- With \"official\" or \"music video\"
- Alternative spellings or formats
- Without extra words that might confuse search

Example for \"Never Gonna Give You Up - Rick Astley\":
[\"Rick Astley Never Gonna Give You Up\", \"Rick Astley Never Gonna Give You Up official\", \"Never Gonna Give You Up Rick Astley music video\", \"Rick Astley Never Gonna Give You Up 1987\"]",
            song_query
        );
        
        let response = self.call_llm_api(&prompt).await?;
        let queries: Vec<String> = serde_json::from_str(&response)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse search queries: {} - Response: {}", e, response)))?;
        
        println!("🔍 Generated {} search queries", queries.len());
        for (i, query) in queries.iter().enumerate() {
            println!("  {}. {}", i + 1, query);
        }
        
        Ok(queries)
    }
    
    async fn generate_refined_search_queries(&self, song_query: &str, previous_iterations: &[SearchIteration]) -> Result<Vec<String>, MusicDownloadError> {
        let previous_context = previous_iterations
            .iter()
            .map(|iter| format!("Query: {} | Reasoning: {}", iter.query, iter.reasoning))
            .collect::<Vec<_>>()
            .join("\n");
        
        let prompt = format!(
            "Based on previous search attempts, generate 2-3 NEW refined YouTube search queries for: '{}'

Previous attempts:\n{}\n
Return ONLY a JSON array of search query strings.

Try different approaches:
- More specific terms
- Different word order
- Add year, genre, or album info
- Try alternate artist/song spellings
- Focus on official sources",
            song_query, previous_context
        );
        
        let response = self.call_llm_api(&prompt).await?;
        let queries: Vec<String> = serde_json::from_str(&response)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse refined queries: {} - Response: {}", e, response)))?;
        
        println!("🔍 Generated {} refined queries", queries.len());
        for (i, query) in queries.iter().enumerate() {
            println!("  {}. {}", i + 1, query);
        }
        
        Ok(queries)
    }
    
    async fn search_youtube_with_ytdlp(&self, query: &str) -> Result<Vec<SearchResult>, MusicDownloadError> {
        println!("🔍 Searching YouTube: {}", query);
        
        let output = TokioCommand::new("yt-dlp")
            .arg("--dump-json")
            .arg("--playlist-end")
            .arg("10")  // Limit to top 10 results
            .arg("--no-download")
            .arg(&format!("ytsearch10:{}", query))
            .output()
            .await
            .map_err(|e| MusicDownloadError::Download(format!("Failed to run yt-dlp search: {}", e)))?;
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(MusicDownloadError::Download(format!("yt-dlp search failed: {}", error_msg)));
        }
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut results = Vec::new();
        
        // Parse each JSON line
        for line in output_str.lines() {
            if line.trim().is_empty() {
                continue;
            }
            
            let json_value: serde_json::Value = serde_json::from_str(line)
                .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse yt-dlp JSON: {}", e)))?;
            
            let result = SearchResult {
                id: json_value["id"].as_str().unwrap_or("").to_string(),
                title: json_value["title"].as_str().unwrap_or("").to_string(),
                uploader: json_value["uploader"].as_str().unwrap_or("").to_string(),
                duration: json_value["duration"].as_u64().map(|d| d as u32),
                view_count: json_value["view_count"].as_u64(),
                upload_date: json_value["upload_date"].as_str().map(|s| s.to_string()),
                url: format!("https://youtube.com/watch?v={}", json_value["id"].as_str().unwrap_or("")),
            };
            
            results.push(result);
        }
        
        println!("🔍 Found {} search results", results.len());
        Ok(results)
    }
    
    async fn analyze_search_results(&self, original_query: &str, results: &[SearchResult], previous_iterations: &[SearchIteration]) -> Result<SearchIteration, MusicDownloadError> {
        let results_summary = results
            .iter()
            .take(10)  // Limit to top 10 for analysis
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
            "Analyze these YouTube search results for the song: \"{}\"

Results:
{}
{}

Return ONLY a JSON response in this format:
{{
  \"query\": \"search query used\",
  \"reasoning\": \"why this result was selected\",
  \"selected_result_index\": N,
  \"confidence\": 0.XX
}}

Prioritize:
1. Official artist/label uploads
2. Exact title match
3. High view count
4. Normal song duration (2-5 min)

Set index to -1 if no good match found.",
            original_query, results_summary, previous_context
        );
        
        let response = self.call_llm_api(&prompt).await?;
        
        // Parse the JSON response
        let json_value: serde_json::Value = serde_json::from_str(&response)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse analysis response: {} - Response: {}", e, response)))?;
        
        let query = json_value["query"].as_str().unwrap_or("unknown").to_string();
        let reasoning = json_value["reasoning"].as_str().unwrap_or("No reasoning provided").to_string();
        let selected_index = json_value["selected_result_index"].as_i64().unwrap_or(-1);
        let confidence = json_value["confidence"].as_f64().unwrap_or(0.0) as f32;
        
        let selected_result = if selected_index >= 0 && (selected_index as usize) < results.len() {
            Some(results[selected_index as usize].clone())
        } else {
            None
        };
        
        Ok(SearchIteration {
            query,
            results: results.to_vec(),
            reasoning,
            selected_result,
            confidence,
        })
    }
    
    fn extract_spotify_track_id(&self, spotify_url: &str) -> Option<String> {
        // Extract track ID from Spotify URL
        // Format: https://open.spotify.com/track/4uGBcjQwCHBR1KYcj9bv3l
        let parts: Vec<&str> = spotify_url.split('/').collect();
        if parts.len() > 4 && parts[3] == "track" {
            let track_id = parts[4].split('?').next()?;
            return Some(track_id.to_string());
        }
        None
    }
    
    async fn extract_song_info_from_spotify_url(&self, spotify_url: &str) -> Result<String, MusicDownloadError> {
        println!("🔍 Extracting metadata from Spotify URL: {}", spotify_url);
        
        // Don't try to download from Spotify directly - use API or web scraping instead
        if let Some(track_id) = self.extract_spotify_track_id(spotify_url) {
            // Try using Spotify API if available
            if let Some(client) = &self.spotify_client {
                if let Ok(track_info) = self.get_spotify_track_info_api(client, &track_id).await {
                    return Ok(track_info);
                }
            }
            
            // Fallback to web scraping
            if let Ok(track_info) = self.get_spotify_track_info_web(&track_id).await {
                return Ok(track_info);
            }
        }
        
        Err(MusicDownloadError::Download(format!("Could not extract song info from Spotify URL: {}", spotify_url)))
    }
    
    async fn get_spotify_track_info_api(&self, client: &ClientCredsSpotify, track_id: &str) -> Result<String, MusicDownloadError> {
        use rspotify::model::TrackId;
        use rspotify::clients::BaseClient;
        
        let track_id = TrackId::from_id(track_id)
            .map_err(|e| MusicDownloadError::Download(format!("Invalid Spotify track ID: {}", e)))?;
        
        let track = client.track(track_id, None).await
            .map_err(|e| MusicDownloadError::Download(format!("Spotify API error: {}", e)))?;
        
        let artist = track.artists.get(0)
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Unknown Artist".to_string());
        
        let song_info = format!("{} - {}", artist, track.name);
        println!("✅ Extracted from Spotify API: {}", song_info);
        Ok(song_info)
    }
    
    async fn get_spotify_track_info_web(&self, track_id: &str) -> Result<String, MusicDownloadError> {
        // Fallback web scraping method using the public Spotify web page
        let url = format!("https://open.spotify.com/track/{}", track_id);
        
        // Use reqwest to fetch the page and extract metadata from HTML
        let response = reqwest::get(&url).await
            .map_err(|e| MusicDownloadError::Download(format!("Failed to fetch Spotify page: {}", e)))?;
        
        let html = response.text().await
            .map_err(|e| MusicDownloadError::Download(format!("Failed to read HTML: {}", e)))?;
        
        // Simple regex to extract title and artist from the HTML meta tags
        if let Some(title_match) = html.find("\"name\":\"") {
            let start = title_match + 9; // Length of "\"name\":\""
            if let Some(end) = html[start..].find("\"") {
                let title = &html[start..start + end];
                
                // Look for artist info nearby
                if let Some(artist_start) = html[start..].find("\"artist\":{\"name\":\"") {
                    let artist_start = start + artist_start + 18; // Length of "\"artist\":{\"name\":\""
                    if let Some(artist_end) = html[artist_start..].find("\"") {
                        let artist = &html[artist_start..artist_start + artist_end];
                        let song_info = format!("{} - {}", artist, title);
                        println!("✅ Extracted from Spotify web: {}", song_info);
                        return Ok(song_info);
                    }
                }
            }
        }
        
        Err(MusicDownloadError::Download(format!("Could not extract track info from web page for: {}", track_id)))
    }
    
    async fn extract_song_info_from_soundcloud_url(&self, soundcloud_url: &str) -> Result<String, MusicDownloadError> {
        let prompt = format!(
            "Extract the artist and song name from this SoundCloud URL: '{}'

Return ONLY the song info in format: \"Artist - Song Title\"

Example: For a SoundCloud URL, return: \"Artist Name - Song Title\"",
            soundcloud_url
        );
        
        let response = self.call_llm_api(&prompt).await?;
        Ok(response.trim().to_string())
    }
    
    async fn get_youtube_video_info(&self, youtube_url: &str) -> Result<String, MusicDownloadError> {
        // Extract video title using yt-dlp
        let output = TokioCommand::new("yt-dlp")
            .arg("--dump-json")
            .arg("--no-download")
            .arg(youtube_url)
            .output()
            .await
            .map_err(|e| MusicDownloadError::Download(format!("Failed to get YouTube info: {}", e)))?;
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(MusicDownloadError::Download(format!("yt-dlp failed to get video info: {}", error_msg)));
        }
        
        let output_str = String::from_utf8_lossy(&output.stdout);
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&output_str) {
            let title = json["title"].as_str().unwrap_or("Unknown Video");
            let uploader = json["uploader"].as_str().unwrap_or("Unknown Channel");
            return Ok(format!("{} by {}", title, uploader));
        }
        
        Err(MusicDownloadError::Download("Could not extract video info".to_string()))
    }
    
    async fn extract_metadata_from_search_result(&self, search_result: &SearchResult, original_query: &str) -> Result<SongMetadata, MusicDownloadError> {
        let prompt = format!(
            "Extract structured metadata from this YouTube video information:

Original Query: '{}'
Video Title: '{}'
Uploader: '{}'
URL: '{}'

Return ONLY a JSON object with this exact format:
{{
  \"artist\": \"Artist Name\",
  \"title\": \"Song Title\",
  \"album\": \"Album Name\",
  \"year\": 2023,
  \"youtube_url\": \"{}\"
}}

Extract the clean artist and song title, removing extra text like '[Official Video]', 'HD', etc.",
            original_query, search_result.title, search_result.uploader, search_result.url, search_result.url
        );
        
        let response = self.call_llm_api(&prompt).await?;
        self.parse_metadata_response(&response)
    }
    
    async fn call_llm_api(&self, prompt: &str) -> Result<String, MusicDownloadError> {
        match &*self.llm_provider {
            LLMProvider::Ollama { url, model, num_context } => {
                self.call_ollama_api(url, model, num_context.as_ref(), prompt).await
            },
            LLMProvider::OpenAI { api_key, model } => {
                self.call_openai_api(api_key, model, prompt).await
            },
            LLMProvider::Gemini { api_key } => {
                self.call_gemini_api(api_key, prompt).await
            },
            LLMProvider::Claude { api_key } => {
                self.call_claude_api(api_key, prompt).await
            },
        }
    }
    
    fn sanitize_llm_output(&self, output: &str) -> String {
        // Remove anything before the first JSON opening bracket or array
        if let Some(json_start) = output.find(|c| c == '{' || c == '[') {
            let json_part = &output[json_start..];
            // Find the matching closing bracket
            let mut depth = 0;
            let mut end_pos = json_part.len();
            
            for (i, c) in json_part.chars().enumerate() {
                match c {
                    '{' | '[' => depth += 1,
                    '}' | ']' => {
                        depth -= 1;
                        if depth == 0 {
                            end_pos = i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            
            return json_part[..end_pos].to_string();
        }
        
        // If no JSON found, remove any "thinking" blocks and try to get the actual response
        let output = output.replace("<think>\n", "").replace("</think>\n", "");
        
        // Remove any markdown code block markers
        let output = output.replace("```json\n", "").replace("```\n", "");
        
        // Remove any explanation blocks after the response
        if let Some(explanation_start) = output.find("**Explanation:**") {
            output[..explanation_start].trim().to_string()
        } else {
            output.trim().to_string()
        }
    }
    
    async fn call_ollama_api(&self, url: &str, model: &str, num_context: Option<&u32>, prompt: &str) -> Result<String, MusicDownloadError> {
        #[derive(Serialize)]
        struct OllamaRequest {
            model: String,
            prompt: String,
            stream: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            options: Option<OllamaOptions>,
        }
        
        #[derive(Serialize)]
        struct OllamaOptions {
            #[serde(skip_serializing_if = "Option::is_none")]
            num_ctx: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            num_predict: Option<u32>,
            temperature: f32,
        }
        
        #[derive(Deserialize)]
        struct OllamaResponse {
            response: String,
            done: bool,
        }
        
        let request = OllamaRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            stream: false,
            options: Some(OllamaOptions {
                num_ctx: num_context.copied(),
                num_predict: Some(1000),  // Limit response length
                temperature: 0.1,
            }),
        };
        
        let response = self.client
            .post(&format!("{}/api/generate", url.trim_end_matches('/')))
            .json(&request)
            .send()
            .await
            .map_err(|e| MusicDownloadError::Network(e))?;
        
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(MusicDownloadError::LLM(format!("Ollama API error: {}", error_text)));
        }
        
        let ollama_response: OllamaResponse = response.json()
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse Ollama response: {}", e)))?;
        
        // Sanitize the LLM output
        let sanitized = self.sanitize_llm_output(&ollama_response.response);
        Ok(sanitized)
    }
    
    async fn call_openai_api(&self, api_key: &str, model: &str, prompt: &str) -> Result<String, MusicDownloadError> {
        if api_key.is_empty() {
            return Err(MusicDownloadError::LLM("OpenAI API key not configured".to_string()));
        }
        
        #[derive(Serialize)]
        struct OpenAIRequest {
            model: String,
            messages: Vec<OpenAIMessage>,
            temperature: f32,
            max_tokens: u32,
        }
        
        #[derive(Serialize, Deserialize)]
        struct OpenAIMessage {
            role: String,
            content: String,
        }
        
        #[derive(Deserialize)]
        struct OpenAIResponse {
            choices: Vec<OpenAIChoice>,
        }
        
        #[derive(Deserialize)]
        struct OpenAIChoice {
            message: OpenAIMessage,
        }
        
        let request = OpenAIRequest {
            model: model.to_string(),
            messages: vec![OpenAIMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
            temperature: 0.1,
            max_tokens: 1000,
        };
        
        let response = self.client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?
            .json::<OpenAIResponse>()
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("OpenAI API error: {}", e)))?;
        
        if let Some(choice) = response.choices.first() {
            Ok(choice.message.content.clone())
        } else {
            Err(MusicDownloadError::LLM("No response from OpenAI API".to_string()))
        }
    }
    
    async fn call_gemini_api(&self, api_key: &str, prompt: &str) -> Result<String, MusicDownloadError> {
        if api_key.is_empty() {
            return Err(MusicDownloadError::LLM("Gemini API key not configured".to_string()));
        }
        
        let request = GeminiRequest {
            contents: vec![GeminiContent {
                parts: vec![GeminiPart {
                    text: prompt.to_string(),
                }],
            }],
        };
        
        let response = self.client
            .post(&format!(
                "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-lite:generateContent?key={}",
                api_key
            ))
            .json(&request)
            .send()
            .await?
            .json::<GeminiResponse>()
            .await?;
        
        if let Some(candidate) = response.candidates.first() {
            if let Some(part) = candidate.content.parts.first() {
                return Ok(part.text.clone());
            }
        }
        
        Err(MusicDownloadError::LLM("No response from Gemini API".to_string()))
    }
    
    async fn call_claude_api(&self, api_key: &str, prompt: &str) -> Result<String, MusicDownloadError> {
        if api_key.is_empty() {
            return Err(MusicDownloadError::LLM("Claude API key not configured".to_string()));
        }
        
        #[derive(Serialize)]
        struct ClaudeRequest {
            model: String,
            max_tokens: u32,
            messages: Vec<ClaudeMessage>,
        }
        
        #[derive(Serialize)]
        struct ClaudeMessage {
            role: String,
            content: String,
        }
        
        #[derive(Deserialize)]
        struct ClaudeResponse {
            content: Vec<ClaudeContent>,
        }
        
        #[derive(Deserialize)]
        struct ClaudeContent {
            text: String,
        }
        
        let request = ClaudeRequest {
            model: "claude-3-haiku-20240307".to_string(),
            max_tokens: 1000,
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }],
        };
        
        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?
            .json::<ClaudeResponse>()
            .await
            .map_err(|e| MusicDownloadError::LLM(format!("Claude API error: {}", e)))?;
        
        if let Some(content) = response.content.first() {
            Ok(content.text.clone())
        } else {
            Err(MusicDownloadError::LLM("No response from Claude API".to_string()))
        }
    }
    
    fn parse_metadata_response(&self, response: &str) -> Result<SongMetadata, MusicDownloadError> {
        // Use sanitization to extract pure JSON
        let sanitized = utils::llm_utils::sanitize_llm_json_response(response);
        
        serde_json::from_str::<SongMetadata>(&sanitized)
            .map_err(|e| MusicDownloadError::LLM(format!("Failed to parse metadata: {} - Response: {}", e, response)))
    }
    
    async fn download_from_youtube(&self, url: &str) -> Result<(), MusicDownloadError> {
        // For direct YouTube URLs, extract the title and then use ReAct to find the best version
        let video_info = self.get_youtube_video_info(url).await?;
        println!("🔍 YouTube video info: {}", video_info);
        
        // Use ReAct search to find the best version (in case the provided URL is low quality)
        let search_result = self.react_search_for_song(&video_info).await?;
        
        // Extract final metadata
        let metadata = self.extract_metadata_from_search_result(&search_result, &video_info).await?;
        
        // Download and tag the song
        self.download_and_tag_song(metadata).await
    }
    
    async fn extract_youtube_metadata_with_llm(&self, youtube_url: &str) -> Result<SongMetadata, MusicDownloadError> {
        let prompt = format!(
            "Extract song information from this YouTube URL: '{}' and return the metadata.

Please return ONLY a JSON object with this exact format:
{{
  \"artist\": \"Artist Name\",
  \"title\": \"Song Title\",
  \"album\": \"Album Name\",
  \"year\": 2023,
  \"youtube_url\": \"{}\"
}}

Extract the artist and song title from the video title, removing any extra text like '[Official Video]', 'HD', etc.",
            youtube_url, youtube_url
        );
        
        let response = self.call_llm_api(&prompt).await?;
        self.parse_metadata_response(&response)
    }
    
    async fn download_and_tag_song(&self, metadata: SongMetadata) -> Result<(), MusicDownloadError> {
        // Check if this is the "Already Downloaded" marker from early duplicate detection
        if metadata.album.as_ref() == Some(&"Already Downloaded".to_string()) && metadata.youtube_url.is_empty() {
            println!("✅ Song already exists (early detection): {} - {}", metadata.artist, metadata.title);
            return Ok(());
        }
        
        // Fallback duplicate check for cases where early detection was bypassed
        if FuzzyMatcher::song_exists(&metadata.artist, &metadata.title, &self.music_folder) {
            println!("✅ Song already downloaded: {} - {}", metadata.artist, metadata.title);
            return Ok(());
        }
        
        println!("💾 Downloading: {} - {}", metadata.artist, metadata.title);
        // Don't notify for downloading start - we already showed "Music Detected"
        // self.show_notification("💾 Downloading...", &format!("{} - {}", metadata.artist, metadata.title));
        
        // Check if yt-dlp is available
        if !self.check_ytdlp_available() {
            return Err(MusicDownloadError::Download(
                "yt-dlp not found. Please install yt-dlp: pip install yt-dlp".to_string()
            ));
        }
        
        // Create filename
        let safe_filename = format!("{} - {}.%(ext)s", 
            self.sanitize_filename(&metadata.artist),
            self.sanitize_filename(&metadata.title)
        );
        
        let output_path = self.music_folder.join(&safe_filename);
        
        // Download with yt-dlp
        let child = TokioCommand::new("yt-dlp")
            .arg("--extract-audio")
            .arg("--audio-format")
            .arg("mp3")
            .arg("--audio-quality")
            .arg("0")  // Best quality
            .arg("-o")
            .arg(output_path.to_string_lossy().as_ref())
            .arg(&metadata.youtube_url)
            .spawn()
            .map_err(|e| MusicDownloadError::Download(format!("Failed to run yt-dlp: {}", e)))?;
        
        // Track the process ID
        let pid = child.id();
        if let Some(pid) = pid {
            self.active_processes.lock().unwrap().push(pid);
        }
        
        let output = child.wait_with_output().await
            .map_err(|e| MusicDownloadError::Download(format!("Failed to wait for yt-dlp: {}", e)))?;
        
        // Remove from active processes
        if let Some(pid) = pid {
            let mut processes = self.active_processes.lock().unwrap();
            processes.retain(|&p| p != pid);
        }
        
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            return Err(MusicDownloadError::Download(format!("yt-dlp failed: {}", error_msg)));
        }
        
        // Find the downloaded file (yt-dlp replaces %(ext)s with actual extension)
        let mp3_filename = format!("{} - {}.mp3", 
            self.sanitize_filename(&metadata.artist),
            self.sanitize_filename(&metadata.title)
        );
        let mp3_path = self.music_folder.join(&mp3_filename);
        
        if mp3_path.exists() {
            // Tag the MP3 file
            self.tag_mp3_file(&mp3_path, &metadata)?;
            
            println!("✅ Downloaded and tagged: {}", mp3_path.display());
            // No individual notifications - only log to console
        } else {
            return Err(MusicDownloadError::Download("Downloaded file not found".to_string()));
        }
        
        Ok(())
    }
    
    fn check_ytdlp_available(&self) -> bool {
        Command::new("yt-dlp")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
    
    fn sanitize_filename(&self, name: &str) -> String {
        name.chars()
            .map(|c| match c {
                '<' | '>' | ':' | '\"' | '/' | '\\' | '|' | '?' | '*' => '_',
                _ => c,
            })
            .collect::<String>()
            .trim()
            .to_string()
    }
    
    fn tag_mp3_file(&self, file_path: &Path, metadata: &SongMetadata) -> Result<(), MusicDownloadError> {
        use id3::{Tag, TagLike, Version};
        
        let mut tag = Tag::read_from_path(file_path)
            .unwrap_or_else(|_| Tag::new());
        
        tag.set_artist(&metadata.artist);
        tag.set_title(&metadata.title);
        
        if let Some(ref album) = metadata.album {
            tag.set_album(album);
        }
        
        if let Some(year) = metadata.year {
            tag.set_year(year as i32);
        }
        
        // Add custom comment with source URL
        tag.add_comment(id3::frame::Comment {
            lang: "eng".to_string(),
            description: "Source".to_string(),
            text: metadata.youtube_url.clone(),
        });
        
        tag.write_to_path(file_path, Version::Id3v24)
            .map_err(|e| MusicDownloadError::Metadata(format!("Failed to write MP3 tags: {}", e)))?;
        
        Ok(())
    }
    
    // Helper function to parse "Artist - Title" format
    fn parse_artist_title(&self, song_info: &str) -> Option<(String, String)> {
        // Try different separators commonly used
        for separator in [" - ", " – ", " — ", ": ", " | "] {
            if let Some(pos) = song_info.find(separator) {
                let artist = song_info[..pos].trim().to_string();
                let title = song_info[pos + separator.len()..].trim().to_string();
                if !artist.is_empty() && !title.is_empty() {
                    return Some((artist, title));
                }
            }
        }
        
        // If no separator found, try to split on common patterns
        let words: Vec<&str> = song_info.trim().split_whitespace().collect();
        if words.len() >= 2 {
            // Assume first word is artist, rest is title
            let artist = words[0].to_string();
            let title = words[1..].join(" ");
            return Some((artist, title));
        }
        
        None
    }
    
    fn get_history(&self) -> Vec<MusicItem> {
        self.history.lock().unwrap().clone()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    println!("🎵 Starting ClippyB v{} - AI-Powered Music Downloader", VERSION);
    
    let (downloader, mut download_rx) = MusicDownloader::new().await
        .map_err(|e| format!("Failed to initialize music downloader: {}", e))?;
    let downloader = Arc::new(downloader);
    
    // Display LLM provider status
    match &*downloader.llm_provider {
        LLMProvider::Ollama { url, model, num_context } => {
            println!("🤖 LLM Provider: Ollama");
            println!("   URL: {}", url);
            println!("   Model: {}", model);
            if let Some(ctx) = num_context {
                println!("   Context: {} tokens", ctx);
            } else {
                println!("   Context: Model default");
            }
        },
        LLMProvider::OpenAI { model, .. } => {
            println!("🤖 LLM Provider: OpenAI");
            println!("   Model: {}", model);
        },
        LLMProvider::Gemini { .. } => {
            println!("🤖 LLM Provider: Gemini 2.5 Flash Lite");
        },
        LLMProvider::Claude { .. } => {
            println!("🤖 LLM Provider: Claude 3 Haiku");
        },
    }
    
    println!("📁 Music folder: {:?}", downloader.music_folder);
    
    let event_loop = EventLoop::new()?;
    
    // Create system tray menu
    let tray_menu = Menu::new();
    let quit_item = MenuItem::with_id("quit", "Quit", true, None);
    let show_history = MenuItem::with_id("show_history", "Show Download History", true, None);
    let clear_history = MenuItem::with_id("clear_history", "Clear History", true, None);
    let open_folder = MenuItem::with_id("open_folder", "Open Music Folder", true, None);
    let config_menu = MenuItem::with_id("config", "Configure LLM Provider", true, None);
    let abort_downloads = MenuItem::with_id("abort", "🛑 Abort All Downloads", true, None);
    let queue_status = MenuItem::with_id("queue_status", "📊 Show Queue Status", true, None);
    let separator = PredefinedMenuItem::separator();
    
    tray_menu.append_items(&[
        &show_history,
        &open_folder,
        &clear_history,
        &separator,
        &queue_status,
        &abort_downloads,
        &separator,
        &config_menu,
        &separator,
        &quit_item,
    ])?;
    
    // Create tray icon
    let _tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip(&format!("🎵 ClippyB v{} - AI Music Downloader", VERSION))
        .with_icon(create_music_icon())
        .build()?;
    
    // Start clipboard monitoring thread
    let downloader_monitor = Arc::clone(&downloader);
    thread::spawn(move || {
        loop {
            downloader_monitor.check_clipboard();
            thread::sleep(Duration::from_millis(100)); // Faster clipboard monitoring
        }
    });
    
    // Start queue processor for persistent queue
    let queue_processor = queue::QueueProcessor::new(
        downloader.persistent_queue.clone(), 
        Arc::clone(&downloader)
    );
    tokio::spawn(async move {
        queue_processor.start_processing().await;
    });
    println!("🚀 Queue processor started for background processing");
    
    // Start download processing task
    let downloader_processor = Arc::clone(&downloader);
    tokio::spawn(async move {
        while let Some(item) = download_rx.recv().await {
            if let Err(e) = downloader_processor.process_music_item(item).await {
                eprintln!("⚠️ Download failed: {}", e);
                downloader_processor.show_notification("⚠️ Download Failed", &format!("{}", e));
            }
        }
    });
    
    // Handle menu events
    let menu_channel = MenuEvent::receiver();
    let downloader_menu = Arc::clone(&downloader);
    
    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Poll);
        
        match event {
            Event::AboutToWait => {
                // Check for menu events
                if let Ok(event) = menu_channel.try_recv() {
                    println!("🖱️ Menu event received: '{}'", event.id.0);
                    match event.id.0.as_str() {
                        "quit" => {
                            println!("👋 ClippyB shutting down...");
                            elwt.exit();
                        }
                        "abort" => {
                            downloader_menu.abort_all_downloads();
                        }
                        "queue_status" => {
                            let rt = tokio::runtime::Handle::current();
                            let downloader_clone = Arc::clone(&downloader_menu);
                            rt.spawn(async move {
                                let (pending, in_progress, completed, failed, skipped) = 
                                    downloader_clone.persistent_queue.get_status_counts().await;
                                let total = pending + in_progress + completed + failed + skipped;
                                
                                let status_msg = if total == 0 {
                                    "📭 Queue is empty".to_string()
                                } else {
                                    format!(
                                        "📊 Queue Status: {} total | {} pending | {} in progress | {} completed | {} failed | {} skipped",
                                        total, pending, in_progress, completed, failed, skipped
                                    )
                                };
                                
                                println!("\n{}", "=".repeat(80));
                                println!("{}", status_msg);
                                println!("{}", "=".repeat(80));
                                
                                downloader_clone.show_notification("📊 Queue Status", &status_msg);
                            });
                        }
                        "show_history" => {
                            let history = downloader_menu.get_history();
                            println!("\n🎧 Music Download History:");
                            println!("=========================================\n");
                            for (i, item) in history.iter().take(20).enumerate() {
                                let status = if item.processed { "✅" } else { "⏳" };
                                let type_icon = match item.item_type {
                                    MusicItemType::SongName(_) => "🎵",
                                    MusicItemType::YoutubeUrl(_) => "📹",
                                    MusicItemType::SpotifyUrl(_) => "🟢",
                                    MusicItemType::SoundCloudUrl(_) => "🟠",
                                    MusicItemType::SongList(_) => "📜",
                                    MusicItemType::Unknown => "❓",
                                };
                                println!("{}. {} {} {} ({})", 
                                    i + 1, 
                                    status,
                                    type_icon,
                                    item.content.chars().take(70).collect::<String>(),
                                    format_time(&item.timestamp)
                                );
                            }
                            println!("\n=========================================\n");
                        }
                        "clear_history" => {
                            downloader_menu.history.lock().unwrap().clear();
                            println!("🗑️ Music download history cleared");
                        }
                        "open_folder" => {
                            let _ = Command::new("explorer")
                                .arg(downloader_menu.music_folder.to_string_lossy().as_ref())
                                .spawn();
                        }
                        "config" => {
                            let config_path = dirs::config_dir()
                                .map(|p| p.join("clippyb").join("config.json"))
                                .unwrap_or_else(|| PathBuf::from("clippyb_config.json"));
                            
                            // Create config directory if it doesn't exist
                            if let Some(parent) = config_path.parent() {
                                let _ = fs::create_dir_all(parent);
                            }
                            
                            // Create sample config if it doesn't exist
                            if !config_path.exists() {
                                let sample_configs = vec![
                                    ("Ollama (Default)", serde_json::json!({
                                        "provider": "ollama",
                                        "url": "http://localhost:11434",
                                        "model": "llama3.2:3b",
                                        "num_context": 12000
                                    })),
                                    ("OpenAI", serde_json::json!({
                                        "provider": "openai", 
                                        "model": "gpt-4o-mini",
                                        "api_key": "your-openai-api-key-here"
                                    })),
                                    ("Gemini", serde_json::json!({
                                        "provider": "gemini",
                                        "api_key": "your-gemini-api-key-here"
                                    })),
                                    ("Claude Haiku", serde_json::json!({
                                        "provider": "claude",
                                        "api_key": "your-anthropic-api-key-here"
                                    }))
                                ];
                                
                                let config_content = serde_json::json!({
                                    "_comment": "ClippyB LLM Configuration - Choose one provider below",
                                    "_examples": sample_configs.into_iter().map(|(name, config)| {
                                        serde_json::json!({ name: config })
                                    }).collect::<Vec<_>>(),
                                    "provider": "ollama",
                                    "url": "http://localhost:11434",
                                    "model": "llama3.2:3b",
                                    "num_context": 12000
                                });
                                
                                if let Ok(json_str) = serde_json::to_string_pretty(&config_content) {
                                    let _ = fs::write(&config_path, json_str);
                                }
                            }
                            
                            // Open config file in default editor
                            let _ = Command::new("notepad")
                                .arg(&config_path)
                                .spawn();
                            
                            println!("📝 Config file opened: {:?}", config_path);
                            println!("💡 Edit the config and restart ClippyB to apply changes");
                        }
                        unknown => {
                            println!("⚠️ Unknown menu event: '{}'", unknown);
                        }
                    }
                }
            }
            _ => {}
        }
    })?;
    
    Ok(())
}

fn create_music_icon() -> tray_icon::Icon {
    // Create a better music-themed icon (16x16)
    let mut icon_rgba = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            // Create a musical note pattern
            let color = if 
                // Vertical stem
                (x == 7 && y >= 4 && y <= 13) ||
                // Note head (filled circle)
                ((x >= 5 && x <= 9) && (y >= 11 && y <= 13) && 
                 (((x-7)*(x-7) + (y-12)*(y-12)) <= 4)) ||
                // Beam/flag
                (x >= 8 && x <= 11 && y >= 4 && y <= 6) ||
                (x >= 9 && x <= 12 && y >= 5 && y <= 7)
            {
                [0x1E, 0x90, 0xFF, 0xFF] // Musical blue
            } else {
                [0x00, 0x00, 0x00, 0x00] // Transparent
            };
            icon_rgba.extend_from_slice(&color);
        }
    }
    
    tray_icon::Icon::from_rgba(icon_rgba, 16, 16)
        .expect("Failed to create music icon")
}

fn format_time(time: &SystemTime) -> String {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(duration) => {
            let secs = duration.as_secs();
            let mins = secs / 60;
            let hours = mins / 60;
            let days = hours / 24;
            
            if days > 0 {
                format!("{}d ago", days)
            } else if hours > 0 {
                format!("{}h ago", hours)
            } else if mins > 0 {
                format!("{}m ago", mins)
            } else {
                "Just now".to_string()
            }
        }
        Err(_) => "Unknown".to_string(),
    }
}
