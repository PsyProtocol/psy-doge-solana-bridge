//! Token bucket rate limiter for RPC requests.
//!
//! Provides fair rate limiting with optional queuing to prevent
//! overwhelming RPC endpoints.

use crate::config::RateLimitConfig;
use crate::errors::BridgeError;
use std::num::NonZeroU32;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A token bucket rate limiter for RPC requests.
///
/// Uses the `governor` crate for efficient token bucket implementation.
/// Supports both immediate rejection and queuing when rate limited.
pub struct RpcRateLimiter {
    limiter: governor::RateLimiter<
        governor::state::NotKeyed,
        governor::state::InMemoryState,
        governor::clock::DefaultClock,
    >,
    config: RateLimitConfig,
    /// Queue for requests waiting for rate limit slots
    queue_size: Arc<Mutex<usize>>,
}

/// Guard returned when rate limit slot is acquired.
/// Exists mainly for future extensions (e.g., tracking active requests).
pub struct RateLimitGuard {
    _private: (),
}

impl RateLimitGuard {
    fn new() -> Self {
        Self { _private: () }
    }
}

impl RpcRateLimiter {
    /// Create a new rate limiter with the given configuration.
    pub fn new(config: RateLimitConfig) -> Self {
        let quota = governor::Quota::per_second(
            NonZeroU32::new(config.max_rps).unwrap_or(NonZeroU32::new(10).unwrap()),
        )
        .allow_burst(
            NonZeroU32::new(config.burst_size).unwrap_or(NonZeroU32::new(20).unwrap()),
        );

        let limiter = governor::RateLimiter::direct(quota);

        Self {
            limiter,
            config,
            queue_size: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a rate limiter with default configuration.
    pub fn default_limiter() -> Self {
        Self::new(RateLimitConfig::default())
    }

    /// Create a rate limiter that allows unlimited requests (for testing).
    pub fn unlimited() -> Self {
        Self::new(RateLimitConfig {
            max_rps: u32::MAX,
            burst_size: u32::MAX,
            queue_on_limit: false,
            max_queue_depth: 0,
        })
    }

    /// Acquire a rate limit slot.
    ///
    /// If rate limited:
    /// - If `queue_on_limit` is true, queues the request
    /// - Otherwise, waits for the next available slot
    ///
    /// Returns an error if the queue is full.
    pub async fn acquire(&self) -> Result<RateLimitGuard, BridgeError> {
        // Try immediate acquisition
        if self.limiter.check().is_ok() {
            return Ok(RateLimitGuard::new());
        }

        // Check queue depth
        let mut queue_size = self.queue_size.lock().await;
        if self.config.queue_on_limit && *queue_size >= self.config.max_queue_depth {
            return Err(BridgeError::RateLimited {
                retry_after_ms: self.estimated_wait_ms(),
            });
        }

        // Increment queue size while waiting
        *queue_size += 1;
        drop(queue_size);

        // Wait for next available slot
        self.limiter.until_ready().await;

        // Decrement queue size
        let mut queue_size = self.queue_size.lock().await;
        *queue_size = queue_size.saturating_sub(1);

        Ok(RateLimitGuard::new())
    }

    /// Try to acquire a rate limit slot without waiting.
    ///
    /// Returns None if rate limited.
    pub fn try_acquire(&self) -> Option<RateLimitGuard> {
        if self.limiter.check().is_ok() {
            Some(RateLimitGuard::new())
        } else {
            None
        }
    }

    /// Estimate wait time in milliseconds based on current rate.
    fn estimated_wait_ms(&self) -> u64 {
        // Simple estimate: time for one token at current rate
        1000 / self.config.max_rps as u64
    }

    /// Get the current queue depth.
    pub async fn queue_depth(&self) -> usize {
        *self.queue_size.lock().await
    }

    /// Check if the limiter would allow a request immediately.
    pub fn would_allow(&self) -> bool {
        self.limiter.check().is_ok()
    }
}

/// A wrapper that applies rate limiting to async operations.
pub struct RateLimitedExecutor<T> {
    inner: T,
    limiter: Arc<RpcRateLimiter>,
}

impl<T> RateLimitedExecutor<T> {
    /// Create a new rate-limited executor.
    pub fn new(inner: T, limiter: Arc<RpcRateLimiter>) -> Self {
        Self { inner, limiter }
    }

    /// Get a reference to the inner value.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Get a mutable reference to the inner value.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Execute an operation with rate limiting.
    pub async fn execute<F, Fut, R>(&self, f: F) -> Result<R, BridgeError>
    where
        F: FnOnce(&T) -> Fut,
        Fut: std::future::Future<Output = Result<R, BridgeError>>,
    {
        let _guard = self.limiter.acquire().await?;
        f(&self.inner).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_burst() {
        let limiter = RpcRateLimiter::new(RateLimitConfig {
            max_rps: 10,
            burst_size: 5,
            queue_on_limit: false,
            max_queue_depth: 0,
        });

        // Should allow burst of 5 immediately
        for _ in 0..5 {
            assert!(limiter.try_acquire().is_some());
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_after_burst() {
        let limiter = RpcRateLimiter::new(RateLimitConfig {
            max_rps: 100,
            burst_size: 2,
            queue_on_limit: false,
            max_queue_depth: 0,
        });

        // Exhaust burst
        limiter.try_acquire();
        limiter.try_acquire();

        // Next should be blocked
        assert!(limiter.try_acquire().is_none());
    }

    #[tokio::test]
    async fn test_unlimited_limiter() {
        let limiter = RpcRateLimiter::unlimited();

        // Should allow many requests immediately
        for _ in 0..1000 {
            assert!(limiter.try_acquire().is_some());
        }
    }
}
