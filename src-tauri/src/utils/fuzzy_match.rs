use std::path::Path;
use std::fs;

/// Simple fuzzy matching for song detection
pub struct FuzzyMatcher;

impl FuzzyMatcher {
    /// Normalize a string for comparison (lowercase, remove special chars)
    fn normalize(s: &str) -> String {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
    
    /// Check if a song already exists in the directory  
    pub fn song_exists(artist: &str, title: &str, music_dir: &Path) -> bool {
        let normalized_artist = Self::normalize(artist);
        let normalized_title = Self::normalize(title);
        
        if let Ok(entries) = fs::read_dir(music_dir) {
            for entry in entries.flatten() {
                if let Some(filename) = entry.file_name().to_str() {
                    if filename.ends_with(".mp3") || filename.ends_with(".m4a") {
                        let normalized_filename = Self::normalize(filename);
                        
                        // More aggressive matching - check for partial matches too
                        if (normalized_filename.contains(&normalized_artist) && 
                            normalized_filename.contains(&normalized_title)) ||
                           (normalized_artist.len() > 3 && normalized_filename.contains(&normalized_artist)) ||
                           (normalized_title.len() > 3 && normalized_filename.contains(&normalized_title)) {
                            return true;
                        }
                    }
                }
            }
        }
        
        false
    }
    
    /// Calculate similarity score between two strings (0.0 to 1.0)
    pub fn similarity_score(s1: &str, s2: &str) -> f32 {
        let n1 = Self::normalize(s1);
        let n2 = Self::normalize(s2);
        
        if n1.is_empty() || n2.is_empty() {
            return 0.0;
        }
        
        let words1: Vec<&str> = n1.split_whitespace().collect();
        let words2: Vec<&str> = n2.split_whitespace().collect();
        
        let mut matches = 0;
        let total = words1.len().max(words2.len());
        
        for word in &words1 {
            if words2.contains(word) {
                matches += 1;
            }
        }
        
        matches as f32 / total as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_normalize() {
        assert_eq!(FuzzyMatcher::normalize("Rick Astley - Never Gonna Give You Up"), "rick astley never gonna give you up");
        assert_eq!(FuzzyMatcher::normalize("The Beatles (1968) - Hey Jude [HD]"), "the beatles 1968 hey jude hd");
    }
    
    #[test]
    fn test_similarity() {
        assert!(FuzzyMatcher::similarity_score("Rick Astley", "rick astley") > 0.9);
        assert!(FuzzyMatcher::similarity_score("Never Gonna Give You Up", "Never Gonna Give U Up") > 0.7);
    }
}