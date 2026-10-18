//! AT-2 (location part): byte offset to line and column. Requirements: REQ-T-2, ASM-056.
//!
//! Lines and columns are 1-based; columns count Unicode scalar values; CRLF is one line break.

#![forbid(unsafe_code)]

use umbra_templates::SourceFile;

fn file(text: &str) -> SourceFile {
    SourceFile::new("a.toml", text)
}

#[test]
fn new_keeps_path_and_text() {
    let f = file("x = 1\n");

    assert_eq!(f.path(), std::path::Path::new("a.toml"));
    assert_eq!(f.text(), "x = 1\n");
}

#[test]
fn given_offset_zero_then_line_1_column_1() {
    assert_eq!(file("ab\ncd\n").line_col(0), (1, 1));
}

#[test]
fn given_offset_in_first_line_then_column_is_offset_plus_one() {
    assert_eq!(file("abcdef\n").line_col(3), (1, 4));
}

#[test]
fn given_offset_after_lf_then_next_line_column_1() {
    assert_eq!(file("ab\ncd\n").line_col(3), (2, 1));
}

#[test]
fn given_offset_after_lf_then_column_counts_from_line_start() {
    assert_eq!(file("ab\ncd\n").line_col(4), (2, 2));
}

#[test]
fn given_offset_after_crlf_then_next_line_column_1() {
    // a b \r \n c d: 'c' is at byte 4.
    assert_eq!(file("ab\r\ncd\r\nef\r\n").line_col(4), (2, 1));
}

#[test]
fn given_crlf_file_then_offset_on_third_line_is_line_3() {
    // "ab\r\n" (0..4) "cd\r\n" (4..8) "ef\r\n": 'e' is at byte 8.
    assert_eq!(file("ab\r\ncd\r\nef\r\n").line_col(8), (3, 1));
}

#[test]
fn given_offset_before_cr_then_column_counts_the_line_content() {
    // The CR of "ab\r\n" is at byte 2, after two characters.
    assert_eq!(file("ab\r\ncd\r\n").line_col(2), (1, 3));
}

#[test]
fn given_multibyte_character_before_offset_then_column_counts_characters_not_bytes() {
    // "é" is two bytes: '=' is at byte 2 but the second character.
    assert_eq!(file("é=1\n").line_col(2), (1, 2));
}

#[test]
fn given_multibyte_characters_on_second_line_then_column_counts_characters() {
    // line 1 is "é=1\n" (bytes 0..5); line 2 is "ñx": 'ñ' at 5..7, 'x' at byte 7.
    assert_eq!(file("é=1\nñx\n").line_col(7), (2, 2));
}

#[test]
fn given_four_byte_character_before_offset_then_it_counts_as_one_column() {
    // U+1F600 is four bytes.
    assert_eq!(file("\u{1F600}x").line_col(4), (1, 2));
}

#[test]
fn given_offset_at_eof_without_trailing_newline_then_column_after_last_character() {
    assert_eq!(file("ab").line_col(2), (1, 3));
}

#[test]
fn given_offset_at_eof_after_trailing_newline_then_first_column_of_next_line() {
    assert_eq!(file("ab\n").line_col(3), (2, 1));
}

#[test]
fn given_empty_file_then_eof_is_line_1_column_1() {
    assert_eq!(file("").line_col(0), (1, 1));
}
