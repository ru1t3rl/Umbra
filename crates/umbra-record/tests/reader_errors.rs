//! M7 WI-11: the reader's errors are located and the iterator stops after the first (REQ-K-7,
//! NFR-2, ADR 0053).

#![forbid(unsafe_code)]
#![expect(clippy::expect_used, reason = "tests construct known-valid values")]

mod common;

use std::io::Cursor;

use common::{every_record, header, read_bytes, write_bytes};
use umbra_record::{MAX_LINE_BYTES, Reader, Record, RecordError};

fn lines_of(bytes: &[u8]) -> Vec<String> {
    String::from_utf8(bytes.to_vec())
        .expect("utf-8")
        .lines()
        .map(str::to_owned)
        .collect()
}

fn joined(lines: &[&str]) -> Vec<u8> {
    let mut out = lines.join("\n").into_bytes();
    out.push(b'\n');
    out
}

fn header_line() -> String {
    lines_of(&write_bytes(&header(), &[])).remove(0)
}

/// The error of reading `bytes` to the end.
fn error_of(bytes: Vec<u8>) -> RecordError {
    read_bytes(bytes).expect_err("must fail")
}

#[test]
fn given_a_newer_version_then_the_error_names_the_version_and_line_1() {
    let newer = r#"{"type":"header","format":"umbra.recording","version":2,"encoding":"cbor","anything":[1]}"#;

    let err = Reader::from_reader(Cursor::new(joined(&[newer]))).expect_err("must fail");

    assert!(matches!(err, RecordError::Version { found: 2 }), "{err:?}");
    assert!(err.to_string().contains("line 1"), "{err}");
}

#[test]
fn given_a_version_beyond_u32_then_it_is_still_a_newer_version() {
    let newer = r#"{"type":"header","format":"umbra.recording","version":99999999999}"#;

    let err = Reader::from_reader(Cursor::new(joined(&[newer]))).expect_err("must fail");

    assert!(
        matches!(
            err,
            RecordError::Version {
                found: 99_999_999_999
            }
        ),
        "{err:?}"
    );
}

#[test]
fn given_an_unknown_record_type_then_the_error_names_the_line_and_the_type() {
    let h = header_line();
    let bytes = joined(&[
        &h,
        r#"{"type":"hash","tick":1,"hash":"0000000000000007"}"#,
        r#"{"type":"mystery","tick":1}"#,
    ]);

    let err = error_of(bytes);

    let RecordError::Syntax { line, message } = &err else {
        panic!("expected a syntax error, got {err:?}");
    };
    assert_eq!(*line, 3);
    assert!(
        message.contains("unknown record type \"mystery\""),
        "{message}"
    );
}

#[test]
fn given_an_unknown_command_or_message_kind_then_it_is_an_error() {
    let h = header_line();
    let command = r#"{"type":"command","before_tick":0,"seq":0,"command":{"kind":"fly"}}"#;
    let message = r#"{"type":"inbound","tick":1,"adapter":"a","messages":[{"kind":"smoke"}]}"#;

    for (line, text) in [(2, command), (2, message)] {
        let err = error_of(joined(&[&h, text]));
        assert!(
            matches!(&err, RecordError::Syntax { line: l, .. } if *l == line),
            "{err:?}"
        );
    }
    let err = error_of(joined(&[&h, message]));
    assert!(err.to_string().contains("`messages`[0]"), "{err}");
}

#[test]
fn given_a_file_cut_inside_a_line_then_the_error_names_that_line_and_truncation() {
    let bytes = write_bytes(&header(), &every_record());
    let cut = bytes.len() - 7;

    let err = error_of(bytes[..cut].to_vec());

    let lines = lines_of(&bytes).len() as u64;
    assert!(
        matches!(&err, RecordError::Syntax { line, message } if *line == lines && message.contains("truncated")),
        "{err:?}"
    );
    assert!(err.to_string().contains(&format!("line {lines}")), "{err}");
}

