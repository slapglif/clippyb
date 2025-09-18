# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Project Overview

ClippyB is an AI-powered music downloader that automatically detects music-related content in your clipboard and downloads high-quality MP3 files. The application runs as a native Rust system tray application that monitors clipboard changes, uses LLM integration to find the best YouTube versions of songs, and automatically downloads and tags MP3 files.

## Architecture

### Core Stack
- **Backend**: Native Rust application (no web dependencies)
- **Target**: Windows desktop (x86_64-pc-windows-gnu)
- **AI Integration**: Gemini 2.5 Flash for song search and metadata extraction
- **Download Engine**: yt-dlp for YouTube audio extraction
- **Audio Processing**: MP3 conversion with ID3 metadata tagging

### Key Components
- **src-tauri/src/main.rs**: Main application with clipboard monitoring, LLM integration, and download processing
- **MusicDownloader**: Core struct handling all music download operations
- **System Tray**: Native Windows tray integration with music-themed icon
- **Clipboard Detection**: Regex-based content classification for music URLs and song names

### Music Detection Capabilities
- **YouTube URLs**: Direct video and playlist links
- **Spotify URLs**: Track, album, and playlist links
- **SoundCloud URLs**: Track links
- **Song Names**: Artist - Title format and natural language
- **Song Lists**: Multiple songs in clipboard (line-separated)

### ReAct Pattern Integration
- **Multi-LLM Support**: Ollama (default), OpenAI, Gemini 2.5 Flash, Claude 3 Haiku
- **ReAct Search Strategy**: Reasoning + Acting pattern with iterative refinement
- **yt-dlp Integration**: Real YouTube search results, not hallucinated URLs
- **Smart Validation**: AI analyzes search results to find exact official versions
- **Confidence Scoring**: Iterates until high confidence or max attempts reached
- **Anti-Fake Protection**: Specifically trained to avoid covers, fan uploads, wrong videos

## Development Commands

### Building the Application
```bash
# Build optimized release version
cargo build --release --target x86_64-pc-windows-gnu --manifest-path "src-tauri/Cargo.toml"

# Build debug version
cargo build --target x86_64-pc-windows-gnu --manifest-path "src-tauri/Cargo.toml"

# Run the application
./src-tauri/target/x86_64-pc-windows-gnu/release/clippyb.exe
```

### Development Tools
```bash
# Check for compilation errors
cargo check --manifest-path "src-tauri/Cargo.toml"

# Run tests
cargo test --manifest-path "src-tauri/Cargo.toml"

# Format code
cargo fmt --manifest-path "src-tauri/Cargo.toml"

# Lint code
cargo clippy --manifest-path "src-tauri/Cargo.toml"
```

## Configuration and Setup

### Required Dependencies
1. **yt-dlp**: Install via pip for YouTube downloading and search
   ```bash
   pip install yt-dlp
   ```

2. **LLM Provider**: Choose one of the following:

   **Ollama (Recommended - Free & Local)**:
   ```bash
   # Install Ollama from https://ollama.ai
   ollama pull llama3.2:3b
   # No API key required!
   ```

   **OpenAI**:
   ```bash
   set OPENAI_API_KEY=your-openai-api-key
   ```

   **Gemini**:
   ```bash
   set GEMINI_API_KEY=your-gemini-api-key
   ```

   **Claude**:
   ```bash
   set ANTHROPIC_API_KEY=your-claude-api-key
   ```

### Cargo Dependencies
```toml
[dependencies]
# System integration
clipboard-win = "5.4"          # Windows clipboard access
tray-icon = "0.21"             # System tray functionality
winit = "0.30"                 # Window/event handling
notify-rust = "4.11"           # Desktop notifications

# HTTP and networking
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }
tokio = { version = "1.0", features = ["full"] }

# Music processing
id3 = "1.14"                   # MP3 metadata tagging
mp3-metadata = "0.3"           # MP3 file analysis
regex = "1.11"                 # Pattern matching
url = "2.5"                    # URL parsing

# File handling
dirs = "5.0"                   # System directories
tempfile = "3.14"              # Temporary files
which = "7.0"                  # Executable detection

# Serialization and utilities
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1.11", features = ["v4"] }
anyhow = "1.0"                 # Error handling
thiserror = "2.0"              # Error derive macros
```

## Application Flow

### Clipboard Monitoring
1. Runs background thread checking clipboard every 500ms
2. Detects changes and classifies content type
3. **AI Music Detection**: LLM determines if content is actually music-related
4. Shows desktop notification for detected music content
5. Queues items for processing

### ReAct Search Pattern
When processing music content, ClippyB uses an advanced ReAct (Reasoning + Acting) pattern:

1. **Query Generation**: LLM generates 3-4 strategic YouTube search queries
   - Exact artist/song combinations
   - Variations with "official", "music video"
   - Alternative spellings and formats

2. **Real Search**: Execute searches using yt-dlp (not AI hallucination)
   - Gets actual YouTube metadata: title, uploader, views, duration
   - Returns top 10 results per query
   - Rate-limited to avoid API abuse

