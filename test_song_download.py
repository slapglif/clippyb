#!/usr/bin/env python3
"""
Comprehensive ClippyB Song Download Test

This test verifies that ClippyB can successfully:
1. Accept a Spotify URL from clipboard  
2. Extract song metadata via Spotify API
3. Generate YouTube search queries via Ollama LLM
4. Search YouTube and analyze results
5. Download the selected song using yt-dlp

The test will FAIL until ClippyB can successfully download songs.
"""

import subprocess
import time
import json
import os
import sys
import tempfile
from pathlib import Path
import pyautogui
import pyperclip
import requests
from datetime import datetime

# Test configuration
SPOTIFY_TEST_URL = "https://open.spotify.com/track/2zD0c9Hm3rr7qNTWHEGhYC?si=abc123"  # Elohim - Half Alive
OLLAMA_URL = "http://98.87.166.97:11434"
GRANITE_MODEL = "granite3.3:latest"
CLIPPYB_EXE = r"C:\Users\MichaelBrown\work\clippyb\src-tauri\target\release\deps\clippyb-7d87575338260718.exe"
DOWNLOAD_DIR = Path.home() / "Downloads" / "ClippyB_Test"
MAX_WAIT_TIME = 120  # 2 minutes
POLL_INTERVAL = 2    # Check every 2 seconds

class ClippyBTestError(Exception):
    """Custom exception for ClippyB test failures"""
    pass

