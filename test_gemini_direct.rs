// Test Gemini API directly without rig
use reqwest;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = "AIzaSyDepY_ZOJPQCmz62H8K23LB_TH2CVGyoT4";
    let model = "gemini-2.5-flash-lite";
    
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}/generateContent?key={}",
        model, api_key
    );
    
    let body = json!({
        "contents": [{
            "parts": [{
                "text": "Is 'epa-format-integration' related to music? Answer with only YES or NO."
            }]
        }],
        "generationConfig": {
            "temperature": 0.1,
            "topK": 1,
            "topP": 0.8,
            "maxOutputTokens": 10
        }
    });
    
    println!("Request URL: {}", url);
    println!("Request body: {}", serde_json::to_string_pretty(&body)?);
    
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .json(&body)
        .send()
        .await?;
    
    let status = response.status();
    let text = response.text().await?;
    
    println!("\nResponse status: {}", status);
    println!("Response body: {}", text);
    
    Ok(())
}