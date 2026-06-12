use std::time::Duration;
use tokio::time::sleep;

/// Exponential-backoff wrapper: retries up to 3 times (1s, 2s, 4s).
#[derive(Debug, thiserror::Error)]
pub enum RetryError<E> {
    #[error("max retries exceeded: {0}")]
    MaxRetries(E),
}

/// Retry an async operation with exponential backoff.
///
/// # Errors
/// Returns [`RetryError::MaxRetries`] wrapping the last error after 3 failed attempts.
pub async fn retry_with_backoff<F, Fut, T, E>(mut f: F) -> Result<T, RetryError<E>>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut retries = 0;
    loop {
        match f().await {
            Ok(t) => return Ok(t),
            Err(e) => {
                if retries >= 2 {
                    return Err(RetryError::MaxRetries(e));
                }
                let delay = Duration::from_millis(1000 * (2_u64.pow(retries)));
                tracing::warn!(
                    "attempt {} failed ({}), backing off {:?}",
                    retries + 1,
                    e,
                    delay
                );
                sleep(delay).await;
                retries += 1;
            }
        }
    }
}
