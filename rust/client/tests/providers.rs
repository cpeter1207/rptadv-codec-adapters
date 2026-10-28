//! Exercise protocol-neutral providers through their real versioned shared objects.
use rptadv_codec_client::{CodecCatalog, ProfileId, ProviderSelection};
use std::path::PathBuf;

#[test]
fn contract_missing_provider_is_not_a_silent_ulaw_fallback() {
    assert!(
        CodecCatalog::load(&[ProviderSelection {
            path: PathBuf::from("/missing/rptadv-codec-provider.so"),
            profiles: vec![ProfileId::G711Mu],
        }])
        .is_err()
    );
}

#[test]
fn contract_all_selected_profiles_open_independent_directions() {
    let directory = PathBuf::from(std::env::var_os("RPTADV_CODEC_DIR").unwrap_or_default());
    let selections = [
        ("g711", vec![ProfileId::G711Mu, ProfileId::G711A]),
        ("pcm", vec![ProfileId::Pcm8, ProfileId::Pcm16]),
        ("gsm", vec![ProfileId::GsmFr]),
        ("speex", vec![ProfileId::Speex, ProfileId::Speex16]),
        ("opus", vec![ProfileId::Opus]),
        (
            "spandsp",
            vec![
                ProfileId::G726Aal2,
                ProfileId::G726,
                ProfileId::G722,
                ProfileId::Lpc10,
            ],
        ),
        ("ffmpeg", vec![ProfileId::G723]),
        ("bcg729", vec![ProfileId::G729]),
        ("ilbc", vec![ProfileId::Ilbc]),
        ("dialogic", vec![ProfileId::Dialogic]),
    ]
    .into_iter()
    .map(|(backend, profiles)| ProviderSelection {
        path: directory.join(format!("librptadv_codec_{backend}.so.1")),
        profiles,
    })
    .collect::<Vec<_>>();
    let catalog = CodecCatalog::load(&selections).unwrap();
    assert_eq!(catalog.profiles().count(), 16);
    for profile in catalog.profiles() {
        let mode = &profile.modes[0];
        let packet =
            (profile.sample_rate_hz / 50).div_ceil(mode.frame_samples) * mode.frame_samples;
        let _encoder = catalog.encoder(profile.id, mode.id, packet, 65503).unwrap();
        let _decoder = catalog.decoder(profile.id, mode.id, packet, 65503).unwrap();
    }
    assert_eq!(catalog.profile(ProfileId::Ilbc).unwrap().modes[0].id, 30);
}
