//! Dependency-return tests; real G.723 frames are compared with released FFmpeg.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

static FAIL_OPEN: AtomicBool = AtomicBool::new(false);

#[unsafe(export_name = "rptadv_g723_open")]
extern "C" fn open_return(_: c_int) -> *mut c_void {
    if FAIL_OPEN.load(Ordering::Relaxed) {
        std::ptr::null_mut()
    } else {
        Box::into_raw(Box::new(0_u8)).cast()
    }
}
#[unsafe(export_name = "rptadv_g723_close")]
unsafe extern "C" fn close_state(state: *mut c_void) {
    unsafe { drop(Box::from_raw(state.cast::<u8>())) };
}
#[unsafe(export_name = "rptadv_g723_encode")]
extern "C" fn encode_return(_: *mut c_void, _: *const i16, _: *mut u8) -> c_int {
    -1
}
#[unsafe(export_name = "rptadv_g723_decode")]
extern "C" fn decode_return(_: *mut c_void, _: *const u8, _: c_int, _: *mut i16) -> c_int {
    -1
}

#[test]
fn failed_dependency_results_and_invalid_spans_do_not_publish_audio() {
    let mut encoder = G723::new(true, 6300).unwrap();
    let mut decoder = G723::new(false, 5300).unwrap();
    for (packet, mode) in [
        (&[][..], 5300),
        (&[0][..], 6300),
        (&[1][..], 5300),
        (&[2][..], 5300),
        (&[3][..], 5300),
    ] {
        assert_eq!(decoder.packet_mode(packet), mode);
    }
    assert_eq!(
        encoder.encode(&[0.0; 240], &mut [0; 24]),
        Err(Status::Failed)
    );
    assert_eq!(
        decoder.decode(&[0; 24], &mut [0.0; 240]),
        Err(Status::Malformed)
    );
    assert_eq!(decoder.inspect(&[0]), Err(Status::Malformed));
    // A legal one-byte no-data unit still consumes 240 source frames. The
    // complete packet's frame count must not wrap the public u32 descriptor.
    assert_eq!(
        decoder.inspect(&vec![3; u32::MAX as usize / 240 + 1]),
        Err(Status::Malformed)
    );
    FAIL_OPEN.store(true, Ordering::Relaxed);
    assert_eq!(encoder.reset(), Err(Status::Failed));
    assert_eq!(decoder.reset(), Err(Status::Failed));
    FAIL_OPEN.store(false, Ordering::Relaxed);
}
