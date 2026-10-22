//! DIS versions and the header strategy (ADR 0034): the only place version differences live.
//!
//! The 12-byte common header has the same layout in IEEE 1278.1a-1998 (protocol version 6) and
//! IEEE 1278.1-2012 (version 7) except byte 0, the version itself, and byte 10, which is padding in
//! v6 and the PDU status bit field in v7. Bodies of the supported PDU families are identical.

#![deny(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used
)]

/// A supported DIS protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DisVersion {
    /// IEEE 1278.1a-1998, protocol version 6.
    V6,
    /// IEEE 1278.1-2012, protocol version 7.
    V7,
}

impl DisVersion {
    /// The protocol version byte of the header.
    #[must_use]
    pub const fn protocol_version(self) -> u8 {
        match self {
            DisVersion::V6 => 6,
            DisVersion::V7 => 7,
        }
    }

    /// The version a header byte names; `None` for versions this codec does not speak.
    #[must_use]
    pub const fn from_protocol_version(byte: u8) -> Option<Self> {
        match byte {
            6 => Some(DisVersion::V6),
            7 => Some(DisVersion::V7),
            _ => None,
        }
    }

    /// The header strategy of this version.
    #[must_use]
    pub fn header(self) -> &'static dyn HeaderStrategy {
        match self {
            DisVersion::V6 => &V6Header,
            DisVersion::V7 => &V7Header,
        }
    }
}

/// How a version lays out header bytes 10 and 11 (the two bytes after the length field).
pub trait HeaderStrategy: Sync {
    /// The version this strategy writes.
    fn version(&self) -> DisVersion;
    /// Bytes 10 and 11 for a PDU with this status.
    fn encode_status(&self, status: u8) -> [u8; 2];
    /// The PDU status carried by bytes 10 and 11 (0 when the version has none).
    fn decode_status(&self, bytes: [u8; 2]) -> u8;
}

/// v6: bytes 10 and 11 are padding, written as zero, ignored on decode.
#[derive(Debug, Clone, Copy)]
pub struct V6Header;

/// v7: byte 10 is the PDU status, byte 11 is padding.
#[derive(Debug, Clone, Copy)]
pub struct V7Header;

impl HeaderStrategy for V6Header {
    fn version(&self) -> DisVersion {
        DisVersion::V6
    }

    fn encode_status(&self, _status: u8) -> [u8; 2] {
        [0, 0]
    }

    fn decode_status(&self, _bytes: [u8; 2]) -> u8 {
        0
    }
}

impl HeaderStrategy for V7Header {
    fn version(&self) -> DisVersion {
        DisVersion::V7
    }

    fn encode_status(&self, status: u8) -> [u8; 2] {
        [status, 0]
    }

    fn decode_status(&self, bytes: [u8; 2]) -> u8 {
        let [status, _padding] = bytes;
        status
    }
}
