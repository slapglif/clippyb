import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { appWindow } from "@tauri-apps/api/window";
import "./App.css";

interface DownloadProgress {
  current: number;
  total: number;
  currentSong: string;
  status: string;
}

interface Config {
  provider: string;
  url?: string;
  model?: string;
  apiKey?: string;
  numContext?: number;
  musicFolder: string;
}

function App() {
  const [url, setUrl] = useState("");
  const [songList, setSongList] = useState("");
  const [downloads, setDownloads] = useState<string[]>([]);
  const [progress, setProgress] = useState<DownloadProgress>({
    current: 0,
    total: 0,
    currentSong: "",
    status: "idle"
  });
  const [config, setConfig] = useState<Config>({
    provider: "ollama",
    url: "http://98.87.166.97:11434",
    model: "granite3.3",
    numContext: 12000,
    musicFolder: ""
  });
  const [showConfig, setShowConfig] = useState(false);
  const progressInterval = useRef<number>();

  useEffect(() => {
    // Load initial config
    loadConfig();
    
    // Listen for download updates
    const unlisten = setupListeners();
    
    return () => {
      if (progressInterval.current) {
        clearInterval(progressInterval.current);
      }
      unlisten?.then(fn => fn());
    };
  }, []);

  async function loadConfig() {
    try {
      const cfg = await invoke<Config>("get_config");
      setConfig(cfg);
    } catch (e) {
      console.error("Failed to load config:", e);
    }
  }

  async function setupListeners() {
    // This would be implemented with Tauri events
    // For now, we'll simulate progress
    return null;
  }

  async function minimizeToTray() {
    await appWindow.hide();
  }

  async function processUrl() {
    if (!url.trim()) return;
    
    setProgress({
      current: 0,
      total: 1,
      currentSong: url,
      status: "processing"
    });
    
    try {
      await invoke("process_url", { url });
      setDownloads([...downloads, url]);
      setUrl("");
    } catch (e) {
      console.error("Failed to process URL:", e);
    }
  }

  async function processSongList() {
    if (!songList.trim()) return;
    
    const songs = songList.split('\n').filter(s => s.trim());
    setProgress({
      current: 0,
      total: songs.length,
      currentSong: songs[0],
      status: "processing"
    });
    
    try {
      await invoke("process_song_list", { songs });
      setDownloads([...downloads, ...songs]);
      setSongList("");
    } catch (e) {
      console.error("Failed to process songs:", e);
    }
  }

  async function saveConfig() {
    try {
      await invoke("save_config", { config });
      setShowConfig(false);
    } catch (e) {
      console.error("Failed to save config:", e);
    }
  }

  async function browseFolder() {
    // This would open a folder picker dialog
    // For now, we'll just use the default
  }

  return (
    <main className="container">
      <div className="header">
        <h1>🎵 ClippyB Music Downloader</h1>
        <div className="controls">
          <button onClick={() => setShowConfig(!showConfig)}>⚙️ Config</button>
          <button onClick={minimizeToTray}>➖ Minimize</button>
        </div>
      </div>

      {showConfig ? (
        <div className="config-section">
          <h2>Configuration</h2>
          <div className="config-group">
            <label>LLM Provider:</label>
            <select value={config.provider} onChange={e => setConfig({...config, provider: e.target.value})}>
              <option value="ollama">Ollama</option>
              <option value="openai">OpenAI</option>
              <option value="claude">Claude</option>
              <option value="gemini">Gemini</option>
            </select>
          </div>
          
          {config.provider === "ollama" && (
            <>
              <div className="config-group">
                <label>URL:</label>
                <input 
                  type="text" 
                  value={config.url} 
                  onChange={e => setConfig({...config, url: e.target.value})}
                />
              </div>
              <div className="config-group">
                <label>Model:</label>
                <input 
                  type="text" 
                  value={config.model} 
                  onChange={e => setConfig({...config, model: e.target.value})}
                />
              </div>
              <div className="config-group">
                <label>Context Size:</label>
                <input 
                  type="number" 
                  value={config.numContext} 
                  onChange={e => setConfig({...config, numContext: parseInt(e.target.value)})}
                />
              </div>
            </>
          )}
          
          {config.provider !== "ollama" && (
            <div className="config-group">
              <label>API Key:</label>
              <input 
                type="password" 
                value={config.apiKey || ""} 
                onChange={e => setConfig({...config, apiKey: e.target.value})}
                placeholder="Enter your API key"
              />
            </div>
          )}
          
          <div className="config-group">
            <label>Music Folder:</label>
            <div className="folder-picker">
              <input 
                type="text" 
                value={config.musicFolder} 
                onChange={e => setConfig({...config, musicFolder: e.target.value})}
              />
              <button onClick={browseFolder}>Browse</button>
            </div>
          </div>
          
          <div className="config-actions">
            <button onClick={saveConfig}>Save</button>
            <button onClick={() => setShowConfig(false)}>Cancel</button>
          </div>
        </div>
      ) : (
        <>
          <div className="input-section">
            <div className="url-input">
              <h3>Single URL/Song:</h3>
              <div className="input-group">
                <input
                  type="text"
                  value={url}
                  onChange={e => setUrl(e.target.value)}
                  placeholder="Paste YouTube/Spotify URL or song name..."
                  onKeyPress={e => e.key === 'Enter' && processUrl()}
                />
                <button onClick={processUrl}>Download</button>
              </div>
            </div>

            <div className="list-input">
              <h3>Song List:</h3>
              <textarea
                value={songList}
                onChange={e => setSongList(e.target.value)}
                placeholder="Paste multiple songs (one per line)..."
                rows={5}
              />
              <button onClick={processSongList}>Download All</button>
            </div>
          </div>

          {progress.status !== "idle" && (
            <div className="progress-section">
              <h3>Download Progress:</h3>
              <div className="progress-info">
                {progress.total > 1 ? (
                  <span>Downloading {progress.current} of {progress.total} files...</span>
                ) : (
                  <span>Downloading...</span>
                )}
              </div>
              <div className="current-song">{progress.currentSong}</div>
              <div className="progress-bar">
                <div 
                  className="progress-fill" 
                  style={{width: `${(progress.current / progress.total) * 100}%`}}
                />
              </div>
            </div>
          )}

          <div className="downloads-section">
            <h3>Recent Downloads:</h3>
            <ul className="download-list">
              {downloads.slice(-10).reverse().map((song, idx) => (
                <li key={idx}>✅ {song}</li>
              ))}
            </ul>
          </div>
        </>
      )}
    </main>
  );
}

export default App;