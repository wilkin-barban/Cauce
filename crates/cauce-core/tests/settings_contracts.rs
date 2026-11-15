//! `Settings` contract (`docs/contracts.md` §2.6).

mod common;

use cauce_core::{DuplicatePolicy, Language, Settings, Theme};

const SETTINGS_JSON: &str = r#"{
  "output_dir": "/home/wilkin/videos/converted",
  "max_concurrent_jobs": 1,
  "theme": "system",
  "language": "pt-BR",
  "notifications": false,
  "duplicate_policy": "suffix",
  "keep_temp_on_failure": false
}"#;

#[test]
fn settings_round_trips_and_uses_the_brazilian_locale_tag() {
    let settings = Settings {
        output_dir: String::from("/home/wilkin/videos/converted"),
        max_concurrent_jobs: 1,
        theme: Theme::System,
        language: Language::PtBr,
        notifications: false,
        duplicate_policy: DuplicatePolicy::Suffix,
        keep_temp_on_failure: false,
    };

    common::round_trip(&settings, SETTINGS_JSON);
    assert!(
        serde_json::to_string(&settings)
            .expect("settings must serialize")
            .contains("\"pt-BR\""),
        "the locale tag must stay `pt-BR`, not `pt-br`"
    );
}

#[test]
fn settings_reject_out_of_range_concurrency() {
    let raw = SETTINGS_JSON.replace("\"max_concurrent_jobs\": 1", "\"max_concurrent_jobs\": 9");
    let error =
        serde_json::from_str::<Settings>(&raw).expect_err("concurrency above 8 must be rejected");
    assert!(
        error.to_string().contains("max_concurrent_jobs"),
        "the error must name the offending field, got: {error}"
    );
}
