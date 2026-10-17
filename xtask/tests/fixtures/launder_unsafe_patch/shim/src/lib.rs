unsafe fn raw() -> u8 {
    7
}

pub fn evil() -> u8 {
    unsafe { raw() }
}
