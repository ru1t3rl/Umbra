//! gen-header fixture: one headed struct (`tests/gen_header.rs` appends a field to a copy).

#![forbid(unsafe_code)]

/// A headed struct.
#[repr(C)]
pub struct FixtureStruct {
    /// ABI version.
    pub abi_version: u32,
    /// Struct size.
    pub struct_size: u32,
}
