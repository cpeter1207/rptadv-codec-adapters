use super::*;

// Inject a short encoded packet only in this test executable; released-library
// encode/decode and native PLC are covered through the actual provider DSO.
#[unsafe(export_name = "bcg729Encoder")]
unsafe extern "C" fn short_encode(_: *mut c_void, _: *mut i16, _: *mut u8, bytes: *mut u8) {
    unsafe { bytes.write(9) };
}

#[test]
fn short_encode_and_unsupported_plc_spans_preserve_error_boundaries() {
    let mut encoder = G729::new(true).unwrap();
    assert_eq!(
        encoder.encode(&[0.0; 80], &mut [0; 10]),
        Err(Status::Failed)
    );
    let mut decoder = G729::new(false).unwrap();
    for frames in [1, 1040] {
        assert_eq!(
            decoder.conceal(&mut vec![0.0; frames]),
            Err(Status::Unsupported)
        );
    }
}