#[test]
fn given_a_header_cut_in_half_then_the_error_is_a_truncation_at_line_1() {
    let bytes = write_bytes(&header(), &[]);

    let err = error_of(bytes[..20].to_vec());

    assert!(
        matches!(&err, RecordError::Syntax { line: 1, message } if message.contains("truncated")),
        "{err:?}"
    );
}

#[test]
fn given_an_empty_file_then_the_error_is_a_syntax_error_at_line_1() {
    assert!(matches!(
        error_of(Vec::new()),
        RecordError::Syntax { line: 1, .. }
    ));
}

#[test]
fn given_a_file_cut_between_lines_then_the_records_before_the_cut_read() {
    let records = every_record();
    let bytes = write_bytes(&header(), &records);
    let first_two = lines_of(&bytes)[..3].join("\n") + "\n";

    let (_, got) = read_bytes(first_two.into_bytes()).expect("read");

    assert_eq!(got, records[..2]);
}

#[test]
fn given_a_bad_line_then_the_iterator_ends_after_the_error() {
    let h = header_line();
    let bytes = joined(&[
        &h,
        "not json",
        r#"{"type":"hash","tick":1,"hash":"0000000000000007"}"#,
    ]);
    let (_, mut reader) = Reader::from_reader(Cursor::new(bytes)).expect("header");

    let first = reader.next();
    let second = reader.next();

    assert!(
        matches!(first, Some(Err(RecordError::Syntax { line: 2, .. }))),
        "{first:?}"
    );
    assert!(second.is_none());
}

#[test]
fn given_header_problems_then_each_is_a_syntax_error_on_line_1() {
    let cases = [
        r#"{"type":"command"}"#,
        r#"{"type":"header","format":"other","version":1}"#,
        r#"{"type":"header","format":"umbra.recording","version":0}"#,
        r#"{"type":"header","format":"umbra.recording","version":1,"encoding":"cbor"}"#,
        r#"{"type":"header","format":"umbra.recording","version":1,"encoding":"json"}"#,
        "[]",
        "",
    ];
    for text in cases {
        let err = Reader::from_reader(Cursor::new(joined(&[text]))).expect_err(text);
        assert!(
            matches!(err, RecordError::Syntax { line: 1, .. }),
            "{text}: {err:?}"
        );
    }
}

