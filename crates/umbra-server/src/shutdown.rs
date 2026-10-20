//! The server's shutdown signal (ADR 0026, review M12): one trigger, many waiters.

use tokio::sync::watch;

/// A cloneable one-shot signal. Every clone sees the trigger, also when it is cloned or starts waiting
/// after it fired.
#[derive(Debug, Clone)]
pub struct Shutdown {
    sender: watch::Sender<bool>,
    receiver: watch::Receiver<bool>,
}

impl Shutdown {
    /// A signal that has not fired.
    #[must_use]
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self { sender, receiver }
    }

    /// Fires the signal; further calls do nothing.
    pub fn trigger(&self) {
        self.sender.send_replace(true);
    }

    /// Whether the signal has fired.
    #[must_use]
    pub fn is_triggered(&self) -> bool {
        *self.receiver.borrow()
    }

    /// Completes once the signal has fired (at once if it already has).
    pub async fn triggered(&self) {
        let mut receiver = self.receiver.clone();
        // The sender lives in `self`, so the channel cannot close while we wait.
        let _ = receiver.wait_for(|fired| *fired).await;
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}
