//! `JobError` and `ErrorCode` contract (`docs/contracts.md` §8).

mod common;

use cauce_core::{ErrorCode, JobError};

#[test]
fn job_error_carries_a_stable_code() {
    let raw = r#"{
      "code": "OUTPUT_VALIDATION_FAILED",
      "message": "La salida no cumple el contrato.",
      "retryable": true
    }"#;

    let error: JobError = serde_json::from_str(raw).expect("error shape must deserialize");
    assert_eq!(error.code, ErrorCode::OutputValidationFailed);
    assert!(error.retryable);
    common::round_trip(&error, raw);
}
