//! Priming (ADR 0024, review M1): a new subscription gets one control frame of the current state,
//! and only it gets that frame.

use super::thread::Worker;
use crate::bus::FrameCause;

impl Worker {
    /// Publishes a control frame of the current state to each subscription marked for priming.
    pub(super) fn serve_primes(&mut self) {
        for slot in self.shared.take_primes() {
            let frame = self.frame(FrameCause::Control, Vec::new(), None, false);
            self.bus.publish(frame, Some(slot));
        }
    }
}
