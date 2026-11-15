//! Shared helpers for the domain contract tests.

#![allow(dead_code)]

/// Parses a golden JSON document that pins the wire format of a contract type.
pub fn golden(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).expect("golden fixture must be valid JSON")
}

/// Asserts that `value` serializes exactly like the golden document and that the
/// document deserializes back into an equal, re-serializable value.
pub fn round_trip<T>(value: &T, raw: &str)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let expected = golden(raw);
    let actual = serde_json::to_value(value).expect("value must serialize");
    assert_eq!(actual, expected, "serialized form must match the contract");

    let decoded: T = serde_json::from_value(expected).expect("contract JSON must deserialize");
    let reencoded = serde_json::to_value(&decoded).expect("decoded value must serialize again");
    assert_eq!(reencoded, actual, "round trip must be lossless");
}