#[test]
fn given_an_otherwise_valid_header_with_version_0_then_it_is_refused() {
    let zero = header_line().replace(r#""version":1"#, r#""version":0"#);

    let err = Reader::from_reader(Cursor::new(joined(&[&zero]))).expect_err("must fail");

    assert!(
        matches!(&err, RecordError::Syntax { line: 1, message } if message.contains("version")),
        "{err:?}"
    );
}

#[test]
fn given_blank_invalid_utf8_and_bad_values_then_each_is_a_syntax_error_at_its_line() {
    let h = header_line();
    let hash = r#"{"type":"hash","tick":1,"hash":"0000000000000007"}"#;
    let bad_hash = r#"{"type":"hash","tick":1,"hash":"7"}"#;
    let zero_id =
        r#"{"type":"command","before_tick":0,"seq":0,"command":{"kind":"remove","entity":0}}"#;
    let negative = r#"{"type":"hash","tick":-1,"hash":"0000000000000007"}"#;
    for text in ["", bad_hash, zero_id, negative, "{}", "null"] {
        let err = error_of(joined(&[&h, hash, text]));
        assert!(
            matches!(err, RecordError::Syntax { line: 3, .. }),
            "{text:?}: {err:?}"
        );
    }
    let mut bytes = joined(&[&h, hash]);
    bytes.extend_from_slice(b"\xff\xfe\n");
    assert!(matches!(
        error_of(bytes),
        RecordError::Syntax { line: 3, .. }
    ));
}

#[test]
fn given_extra_keys_then_they_are_ignored() {
    let h = header_line().replace(r#""type":"header""#, r#""type":"header","future":{"x":1}"#);
    let bytes = joined(&[
        &h,
        r#"{"type":"hash","tick":1,"extra":true,"hash":"0000000000000007"}"#,
    ]);

    let (got_header, got) = read_bytes(bytes).expect("read");

    assert_eq!(got_header, header());
    assert_eq!(got.len(), 1);
}

#[test]
fn given_a_line_longer_than_the_limit_then_it_is_a_syntax_error() {
    let mut bytes = write_bytes(&header(), &[]);
    let too_long = usize::try_from(MAX_LINE_BYTES).expect("limit") + 10;
    bytes.extend(std::iter::repeat_n(b' ', too_long));
    bytes.push(b'\n');

    let err = error_of(bytes);

    assert!(
        matches!(&err, RecordError::Syntax { line: 2, message } if message.contains("longer")),
        "{err:?}"
    );
}

#[test]
fn given_a_reader_on_a_missing_file_then_open_is_an_io_error() {
    let missing = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("no-such.umbrarec.jsonl");

    let err = Reader::open(&missing).expect_err("must fail");

    assert!(matches!(err, RecordError::Io(_)), "{err:?}");
}

#[test]
fn given_a_recording_without_an_end_record_then_it_reads_as_far_as_it_goes() {
    let records: Vec<Record> = every_record()
        .into_iter()
        .filter(|r| !matches!(r, Record::End(_)))
        .collect();

    let (_, got) = read_bytes(write_bytes(&header(), &records)).expect("read");

    assert_eq!(got, records);
}

const SEED: &str = r#""seed":"18446744073709551615""#;

#[test]
fn given_decimal_strings_that_are_not_canonical_then_they_are_refused() {
    for bad in ["+5", "007", "", " 5", "5 ", "-0", "0x5", "٣"] {
        let text = header_line().replace(SEED, &format!(r#""seed":"{bad}""#));

        let err = Reader::from_reader(Cursor::new(joined(&[&text]))).expect_err(bad);

        assert!(
            matches!(err, RecordError::Syntax { line: 1, .. }),
            "{bad:?}: {err:?}"
        );
    }
    let zero = header_line().replace(SEED, r#""seed":"0""#);
    let (h, _) = Reader::from_reader(Cursor::new(joined(&[&zero]))).expect("0 is canonical");
    assert_eq!(h.seed, 0);
}

#[test]
fn given_a_key_that_is_not_canonical_decimal_then_the_message_line_is_refused() {
    let line = r#"{"type":"inbound","tick":1,"adapter":"a","messages":[{"kind":"entity_removed","key":"+5"}]}"#;

    let err = error_of(joined(&[&header_line(), line]));

    assert!(
        matches!(err, RecordError::Syntax { line: 2, .. }),
        "{err:?}"
    );
}

#[test]
fn given_uppercase_or_short_hex_then_the_hash_is_refused() {
    let h = header_line();
    for bad in [
        "00000000000000AB",
        "ab",
        "000000000000000g",
        "+000000000000001",
    ] {
        let line = format!(r#"{{"type":"hash","tick":1,"hash":"{bad}"}}"#);

        let err = error_of(joined(&[&h, &line]));

        assert!(
            matches!(err, RecordError::Syntax { line: 2, .. }),
            "{bad}: {err:?}"
        );
    }
}

#[test]
fn given_a_position_with_both_frames_then_it_is_refused() {
    let both = r#"{"type":"command","before_tick":0,"seq":0,"command":{"kind":"spawn","template":"tank.mbt","position":{"ecef":[1,2,3],"geodetic":[0,0,0]},"heading":0,"speed":0,"name":null,"group":null,"force":null,"result":1}}"#;

    let err = error_of(joined(&[&header_line(), both]));

    assert!(
        matches!(&err, RecordError::Syntax { line: 2, message } if message.contains("exactly one of ecef or geodetic")),
        "{err:?}"
    );
}

#[test]
fn given_a_second_header_line_then_the_error_says_a_header_may_only_be_line_1() {
    let h = header_line();

    let err = error_of(joined(&[&h, &h]));

    assert!(
        matches!(&err, RecordError::Syntax { line: 2, message } if message.contains("a header may only be line 1")),
        "{err:?}"
    );
}
