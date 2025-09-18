use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Duration};

pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
    delay_ms: u64,
}

impl RateLimiter {
    pub fn new(max_concurrent: usize, delay_ms: u64) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            delay_ms,
        }
    }
    
    pub async fn acquire(&self) -> Result<SemaphorePermit<'_>, tokio::sync::AcquireError> {
        let permit = self.semaphore.acquire().await?;
        if self.delay_ms > 0 {
            sleep(Duration::from_millis(self.delay_ms)).await;
        }
        Ok(permit)
    }
}

use tokio::sync::SemaphorePermit;