class ClippyBDownloadTest:
    def __init__(self):
        self.test_start_time = datetime.now()
        self.download_dir = DOWNLOAD_DIR
        self.download_dir.mkdir(parents=True, exist_ok=True)
        self.initial_files = set(self.download_dir.glob("*"))
        self.clippyb_process = None
        
    def log(self, message, level="INFO"):
        """Log a test message with timestamp"""
        timestamp = datetime.now().strftime("%H:%M:%S.%f")[:-3]
        # Remove unicode emojis for Windows console compatibility
        clean_message = message.encode('ascii', 'ignore').decode('ascii')
        print(f"[{timestamp}] {level}: {clean_message}")
        
    def verify_prerequisites(self):
        """Verify all prerequisites are met before starting test"""
        self.log("🔍 Verifying prerequisites...")
        
        # Check if ClippyB executable exists
        if not Path(CLIPPYB_EXE).exists():
            raise ClippyBTestError(f"ClippyB executable not found: {CLIPPYB_EXE}")
        self.log(f"✅ ClippyB executable found: {CLIPPYB_EXE}")
        
        # Check if Ollama is running and model is available
        try:
            response = requests.get(f"{OLLAMA_URL}/api/tags", timeout=5)
            if response.status_code == 200:
                models = response.json().get("models", [])
                model_names = [model["name"] for model in models]
                if GRANITE_MODEL in model_names:
                    self.log(f"✅ Ollama is running and {GRANITE_MODEL} model is available")
                else:
                    available = ", ".join(model_names)
                    raise ClippyBTestError(f"Model {GRANITE_MODEL} not found. Available: {available}")
            else:
                raise ClippyBTestError(f"Ollama API returned status {response.status_code}")
        except requests.RequestException as e:
            raise ClippyBTestError(f"Cannot connect to Ollama at {OLLAMA_URL}: {e}")
        
        # Test Ollama structured output capability
        try:
            test_prompt = {
                "model": GRANITE_MODEL,
                "prompt": "Generate a JSON response with format: {\"test\": \"success\"}. Return only valid JSON.",
                "format": "json"
            }
            response = requests.post(f"{OLLAMA_URL}/api/generate", 
                                   json=test_prompt, timeout=10)
            if response.status_code == 200:
                lines = response.text.strip().split('\n')
                for line in lines:
                    if line.strip():
                        data = json.loads(line)
                        if data.get("done", False):
                            result = data.get("response", "")
                            try:
                                json.loads(result)
                                self.log("✅ Ollama JSON structured output is working")
                                break
                            except json.JSONDecodeError:
                                raise ClippyBTestError(f"Ollama returned invalid JSON: {result}")
            else:
                raise ClippyBTestError(f"Ollama generate API returned status {response.status_code}")
        except Exception as e:
            raise ClippyBTestError(f"Ollama JSON format test failed: {e}")
            
        # Check if yt-dlp is available
        try:
            result = subprocess.run(["yt-dlp", "--version"], 
                                  capture_output=True, text=True, timeout=5)
            if result.returncode == 0:
                self.log(f"✅ yt-dlp is available: {result.stdout.strip()}")
            else:
                raise ClippyBTestError("yt-dlp not found or not working")
        except (subprocess.TimeoutExpired, FileNotFoundError) as e:
            raise ClippyBTestError(f"yt-dlp check failed: {e}")
            
        self.log("🎯 All prerequisites verified successfully!")
        
    def start_clippyb(self):
        """Start ClippyB application"""
        self.log("🚀 Starting ClippyB application...")
        try:
            # Start ClippyB process
            self.clippyb_process = subprocess.Popen(
                [CLIPPYB_EXE],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                cwd=Path(CLIPPYB_EXE).parent
            )
            
            # Give it a moment to initialize
            time.sleep(3)
            
            # Check if process is still running
            if self.clippyb_process.poll() is not None:
                stdout, stderr = self.clippyb_process.communicate()
                raise ClippyBTestError(f"ClippyB failed to start. STDOUT: {stdout}, STDERR: {stderr}")
                
            self.log("✅ ClippyB application started successfully")
            
        except Exception as e:
            raise ClippyBTestError(f"Failed to start ClippyB: {e}")
            
    def simulate_spotify_url_paste(self):
        """Simulate copying Spotify URL to clipboard and triggering ClippyB"""
        self.log(f"📋 Setting clipboard to Spotify URL: {SPOTIFY_TEST_URL}")
        
        try:
            # Copy Spotify URL to clipboard
            pyperclip.copy(SPOTIFY_TEST_URL)
            time.sleep(1)
            
            # Verify clipboard content
            clipboard_content = pyperclip.paste()
            if clipboard_content != SPOTIFY_TEST_URL:
                raise ClippyBTestError(f"Clipboard verification failed. Expected: {SPOTIFY_TEST_URL}, Got: {clipboard_content}")
                
            self.log("✅ Spotify URL copied to clipboard successfully")
            self.log("⏳ Waiting for ClippyB to detect and process the URL...")
            
        except Exception as e:
            raise ClippyBTestError(f"Failed to set clipboard: {e}")
            
    def wait_for_download_completion(self):
        """Wait for song download to complete"""
        self.log(f"⏳ Monitoring download directory: {self.download_dir}")
        
        start_time = time.time()
        last_file_count = len(self.initial_files)
        
        while time.time() - start_time < MAX_WAIT_TIME:
            current_files = set(self.download_dir.glob("*"))
            new_files = current_files - self.initial_files
            
            if new_files:
                # Check for completed audio files
                audio_files = [f for f in new_files 
                             if f.suffix.lower() in ['.mp3', '.m4a', '.wav', '.flac']]
                
                if audio_files:
                    for audio_file in audio_files:
                        # Check if file is completely downloaded (size > 1MB and not growing)
                        if audio_file.stat().st_size > 1024 * 1024:  # > 1MB
                            time.sleep(2)  # Wait a bit more
                            size_after_wait = audio_file.stat().st_size
                            
                            if size_after_wait == audio_file.stat().st_size:
                                self.log(f"🎵 Downloaded audio file: {audio_file.name} ({size_after_wait / 1024 / 1024:.1f} MB)")
                                return audio_file
                                
                # Check for any other new files
                if len(new_files) != last_file_count:
                    self.log(f"📁 New files detected: {[f.name for f in new_files]}")
                    last_file_count = len(new_files)
            
            # Check if ClippyB is still running
            if self.clippyb_process.poll() is not None:
                stdout, stderr = self.clippyb_process.communicate()
                raise ClippyBTestError(f"ClippyB process terminated unexpectedly. STDOUT: {stdout}, STDERR: {stderr}")
            
            time.sleep(POLL_INTERVAL)
            
        raise ClippyBTestError(f"Download did not complete within {MAX_WAIT_TIME} seconds")
        
    def verify_download(self, audio_file):
        """Verify the downloaded audio file is valid"""
        self.log(f"🔍 Verifying downloaded file: {audio_file.name}")
        
        # Check file size (should be reasonable for a song)
        file_size_mb = audio_file.stat().st_size / 1024 / 1024
        if file_size_mb < 1:
            raise ClippyBTestError(f"Downloaded file too small: {file_size_mb:.1f} MB")
        if file_size_mb > 50:
            self.log(f"⚠️ Downloaded file unusually large: {file_size_mb:.1f} MB")
            
        # Try to get metadata using yt-dlp
        try:
            result = subprocess.run([
                "yt-dlp", "--print", "title,uploader,duration", 
                "--no-download", str(audio_file)
            ], capture_output=True, text=True, timeout=10)
            
            if result.returncode == 0:
                metadata = result.stdout.strip().split('\n')
                self.log(f"🎵 Song metadata: Title='{metadata[0]}', Uploader='{metadata[1]}', Duration='{metadata[2]}'")
            else:
                self.log(f"⚠️ Could not extract metadata: {result.stderr}")
                
        except Exception as e:
            self.log(f"⚠️ Metadata extraction failed: {e}")
            
        # Check if filename contains expected song info
        filename_lower = audio_file.name.lower()
        if "elohim" in filename_lower and "half alive" in filename_lower.replace("_", " "):
            self.log("✅ Filename contains expected song information")
        else:
            self.log(f"⚠️ Filename may not match expected song: {audio_file.name}")
            
        self.log(f"✅ Download verification completed: {audio_file.name}")
        return True
        
    def cleanup(self):
        """Clean up test resources"""
        self.log("🧹 Cleaning up test resources...")
        
        if self.clippyb_process:
            try:
                self.clippyb_process.terminate()
                self.clippyb_process.wait(timeout=5)
                self.log("✅ ClippyB process terminated")
            except subprocess.TimeoutExpired:
                self.clippyb_process.kill()
                self.log("⚠️ ClippyB process killed (did not terminate gracefully)")
            except Exception as e:
                self.log(f"⚠️ Error terminating ClippyB: {e}")
                
        # Optionally clean up downloaded test files
        # (Comment out if you want to keep the files for inspection)
        # try:
        #     current_files = set(self.download_dir.glob("*"))
        #     test_files = current_files - self.initial_files
        #     for test_file in test_files:
        #         test_file.unlink()
        #         self.log(f"🗑️ Removed test file: {test_file.name}")
        # except Exception as e:
        #     self.log(f"⚠️ Error cleaning up test files: {e}")
            
    def run_test(self):
        """Run the complete ClippyB download test"""
        self.log("🎯 Starting ClippyB Song Download Test")
        self.log(f"📋 Test URL: {SPOTIFY_TEST_URL}")
        self.log(f"📁 Download Directory: {self.download_dir}")
        self.log(f"🤖 LLM: {GRANITE_MODEL} at {OLLAMA_URL}")
        
        try:
            # Step 1: Verify prerequisites
            self.verify_prerequisites()
            
            # Step 2: Start ClippyB
            self.start_clippyb()
            
            # Step 3: Simulate Spotify URL paste
            self.simulate_spotify_url_paste()
            
            # Step 4: Wait for download completion
            downloaded_file = self.wait_for_download_completion()
            
            # Step 5: Verify download
            self.verify_download(downloaded_file)
            
            # Test passed!
            test_duration = (datetime.now() - self.test_start_time).total_seconds()
            self.log(f"🎉 TEST PASSED! ClippyB successfully downloaded song in {test_duration:.1f} seconds")
            self.log(f"📄 Downloaded: {downloaded_file.name}")
            return True
            
        except ClippyBTestError as e:
            self.log(f"❌ TEST FAILED: {e}", level="ERROR")
            return False
        except Exception as e:
            self.log(f"💥 UNEXPECTED ERROR: {e}", level="ERROR")
            return False
        finally:
            self.cleanup()

def main():
    """Main test execution function"""
    print("=" * 80)
    print("ClippyB Comprehensive Song Download Test")
    print("=" * 80)
    print()
    
    # Check if required Python packages are installed
    try:
        import pyautogui
        import pyperclip
        import requests
    except ImportError as e:
        missing_package = str(e).split("'")[1]
        print(f"❌ Missing required package: {missing_package}")
        print(f"   Install with: pip install {missing_package}")
        sys.exit(1)
    
    # Run the test
    test = ClippyBDownloadTest()
    success = test.run_test()
    
    print()
    print("=" * 80)
    if success:
        print("RESULT: TEST PASSED - ClippyB can successfully download songs!")
        sys.exit(0)
    else:
        print("RESULT: TEST FAILED - ClippyB cannot download songs yet")
        sys.exit(1)

if __name__ == "__main__":
    main()