//! Override dependency return codes only in this unit-test executable.
//! Real Speex state, framing, signal output and PLC are checked by the DSO tests.
use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

static FAULT: AtomicU32 = AtomicU32::new(0);

#[unsafe(export_name = "speex_encoder_ctl")]
unsafe extern "C" fn encoder_control(_: *mut c_void, request: c_int, value: *mut c_void) -> c_int {
    match (FAULT.load(Ordering::Relaxed), request) {
        (1, _) | (2, 39) | (4, 26) => -1,
        (fault, 39) => {
            unsafe {
                value
                    .cast::<c_int>()
                    .write(if fault == 3 { -1 } else { 40 })
            };
            0
        }
        _ => 0,
    }
}
#[unsafe(export_name = "speex_decoder_ctl")]
extern "C" fn decoder_control(_: *mut c_void, _: c_int, _: *mut c_void) -> c_int {
    if FAULT.load(Ordering::Relaxed) == 4 {
        -1
    } else {
        0
    }
}
#[unsafe(export_name = "speex_encode_int")]
extern "C" fn encode_return(_: *mut c_void, _: *mut i16, _: *mut Bits) -> c_int {
    -1
}
#[unsafe(export_name = "speex_decode_int")]
extern "C" fn decode_return(_: *mut c_void, _: *mut Bits, _: *mut i16) -> c_int {
    -1
}

#[test]
fn dependency_failures_and_unsupported_spans_are_not_silently_accepted() {
    for profile_id in [10, 15] {
        let frames = if profile_id == 10 { 160 } else { 320 };
        let mut config = Config {
            struct_size: size_of::<Config>() as u32,
            profile_id,
            mode_id: 0,
            packet_frames: frames as u32,
            max_packet_bytes: 65503,
        };
        config.max_packet_bytes = c_int::MAX as usize + 1;
        assert!(matches!(Speex::open(&config, true), Err(Status::Malformed)));
        config.max_packet_bytes = 65503;
        FAULT.store(1, Ordering::Relaxed);
        assert!(matches!(Speex::open(&config, true), Err(Status::Failed)));
        FAULT.store(0, Ordering::Relaxed);
        let mut encoder = Speex::open(&config, true).unwrap();
        let mut decoder = Speex::open(&config, false).unwrap();
        for fault in [2, 3] {
            FAULT.store(fault, Ordering::Relaxed);
            assert_eq!(encoder.drain_frames(), Err(Status::Failed));
        }
        FAULT.store(0, Ordering::Relaxed);
        assert_eq!(
            encoder.encode(&vec![0.0; frames], &mut [0; 107]),
            Err(Status::Failed)
        );
        assert_eq!(
            decoder.decode(&[0; 1], &mut vec![0.0; frames]),
            Err(Status::Malformed)
        );
        assert_eq!(decoder.conceal(&mut vec![0.0; frames]), Err(Status::Failed));
        for invalid in [1, frames * 7] {
            assert_eq!(
                decoder.conceal(&mut vec![0.0; invalid]),
                Err(Status::Unsupported)
            );
        }
        FAULT.store(4, Ordering::Relaxed);
        assert_eq!(encoder.reset(), Err(Status::Failed));
        assert_eq!(decoder.reset(), Err(Status::Failed));
        FAULT.store(0, Ordering::Relaxed);
    }
}
