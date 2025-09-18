/// Utilities for handling LLM responses
pub fn sanitize_llm_json_response(output: &str) -> String {
    let output = output.trim();
    
    // Remove markdown code blocks
    let output = if output.starts_with("```json") && output.ends_with("```") {
        output.trim_start_matches("```json").trim_end_matches("```").trim()
    } else if output.starts_with("```") && output.ends_with("```") {
        output.trim_start_matches("```").trim_end_matches("```").trim()
    } else {
        output
    };
    
    // Find the JSON array or object - handle trailing text
    if let Some(start) = output.find('[') {
        if let Some(end) = output.rfind(']') {
            return output[start..=end].to_string();
        }
    }
    
    if let Some(start) = output.find('{') {
        if let Some(end) = output.rfind('}') {
            return output[start..=end].to_string();
        }
    }
    
    output.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_thinking_blocks() {
        let input = r#"<think>
I need to analyze this request and generate search queries.
</think>

["artist song", "artist official", "artist music video"]"#;
        
        let result = sanitize_llm_json_response(input);
        assert_eq!(result, r#"["artist song", "artist official", "artist music video"]"#);
    }

    #[test]
    fn test_sanitize_markdown_blocks() {
        let input = r#"Here are the search queries:

```json
["query 1", "query 2", "query 3"]
```"#;
        
        let result = sanitize_llm_json_response(input);
        assert_eq!(result, r#"["query 1", "query 2", "query 3"]"#);
    }

    #[test]
    fn test_sanitize_mixed_content() {
        let input = r#"<thinking>
Let me think about this...
</thinking>

The queries are:
```json
{
  "queries": ["test 1", "test 2"],
  "confidence": 0.95
}
```

Additional explanation here..."#;
        
        let result = sanitize_llm_json_response(input);
        assert_eq!(result, r#"{
  "queries": ["test 1", "test 2"],
  "confidence": 0.95
}"#);
    }
}