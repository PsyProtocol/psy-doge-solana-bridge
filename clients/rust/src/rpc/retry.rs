//! Retry logic with exponential backoff.
//!
//! Provides automatic retry for transient failures with configurable
//! backoff and jitter to prevent thundering herd.

use crate::config::RetryConfig;
use crate::errors::BridgeError;
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

/// Executor that handles retries with exponential backoff.
#[derive(Clone)]
pub struct RetryExecutor {
    config: RetryConfig,
}

impl RetryExecutor {
    /// Create a new retry executor with the given configuration.
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    /// Create a retry executor with default configuration.
    pub fn default_executor() -> Self {
        Self::new(RetryConfig::default())
    }

    /// Create a retry executor that doesn't retry (for testing).
    pub fn no_retry() -> Self {
        Self::new(RetryConfig {
            max_retries: 0,
            initial_delay_ms: 0,
            max_delay_ms: 0,
            backoff_multiplier: 1.0,
            confirmation_timeout_ms: 60_000,
        })
    }

    /// Execute an operation with retry logic.
    ///
    /// The operation will be retried up to `max_retries` times if it fails
    /// with a retryable error. Non-retryable errors are returned immediately.
    pub async fn execute<F, Fut, T>(&self, operation: F) -> Result<T, BridgeError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, BridgeError>>,
    {
        let mut attempts = 0;
        let mut delay = self.config.initial_delay_ms;

        loop {
            attempts += 1;

            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    tracing::warn!(
                        attempts = attempts,
                        error = %e,
                        "Operation failed"
                    );
                    // Check if we should retry
                    if !e.is_retryable() || attempts > self.config.max_retries {
                        return Err(e);
                    }

                    // Check for retry hint
                    if let Some(hint) = e.retry_hint_ms() {
                        delay = delay.max(hint);
                    }

                    // Add jitter (0-25% of delay)
                    let jitter = self.jitter(delay);
                    let wait_time = delay + jitter;

                    tracing::debug!(
                        attempts = attempts,
                        delay_ms = wait_time,
                        error = %e,
                        "Retrying after error"
                    );

                    sleep(Duration::from_millis(wait_time)).await;

                    // Exponential backoff
                    delay = ((delay as f64) * self.config.backoff_multiplier) as u64;
                    delay = delay.min(self.config.max_delay_ms);
                }
            }
        }
    }

    /// Execute an operation that returns a different error type.
    ///
    /// The error type must implement `Into<BridgeError>`.
    pub async fn execute_with<F, Fut, T, E>(&self, operation: F) -> Result<T, BridgeError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: Into<BridgeError>,
    {
        self.execute(|| async { operation().await.map_err(Into::into) })
            .await
    }

    /// Calculate jitter for the given delay.
    fn jitter(&self, delay: u64) -> u64 {
        // Use a simple pseudo-random jitter based on system time
        // This avoids needing the `rand` crate for basic jitter
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);

        // Jitter is 0-25% of delay
        let max_jitter = delay / 4;
        if max_jitter == 0 {
            0
        } else {
            (nanos as u64) % max_jitter
        }
    }

    /// Get the confirmation timeout from config.
    pub fn confirmation_timeout(&self) -> Duration {
        Duration::from_millis(self.config.confirmation_timeout_ms)
    }

    /// Get the maximum number of retries.
    pub fn max_retries(&self) -> u32 {
        self.config.max_retries
    }
}

/// Builder for creating retry executors with custom logic.
pub struct RetryExecutorBuilder {
    config: RetryConfig,
}

impl RetryExecutorBuilder {
    /// Create a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: RetryConfig::default(),
        }
    }

    /// Set the maximum number of retries.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.config.max_retries = retries;
        self
    }

    /// Set the initial delay in milliseconds.
    pub fn initial_delay_ms(mut self, delay: u64) -> Self {
        self.config.initial_delay_ms = delay;
        self
    }

    /// Set the maximum delay in milliseconds.
    pub fn max_delay_ms(mut self, delay: u64) -> Self {
        self.config.max_delay_ms = delay;
        self
    }

    /// Set the backoff multiplier.
    pub fn backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.config.backoff_multiplier = multiplier;
        self
    }

    /// Set the confirmation timeout in milliseconds.
    pub fn confirmation_timeout_ms(mut self, timeout: u64) -> Self {
        self.config.confirmation_timeout_ms = timeout;
        self
    }

    /// Build the retry executor.
    pub fn build(self) -> RetryExecutor {
        RetryExecutor::new(self.config)
    }
}

impl Default for RetryExecutorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_retry_succeeds_on_first_try() {
        let executor = RetryExecutor::default_executor();

        let result: Result<u32, BridgeError> = executor.execute(|| async { Ok(42) }).await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_after_failures() {
        let executor = RetryExecutor::new(RetryConfig {
            max_retries: 3,
            initial_delay_ms: 1,
            max_delay_ms: 10,
            backoff_multiplier: 2.0,
            confirmation_timeout_ms: 1000,
        });

        let attempts = AtomicU32::new(0);

        let result: Result<u32, BridgeError> = executor
            .execute(|| {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                async move {
                    if attempt < 2 {
                        Err(BridgeError::ConnectionTimeout)
                    } else {
                        Ok(42)
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_no_retry_on_non_retryable_error() {
        let executor = RetryExecutor::new(RetryConfig {
            max_retries: 5,
            initial_delay_ms: 1,
            max_delay_ms: 10,
            backoff_multiplier: 2.0,
            confirmation_timeout_ms: 1000,
        });

        let attempts = AtomicU32::new(0);

        let result: Result<u32, BridgeError> = executor
            .execute(|| {
                attempts.fetch_add(1, Ordering::SeqCst);
                async {
                    // InvalidInput is not retryable
                    Err(BridgeError::InvalidInput("test".to_string()))
                }
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_max_retries_exceeded() {
        let executor = RetryExecutor::new(RetryConfig {
            max_retries: 2,
            initial_delay_ms: 1,
            max_delay_ms: 10,
            backoff_multiplier: 2.0,
            confirmation_timeout_ms: 1000,
        });

        let attempts = AtomicU32::new(0);

        let result: Result<u32, BridgeError> = executor
            .execute(|| {
                attempts.fetch_add(1, Ordering::SeqCst);
                async { Err(BridgeError::ConnectionTimeout) }
            })
            .await;

        assert!(result.is_err());
        // Initial attempt + 2 retries = 3 attempts
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
