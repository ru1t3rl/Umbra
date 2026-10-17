//! STUB (WI-7): owned versus remote entities (REQ-K-5, ASM-041).
#![expect(
    missing_docs,
    clippy::todo,
    reason = "stub until WI-7: documented when implemented"
)]

use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ownership {
    Owned,
    Remote { source: RemoteSource },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RemoteSource(Arc<str>);

impl RemoteSource {
    pub fn new(_s: &str) -> Self {
        todo!("WI-7")
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Ownership {
    pub fn is_owned(&self) -> bool {
        todo!("WI-7")
    }
}
