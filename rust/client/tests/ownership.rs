//! Verify the moved owner still retains real dynamically loaded codec contexts.
use rptadv_codec_client::{CodecCatalog, ProfileId, ProviderSelection, Status};
use std::path::PathBuf;

#[test]
fn codec_client_move_preserves_profiles_and_ownership() {
    let directory = std::env::var_os("RPTADV_CODEC_DIR").unwrap_or_default();
    let catalog = CodecCatalog::load(&[ProviderSelection {
        path: PathBuf::from(directory).join("librptadv_codec_g711.so.1"),
        profiles: vec![ProfileId::G711Mu, ProfileId::G711A],
    }])
    .unwrap();
    assert_eq!(catalog.profiles().count(), 2);
    assert_eq!(
        catalog.profile(ProfileId::G711Mu).unwrap().sample_rate_hz,
        8000
    );
    let mut encoder = catalog.encoder(ProfileId::G711Mu, 0, 160, 65503).unwrap();
    let mut decoder = catalog.decoder(ProfileId::G711Mu, 0, 160, 65503).unwrap();
    // Contexts retain the DSO even after composition/catalog destruction.
    drop(catalog);
    let mut encoded = [0xa5; 160];
    let first = encoder.encode(&[0.0; 37], &mut encoded, false).unwrap();
    assert_eq!(
        (first.status, first.consumed_frames, first.encoded_bytes),
        (Status::NeedInput, 37, 0)
    );
    assert_eq!(
        encoder
            .encode(&[0.0; 123], &mut encoded[..159], false)
            .unwrap_err()
            .required_capacity,
        160
    );
    let packet = encoder.encode(&[0.0; 123], &mut encoded, false).unwrap();
    assert_eq!(
        (
            packet.encoded_bytes,
            packet.source_start_frames,
            packet.source_frames,
            packet.packet_frames
        ),
        (160, 0, 160, 160)
    );
    assert_eq!(encoded, [0xff; 160]);
    assert_eq!(decoder.inspect(&encoded).unwrap().frames, 160);
    let mut pcm = [0.0; 160];
    assert_eq!(
        decoder
            .decode(&encoded, &mut pcm[..159])
            .unwrap_err()
            .required_capacity,
        160
    );
    assert_eq!(decoder.decode(&encoded, &mut pcm).unwrap(), 160);
    assert_eq!(pcm, [0.0; 160]);
    encoder.reset().unwrap();
    decoder.reset().unwrap();
    assert_eq!(
        encoder
            .encode(&[], &mut encoded, true)
            .unwrap()
            .encoded_bytes,
        0
    );
}
