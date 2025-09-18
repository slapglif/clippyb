use serde::{Deserialize, Serialize};
use schemars::{JsonSchema, schema_for};
use serde_json;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct QueryList {
    #[schemars(description = "List of YouTube search queries to find the song")]
    queries: Vec<String>,
}

fn main() {
    // Generate the schema
    let schema = schema_for!(QueryList);
    
    // Pretty print the schema
    println!("Generated JSON Schema:");
    println!("{}", serde_json::to_string_pretty(&schema).unwrap());
    
    // Create the format parameter as ClippyB would
    let format_param = serde_json::json!({
        "format": schema
    });
    
    println!("\nFormat parameter for Ollama:");
    println!("{}", serde_json::to_string_pretty(&format_param).unwrap());
}