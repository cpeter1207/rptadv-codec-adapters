use super::*;

// Only the unit-test executable substitutes this dependency return. The real
// dynamically loaded provider is checked against libgsm in Python conformance.
#[unsafe(export_name = "gsm_decode")]
extern "C" fn failed_decode(_: *mut c_void, _: *mut u8, _: *mut i16) -> c_int {
    -1
}

#[test]
fn malformed_magic_and_native_decode_failure_are_recoverable_media_errors() {
    let mut codec = Gsm::new().unwrap();
    assert_eq!(codec.inspect(&[0; 33]), Err(Status::Malformed));
    let packet = [0xd0; 33];
    assert_eq!(codec.inspect(&packet), Ok(160));
    assert_eq!(
        codec.decode(&packet, &mut [0.0; 160]),
        Err(Status::Malformed)
    );
}
