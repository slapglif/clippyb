use std::time::Duration;
use tokio::time::sleep;

pub struct RetryConfig {
    pub max_attempts: usize,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    pub fn youtube_search() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        }
    }
    
    pub fn download() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 1.5,
        }
    }
}

pub async fn retry_with_backoff<F, Fut, T, E>(
    operation: F,
    config: RetryConfig,
    operation_name: &str,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut delay = config.base_delay;
    
    for attempt in 1..=config.max_attempts {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                if attempt == config.max_attempts {
                    println!("❌ {} failed after {} attempts: {}", operation_name, config.max_attempts, error);
                    return Err(error);
                }
                
                println!("⚠️ {} attempt {}/{} failed: {}, retrying in {:?}", 
                        operation_name, attempt, config.max_attempts, error, delay);
                
                sleep(delay).await;
                
                // Exponential backoff with jitter
                delay = std::cmp::min(
                    Duration::from_millis((delay.as_millis() as f64 * config.backoff_multiplier) as u64),
                    config.max_delay
                );
            }
        }
    }
    
    unreachable!()
}