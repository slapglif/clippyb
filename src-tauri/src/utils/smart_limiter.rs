use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct SmartLimiter {
    semaphore: Arc<Semaphore>,
    max_concurrent: usize,
}

impl SmartLimiter {
    pub fn new() -> Self {
        // Default to number of CPU cores for optimal performance
        let cores = num_cpus::get();
        Self::with_limit(cores)
    }
    
    pub fn with_limit(max_concurrent: usize) -> Self {
        println!("🎛️ Smart limiter configured for {} concurrent operations", max_concurrent);
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            max_concurrent,
        }
    }
    
    pub async fn acquire(&self) -> Result<tokio::sync::SemaphorePermit<'_>, tokio::sync::AcquireError> {
        self.semaphore.acquire().await
    }
    
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
    
    pub fn max_permits(&self) -> usize {
        self.max_concurrent
    }
}

impl Clone for SmartLimiter {
    fn clone(&self) -> Self {
        Self {
            semaphore: Arc::clone(&self.semaphore),
            max_concurrent: self.max_concurrent,
        }
    }
}