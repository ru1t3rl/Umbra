#![forbid(unsafe_code)]

pub fn f() -> u8 {
    shim::evil()
}
