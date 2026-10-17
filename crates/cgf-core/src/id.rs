//! Entity and namespaced string IDs (REQ-C-2, ADR 0002, ASM-017).
//!
//! Grammar of the string IDs: `segment ("." segment)+`, `segment = [a-z][a-z0-9_]*`,
//! at most [`MAX_ID_LEN`] bytes. One private validator is shared by all string-ID newtypes.

use core::fmt;
use core::num::NonZeroU64;
use core::str::FromStr;
use std::sync::Arc;

/// Maximum length of a string ID in bytes.
pub const MAX_ID_LEN: usize = 128;

/// Maximum number of bytes of an over-long ID echoed in [`IdError::TooLong`].
const ERROR_PREFIX_LEN: usize = 64;

/// Identifier of a live entity. Never zero (ASM-017); allocated by the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(NonZeroU64);

impl EntityId {
    /// Creates an ID from a raw value; returns `None` for zero.
    pub const fn new(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(n) => Some(Self(n)),
            None => None,
        }
    }

    /// The raw non-zero value.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Why a string was rejected as a namespaced ID. `index` values are byte offsets.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IdError {
    /// The input was empty.
    #[error("id is empty")]
    Empty,
    /// The input exceeds [`MAX_ID_LEN`]; `id` is only a bounded prefix of it.
    #[error("id {id:?} is {len} bytes; max is {max}")]
    TooLong {
        /// A prefix of the offending input (at most 64 bytes).
        id: String,
        /// Real length in bytes.
        len: usize,
        /// The maximum, [`MAX_ID_LEN`].
        max: usize,
    },
    /// The input has a single segment.
    #[error("id {id:?} needs at least two dot-separated segments")]
    MissingNamespace {
        /// The offending input.
        id: String,
    },
    /// Two dots in a row, or a leading or trailing dot.
    #[error("id {id:?} has an empty segment at byte {index}")]
    EmptySegment {
        /// The offending input.
        id: String,
        /// Byte offset where the empty segment begins.
        index: usize,
    },
    /// A character outside `[a-z0-9_.]`.
    #[error("id {id:?} has invalid character {ch:?} at byte {index}")]
    InvalidChar {
        /// The offending input.
        id: String,
        /// The character.
        ch: char,
        /// Byte offset of the character.
        index: usize,
    },
    /// A segment starts with a digit or underscore.
    #[error("id {id:?}: segment at byte {index} must start with a-z")]
    InvalidSegmentStart {
        /// The offending input.
        id: String,
        /// Byte offset where the segment starts.
        index: usize,
    },
}

/// Validates `s` against the ADR 0002 grammar.
fn validate(s: &str) -> Result<(), IdError> {
    if s.is_empty() {
        return Err(IdError::Empty);
    }
    if s.len() > MAX_ID_LEN {
        let end = (0..=ERROR_PREFIX_LEN)
            .rev()
            .find(|&i| s.is_char_boundary(i))
            .unwrap_or(0);
        return Err(IdError::TooLong {
            id: s[..end].to_owned(),
            len: s.len(),
            max: MAX_ID_LEN,
        });
    }
    let own = || s.to_owned();
    let mut segments = 0usize;
    let mut segment_start = 0usize;
    for (index, ch) in s.char_indices() {
        let at_start = index == segment_start;
        match ch {
            '.' if at_start => return Err(IdError::EmptySegment { id: own(), index }),
            '.' => {
                segments += 1;
                segment_start = index + 1;
            }
            '0'..='9' | '_' if at_start => {
                return Err(IdError::InvalidSegmentStart {
                    id: own(),
                    index: segment_start,
                });
            }
            'a'..='z' | '0'..='9' | '_' => {}
            _ => {
                return Err(IdError::InvalidChar {
                    id: own(),
                    ch,
                    index,
                });
            }
        }
    }
    if segment_start == s.len() {
        return Err(IdError::EmptySegment {
            id: own(),
            index: segment_start,
        });
    }
    if segments == 0 {
        return Err(IdError::MissingNamespace { id: own() });
    }
    Ok(())
}

macro_rules! string_id {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        ///
        /// Ordering is byte-lexicographic on the string, so it is a deterministic map key.
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Arc<str>);

        impl $name {
            /// Validates `s` and creates the ID.
            ///
            /// # Errors
            /// Returns the [`IdError`] describing the first grammar violation.
            pub fn new(s: &str) -> Result<Self, IdError> {
                validate(s)?;
                Ok(Self(Arc::from(s)))
            }

            /// The ID as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = IdError;
            fn from_str(s: &str) -> Result<Self, IdError> {
                Self::new(s)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdError;
            fn try_from(s: &str) -> Result<Self, IdError> {
                Self::new(s)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;
            fn try_from(s: String) -> Result<Self, IdError> {
                Self::new(&s)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    )*};
}

string_id!(
    /// Identifier of an entity template, e.g. `tank.mbt_generic`.
    TemplateId,
    /// Identifier of a behavior, e.g. `acme.behavior.patrol`.
    BehaviorId,
    /// Identifier of a motion model, e.g. `builtin.ground.kinematic`.
    MotionModelId,
    /// Identifier of a plugin, e.g. `acme.tools`.
    PluginId,
    /// Identifier of a registered component of any kind, e.g. `builtin.ground.kinematic`.
    ComponentId,
    /// A weapon label carried by `Intent::Fire` (ADR 0002 grammar), e.g. `acme.cannon_120`.
    WeaponId,
);
