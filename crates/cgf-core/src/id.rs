//! Entity and namespaced string IDs (REQ-C-2, ADR 0002). STUB (M0 step 4): signatures only,
//! bodies are `todo!()` until WI-7.
#![allow(clippy::todo, missing_docs)]

pub const MAX_ID_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(core::num::NonZeroU64);
impl EntityId {
    pub const fn new(raw: u64) -> Option<Self> {
        let _ = raw;
        todo!()
    }
    pub const fn get(self) -> u64 {
        todo!()
    }
}
impl core::fmt::Display for EntityId {
    fn fmt(&self, _: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        todo!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("id is empty")]
    Empty,
    #[error("id `{id}` is {len} bytes; max is {max}")]
    TooLong { id: String, len: usize, max: usize },
    #[error("id `{id}` needs at least two dot-separated segments")]
    MissingNamespace { id: String },
    #[error("id `{id}` has an empty segment at byte {index}")]
    EmptySegment { id: String, index: usize },
    #[error("id `{id}` has invalid character {ch:?} at byte {index}")]
    InvalidChar { id: String, ch: char, index: usize },
    #[error("id `{id}`: segment at byte {index} must start with a-z")]
    InvalidSegmentStart { id: String, index: usize },
}

macro_rules! id_stub {
    ($($name:ident),*) => {$(
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(std::sync::Arc<str>);
        impl $name {
            pub fn new(s: &str) -> Result<Self, IdError> { let _ = s; todo!() }
            pub fn as_str(&self) -> &str { todo!() }
        }
        impl core::str::FromStr for $name {
            type Err = IdError;
            fn from_str(s: &str) -> Result<Self, IdError> { let _ = s; todo!() }
        }
        impl TryFrom<&str> for $name {
            type Error = IdError;
            fn try_from(s: &str) -> Result<Self, IdError> { let _ = s; todo!() }
        }
        impl TryFrom<String> for $name {
            type Error = IdError;
            fn try_from(s: String) -> Result<Self, IdError> { let _ = s; todo!() }
        }
        impl core::fmt::Display for $name {
            fn fmt(&self, _: &mut core::fmt::Formatter<'_>) -> core::fmt::Result { todo!() }
        }
        impl AsRef<str> for $name { fn as_ref(&self) -> &str { todo!() } }
    )*};
}
id_stub!(TemplateId, BehaviorId, MotionModelId, PluginId);
