#!/usr/bin/env python3
"""
Direct test of Ollama extractor functionality
"""

import requests
import json

# Test configuration
OLLAMA_URL = "http://98.87.166.97:11434"
MODEL = "granite3.3:latest"

def test_ollama_extractor():
    """Test if Ollama can extract structured JSON responses"""
    
    # Test 1: Simple JSON extraction
    print("=== Test 1: Simple JSON Response ===")
    prompt = "You are a music search expert. Generate effective YouTube search queries for the song 'Elohim - Half Alive'. You MUST return valid JSON in exactly this format: {\"queries\": [\"query1\", \"query2\", \"query3\"]}. Include 2-3 search query strings."
    
    payload = {
        "model": MODEL,
        "prompt": prompt,
        "format": "json",
        "stream": False
    }
    
    try:
        response = requests.post(f"{OLLAMA_URL}/api/generate", json=payload, timeout=30)
        if response.status_code == 200:
            data = response.json()
            json_response = data.get("response", "")
            print(f"Raw response: {json_response}")
            
            # Try to parse as JSON
            try:
                parsed = json.loads(json_response)
                if "queries" in parsed and isinstance(parsed["queries"], list):
                    print("SUCCESS: Valid JSON with queries array")
                    print(f"Queries: {parsed['queries']}")
                    return True
                else:
                    print("FAIL: JSON missing 'queries' array")
                    return False
            except json.JSONDecodeError as e:
                print(f"FAIL: Invalid JSON - {e}")
                return False
        else:
            print(f"FAIL: HTTP {response.status_code}")
            return False
            
    except Exception as e:
        print(f"FAIL: Exception - {e}")
        return False

def test_result_analysis():
    """Test result analysis structured output"""
    
    print("\n=== Test 2: Result Analysis ===")
    prompt = """You are a music search result analyzer. Select the best match for the song 'Elohim - Half Alive' from these results:

0. Half Alive by Elohim (3:45, 1,200,000 views)
1. Half Alive - Official Audio by Elohim Official (3:44, 500,000 views) 
2. Elohim Half Alive Lyrics by Various Artists (3:50, 100,000 views)

You MUST return valid JSON in exactly this format: {"query": "search query", "reasoning": "explanation", "selected_result_index": 0, "confidence": 0.8}. Use -1 for selected_result_index if no good match."""
    
    payload = {
        "model": MODEL,
        "prompt": prompt,
        "format": "json",
        "stream": False
    }
    
    try:
        response = requests.post(f"{OLLAMA_URL}/api/generate", json=payload, timeout=30)
        if response.status_code == 200:
            data = response.json()
            json_response = data.get("response", "")
            print(f"Raw response: {json_response}")
            
            try:
                parsed = json.loads(json_response)
                required_fields = ["query", "reasoning", "selected_result_index", "confidence"]
                if all(field in parsed for field in required_fields):
                    print("SUCCESS: Valid JSON with all required fields")
                    print(f"Selected index: {parsed['selected_result_index']}")
                    print(f"Confidence: {parsed['confidence']}")
                    print(f"Reasoning: {parsed['reasoning']}")
                    return True
                else:
                    missing = [f for f in required_fields if f not in parsed]
                    print(f"FAIL: Missing fields: {missing}")
                    return False
            except json.JSONDecodeError as e:
                print(f"FAIL: Invalid JSON - {e}")
                return False
        else:
            print(f"FAIL: HTTP {response.status_code}")
            return False
            
    except Exception as e:
        print(f"FAIL: Exception - {e}")
        return False

def main():
    print("Testing Ollama Structured Output for ClippyB")
    print("=" * 50)
    print(f"Model: {MODEL}")
    print(f"URL: {OLLAMA_URL}")
    print()
    
    # Run tests
    test1_passed = test_ollama_extractor()
    test2_passed = test_result_analysis()
    
    print("\n" + "=" * 50)
    if test1_passed and test2_passed:
        print("ALL TESTS PASSED - Ollama structured output is working!")
        print("The issue is likely in the ClippyB Rust code, not Ollama.")
    else:
        print("TESTS FAILED - Ollama structured output needs fixing")
        
        if not test1_passed:
            print("- Query extraction test failed")
        if not test2_passed:
            print("- Result analysis test failed")

if __name__ == "__main__":
    main()