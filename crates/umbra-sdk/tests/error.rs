//! REQ-SDK-1, REQ-P-6: an SDK `Error` carries the `UMBRA_ERR_*` code its export returns, and the
//! message the host receives through the error sink.

#![forbid(unsafe_code)]

use umbra_sdk::Error;
use umbra_sdk::abi::{UMBRA_ERR_INTERNAL, UMBRA_ERR_PARAMS};

#[test]
fn given_a_params_error_then_its_code_is_err_params() {
    let e = Error::params("bad speed");

    assert_eq!(e.code(), UMBRA_ERR_PARAMS);
    assert_eq!(e.to_string(), "bad speed");
}

#[test]
fn given_a_general_error_then_its_code_is_err_internal() {
    let e = Error::new("broke");

    assert_eq!(e.code(), UMBRA_ERR_INTERNAL);
    assert_eq!(e.to_string(), "broke");
}