3. **Analysis & Validation**: LLM analyzes all search results
   - Evaluates authenticity (official channels vs fan uploads)
   - Checks title match accuracy
   - Considers view count, duration, upload date
   - Assigns confidence score (0.0-1.0)

4. **Iterative Refinement**: If confidence < 0.8
   - Generate refined search queries based on previous attempts
   - Repeat search and analysis up to 3 iterations
   - Learn from previous failures to improve queries

5. **Final Selection**: Download the highest-confidence match
   - Prefers official artist channels
   - Avoids covers, remixes, live versions (unless specifically requested)
   - Ensures exact song match, not similar songs

### Content Classification
```rust
enum MusicItemType {
    SongName(String),        // "Artist - Title" or natural language
    YoutubeUrl(String),      // Direct YouTube links
    SpotifyUrl(String),      // Spotify track/playlist links
    SoundCloudUrl(String),   // SoundCloud track links
    SongList(Vec<String>),   // Multiple songs (line-separated)
    Unknown,                 // Non-music content (ignored)
}
```

### Download Processing
1. **LLM Analysis**: Send content to Gemini 2.5 Flash for analysis
2. **Metadata Extraction**: Get artist, title, album, year information
3. **YouTube Search**: Find best quality official version
4. **Audio Download**: Use yt-dlp to extract MP3 audio
5. **Metadata Tagging**: Apply ID3 tags to downloaded MP3
6. **File Organization**: Save to Music/ClippyB Downloads folder

### System Tray Menu
- **Show Download History**: Display recent downloads with status icons
- **Open Music Folder**: Launch Windows Explorer to downloads directory
- **Clear History**: Remove download history (files remain)
- **Quit**: Gracefully shutdown application

## Development Patterns

### Error Handling
- Uses `thiserror` for structured error types
- All downloads wrapped in `Result<T, MusicDownloadError>`
- Network errors, LLM failures, and file I/O errors handled gracefully
- User notifications for both success and failure cases

### Async Processing
- Tokio async runtime for concurrent operations
- Separate task for download queue processing
- Non-blocking clipboard monitoring
- Rate limiting for batch downloads (1 second between songs)

### LLM Prompting
```rust
let prompt = format!(
    "Find the best YouTube video for this song: '{}'
    
    Return ONLY a JSON object:
    {{
      \"artist\": \"Artist Name\",
      \"title\": \"Song Title\", 
      \"album\": \"Album Name\",
      \"year\": 2023,
      \"youtube_url\": \"https://youtube.com/watch?v=VIDEO_ID\"
    }}
    
    Find official or highest quality version, avoid covers."
);
```

### File Naming and Organization
- Sanitized filenames: `Artist - Title.mp3`
- Invalid characters replaced with underscores
- Downloads saved to: `%USERPROFILE%/Music/ClippyB Downloads/`
- Duplicate handling by filename checking

## Debugging and Troubleshooting

### Common Issues
1. **yt-dlp not found**: Ensure yt-dlp is installed and in PATH
2. **API key missing**: Set GEMINI_API_KEY environment variable
3. **Network errors**: Check internet connection and firewall settings
4. **Permission errors**: Ensure write access to Music folder

### Logging
- Console output for all major operations
- Desktop notifications for user feedback
- Error messages include full context and suggestions

### Development Mode
```bash
# Enable detailed logging
set RUST_LOG=debug

# Enable panic backtraces  
set RUST_BACKTRACE=1

# Run with environment variables
set GEMINI_API_KEY=your_key && cargo run --manifest-path "src-tauri/Cargo.toml"
```

## Testing the Application

### Prerequisites Check
```bash
# Verify yt-dlp is available
yt-dlp --version

# Check if API key is set
echo %GEMINI_API_KEY%

# Ensure music folder is writable
mkdir "%USERPROFILE%\Music\ClippyB Downloads"
```

### Manual Testing Scenarios
1. **YouTube URL**: Copy `https://youtube.com/watch?v=dQw4w9WgXcQ` to clipboard
2. **Song Name**: Copy `Never Gonna Give You Up - Rick Astley` to clipboard
3. **Spotify URL**: Copy a Spotify track URL to clipboard
4. **Song List**: Copy multiple songs line-separated to clipboard

### Expected Behavior
- System tray notification appears for detected music
- Console output shows processing steps
- MP3 file appears in Music/ClippyB Downloads folder
- File has proper metadata tags when inspected

## Future Enhancement Areas

### Potential Features
- Support for additional music platforms (Apple Music, Tidal)
- Playlist download from URLs
- Audio quality selection
- Custom download directory configuration
- Download queue management UI
- Integration with music library managers
- Batch processing improvements
- Local song identification from audio fingerprinting

### Code Architecture Improvements
- Plugin system for different music platforms
- Configuration file support
- Background service mode
- Update mechanism
- Performance optimizations for large playlists