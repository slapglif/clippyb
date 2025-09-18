#!/usr/bin/env python3
"""
Test what the JSON schema should look like for Ollama structured output
"""

import json
import requests

# Test the current format we're sending vs what Ollama expects
OLLAMA_URL = "http://98.87.166.97:11434"
MODEL = "granite3.3:latest"

def test_format_simple_json():
    """Test with format: 'json' (what we were doing before)"""
    print("=== Test: format = 'json' ===")
    
    payload = {
        "model": MODEL,
        "prompt": "Generate JSON with format: {\"queries\": [\"query1\", \"query2\"]}. Return only valid JSON.",
        "format": "json",
        "stream": False
    }
    
    response = requests.post(f"{OLLAMA_URL}/api/generate", json=payload, timeout=30)
    if response.status_code == 200:
        data = response.json()
        print(f"Response: {data.get('response', '')}")
        return data.get('response', '')
    return None

def test_format_with_schema():
    """Test with format containing actual JSON schema"""
    print("\n=== Test: format = <schema> ===")
    
    # This is what the Rust schemars would generate
    schema = {
        "type": "object",
        "properties": {
            "queries": {
                "type": "array",
                "items": {"type": "string"},
                "description": "List of YouTube search queries to find the song"
            }
        },
        "required": ["queries"]
    }
    
    payload = {
        "model": MODEL,
        "prompt": "Generate JSON with format: {\"queries\": [\"query1\", \"query2\"]}. Return only valid JSON.",
        "format": schema,
        "stream": False
    }
    
    print(f"Schema being sent: {json.dumps(schema, indent=2)}")
    
    response = requests.post(f"{OLLAMA_URL}/api/generate", json=payload, timeout=30)
    if response.status_code == 200:
        data = response.json()
        print(f"Response: {data.get('response', '')}")
        return data.get('response', '')
    return None

def main():
    print("Testing Ollama Format Parameter Options")
    print("=" * 50)
    
    # Test both approaches
    response1 = test_format_simple_json()
    response2 = test_format_with_schema()
    
    print("\n" + "=" * 50)
    print("RESULTS:")
    
    if response1:
        try:
            json.loads(response1)
            print("✓ format='json' works and returns valid JSON")
        except:
            print("✗ format='json' returns invalid JSON")
    else:
        print("✗ format='json' failed")
        
    if response2:
        try:
            parsed = json.loads(response2)
            if "queries" in parsed and isinstance(parsed["queries"], list):
                print("✓ format=<schema> works and follows schema")
            else:
                print("✗ format=<schema> doesn't follow schema structure")
        except:
            print("✗ format=<schema> returns invalid JSON")
    else:
        print("✗ format=<schema> failed")

if __name__ == "__main__":
    main()