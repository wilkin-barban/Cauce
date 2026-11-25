//! Acoustic engine (phase 1, T3 / `docs/architecture-phases-0-and-1.md` §3.2).
//!
//! The planner decides *what* to mix; this module decides *how*. Every string
//! here is part of the media a user hears, so each one is pinned character by
//! character instead of asserted with `contains`.

use cauce_core::command::{
    DIALOGUE_BOOST_FILTER, HostFilters, dual_track_plans, native_tonemap_filter_chain,
    tonemap_chain, tonemap_filter_chain,
};
use cauce_core::{AudioFilter, StreamAction, ToneMappingOption};

/// §3.2, verbatim, including the limiter phase 1 adds.
const DIFFERENCE_BOOST_WITH_LIMITER: &str =
    "pan=stereo|FL=0.5*FL+0.85*FC+0.5*SL|FR=0.5*FR+0.85*FC+0.5*SR,volume=1.8,alimiter=limit=0.98";

#[test]
fn the_dialogue_boost_chain_ends_with_the_limiter() {
    assert_eq!(
        DIALOGUE_BOOST_FILTER, DIFFERENCE_BOOST_WITH_LIMITER,
        "phase 1 adds alimiter=limit=0.98 to the phase 0 matrix"
    );
}

// ---------------------------------------------------------------------------
// Dual track (S11)
// ---------------------------------------------------------------------------

#[test]
fn dual_track_emits_a_stereo_default_plus_a_surround_companion() {
    let tracks = dual_track_plans(2, Some("spa"));

    assert_eq!(tracks.len(), 2, "the universal mode is exactly two tracks");

    let stereo = &tracks[0];
    assert_eq!(stereo.source_index, 2);
    assert_eq!(stereo.output_index, 0);
    assert_eq!(stereo.action, StreamAction::Transcode);
    assert_eq!(stereo.encoder.as_deref(), Some("aac"));
    assert_eq!(stereo.bitrate.as_deref(), Some("192k"));
    assert_eq!(stereo.channels, 2);
    assert_eq!(stereo.filter, Some(AudioFilter::StereoDialogueBoost));
    assert_eq!(stereo.language.as_deref(), Some("spa"));
    assert!(stereo.is_default, "the stereo track is the one that plays");

    let surround = &tracks[1];
    assert_eq!(surround.source_index, 2, "both tracks come from one source");
    assert_eq!(surround.output_index, 1);
    assert_eq!(surround.action, StreamAction::Transcode);
    assert_eq!(surround.encoder.as_deref(), Some("ac3"));
    assert_eq!(surround.bitrate.as_deref(), Some("448k"));
    assert_eq!(surround.channels, 6);
    assert_eq!(
        surround.filter, None,
        "the surround companion is a straight re-encode, not a downmix"
    );
    assert_eq!(surround.language.as_deref(), Some("spa"));
    assert!(!surround.is_default, "only one track can be the default");
}

#[test]
fn dual_track_carries_no_title_so_the_planner_can_localize_it() {
    let tracks = dual_track_plans(3, None);

    assert!(
        tracks.iter().all(|track| track.title.is_none()),
        "titles are presentation and belong to the planner, not to the filter builder"
    );
    assert!(tracks.iter().all(|track| track.language.is_none()));
}

// ---------------------------------------------------------------------------
// Tone mapping (S7, S25)
// ---------------------------------------------------------------------------

#[test]
fn the_hable_chain_on_a_zscale_host_is_still_the_phase_0_string() {
    assert_eq!(
        tonemap_chain(ToneMappingOption::Hable, &HostFilters::ALL),
        tonemap_filter_chain(),
        "phase 1 must not change the chain phase 0 shipped for hable"
    );
}

#[test]
fn zscale_hosts_honor_every_algorithm() {
    for algorithm in [
        ToneMappingOption::Hable,
        ToneMappingOption::Mobius,
        ToneMappingOption::Reinhard,
    ] {
        let chain = tonemap_chain(algorithm, &HostFilters::ALL);

        assert!(
            chain.starts_with("zscale=t=linear:npl=100"),
            "the zscale path starts with the linearisation: {chain}"
        );
        assert!(
            chain.contains(&format!(
                "tonemap=tonemap={}:desat=0",
                algorithm.ffmpeg_name()
            )),
            "the requested curve must reach the filter: {chain}"
        );
        assert!(
            chain.ends_with("zscale=t=bt709:m=bt709:r=tv"),
            "the chain returns to bt709: {chain}"
        );
    }
}

#[test]
fn hosts_without_zscale_fall_back_to_the_native_curve() {
    let host = HostFilters {
        zscale: false,
        loudnorm: true,
    };

    assert_eq!(
        tonemap_chain(ToneMappingOption::Mobius, &host),
        native_tonemap_filter_chain(),
        "mobius keeps the fallback phase 0 stabilized"
    );
    assert_eq!(
        tonemap_chain(ToneMappingOption::Hable, &host),
        "tonemap=hable,format=yuv420p"
    );
    assert_eq!(
        tonemap_chain(ToneMappingOption::Reinhard, &host),
        "tonemap=reinhard,format=yuv420p"
    );

    for algorithm in [
        ToneMappingOption::Hable,
        ToneMappingOption::Mobius,
        ToneMappingOption::Reinhard,
    ] {
        let chain = tonemap_chain(algorithm, &host);
        assert!(
            !chain.contains("zscale"),
            "a host without zscale must never be asked for it: {chain}"
        );
    }
}
