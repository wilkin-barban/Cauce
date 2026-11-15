//! `Timestamp` contract: Cauce persists and transports UTC RFC 3339 text only.

use cauce_core::Timestamp;

#[test]
fn timestamps_are_validated_as_utc_rfc3339() {
    assert!(Timestamp::parse("2026-11-14T10:00:00Z").is_ok());
    assert!(Timestamp::parse("2026-11-14T10:00:00.250Z").is_ok());
    assert!(Timestamp::parse("2026-11-14T10:00:00").is_err());
    assert!(Timestamp::parse("14/11/2026 10:00").is_err());
}
