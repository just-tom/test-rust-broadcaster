//! Connection state management.

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{BASE_RECONNECT_DELAY_MS, MAX_RECONNECT_ATTEMPTS};

/// Connection state for the RTMP client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionState {
    /// Not connected.
    Disconnected,

    /// Connecting to server.
    Connecting,

    /// Connected and streaming.
    Connected,

    /// Attempting to reconnect.
    Reconnecting { attempt: u32 },

    /// Connection failed permanently.
    Failed { reason: String },
}

impl ConnectionState {
    /// Check if connected.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Check if in a transient state (connecting or reconnecting).
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Connecting | Self::Reconnecting { .. })
    }

    /// Check if failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }

    /// Get status message for UI.
    pub fn message(&self) -> String {
        match self {
            Self::Disconnected => "Disconnected".to_string(),
            Self::Connecting => "Connecting...".to_string(),
            Self::Connected => "Connected".to_string(),
            Self::Reconnecting { attempt } => format!("Reconnecting ({}/{})", attempt, MAX_RECONNECT_ATTEMPTS),
            Self::Failed { reason } => format!("Failed: {}", reason),
        }
    }
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

/// Reconnection policy configuration.
#[derive(Debug, Clone)]
pub struct ReconnectPolicy {
    /// Maximum number of reconnection attempts.
    pub max_attempts: u32,

    /// Base delay between attempts (exponential backoff applied).
    pub base_delay: Duration,

    /// Maximum delay between attempts.
    pub max_delay: Duration,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            max_attempts: MAX_RECONNECT_ATTEMPTS,
            base_delay: Duration::from_millis(BASE_RECONNECT_DELAY_MS),
            max_delay: Duration::from_secs(10),
        }
    }
}

impl ReconnectPolicy {
    /// Calculate delay for a given attempt number.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier = 2u64.pow(attempt.saturating_sub(1));
        let delay = self.base_delay.saturating_mul(multiplier as u32);
        delay.min(self.max_delay)
    }

    /// Check if more attempts are allowed.
    pub fn should_retry(&self, attempt: u32) -> bool {
        attempt < self.max_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconnect_policy_delays() {
        let policy = ReconnectPolicy::default();

        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(1000));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(2000));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(4000));
    }

    #[test]
    fn test_reconnect_policy_should_retry() {
        let policy = ReconnectPolicy::default();

        assert!(policy.should_retry(0));
        assert!(policy.should_retry(1));
        assert!(policy.should_retry(2));
        assert!(!policy.should_retry(3));
    }
}
