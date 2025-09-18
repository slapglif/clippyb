# Spotify API Setup (Optional but Recommended)

ClippyB now supports native Spotify metadata extraction for much better accuracy when processing Spotify URLs!

## Benefits of Spotify API Integration

- **Accurate metadata**: Get the exact artist, song title, and album info directly from Spotify
- **No more LLM guessing**: Eliminates incorrect song identification 
- **Faster processing**: Direct API calls are much faster than yt-dlp parsing
- **Better reliability**: Official API results vs. web scraping

## Setup Instructions

### 1. Create a Spotify App

1. Go to https://developer.spotify.com/dashboard
2. Log in with your Spotify account
3. Click "Create an app"
4. Fill in:
   - **App name**: `ClippyB Music Downloader`
   - **App description**: `Personal music downloader for metadata extraction`
   - **Website**: Leave blank or use `http://localhost`
   - Check the boxes for terms of service

### 2. Get Your Credentials

1. Once created, you'll see your app dashboard
2. Note down:
   - **Client ID**: (visible on the dashboard)
   - **Client Secret**: Click "Show Client Secret"

### 3. Set Environment Variables

Add these to your system environment variables:

```bash
SPOTIFY_CLIENT_ID=your_client_id_here
SPOTIFY_CLIENT_SECRET=your_client_secret_here
```

**Windows (PowerShell):**
```powershell
[Environment]::SetEnvironmentVariable("SPOTIFY_CLIENT_ID", "your_client_id_here", "User")
[Environment]::SetEnvironmentVariable("SPOTIFY_CLIENT_SECRET", "your_client_secret_here", "User")
```

**Windows (Command Prompt):**
```cmd
setx SPOTIFY_CLIENT_ID "your_client_id_here"
setx SPOTIFY_CLIENT_SECRET "your_client_secret_here"
```

### 4. Restart ClippyB

After setting the environment variables, restart ClippyB. You should see:
```
✅ Spotify API client initialized
```

If the variables aren't set, you'll see:
```
⚠️ Spotify API not available (set SPOTIFY_CLIENT_ID and SPOTIFY_CLIENT_SECRET for better metadata)
```

## How It Works

1. **With Spotify API**: When you copy Spotify URLs, ClippyB extracts the track ID and calls the Spotify Web API to get exact artist and song information.

2. **Without Spotify API**: ClippyB falls back to yt-dlp and LLM-based extraction, which may be less accurate.

3. **Multiple URLs**: ClippyB now properly detects lists of Spotify URLs and processes each one individually.

## Example

**Input:** 14 Spotify URLs copied to clipboard
```
https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC
https://open.spotify.com/track/2tUBcZzrBKGj6Z2Zc7p8CW
... (12 more URLs)
```

**Output:** Each URL is processed individually with accurate metadata from Spotify's API, then the best YouTube version is found using the ReAct search pattern.

## Notes

- This uses the Spotify Web API Client Credentials flow (no user login required)
- Only needs "read" access to public track information
- Free Spotify account is sufficient
- Rate limiting is handled automatically
- Falls back gracefully if API is unavailable
