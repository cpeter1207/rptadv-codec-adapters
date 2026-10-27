//! Test-only C-ABI substitutes for native failure returns, never codec math.
//! Real iLBC modes, damaged-frame recovery and signal output use the DSO suite.
use super::*;
use std::sync::atomic::{AtomicU32, Ordering};

static FAULT: AtomicU32 = AtomicU32::new(0);
static LIVE: AtomicU32 = AtomicU32::new(0);

unsafe fn create(out: *mut *mut c_void) -> i16 {
    if FAULT.load(Ordering::Relaxed) == 7 {
        return -1;
    }
    LIVE.fetch_add(1, Ordering::Relaxed);
    unsafe { out.write(Box::into_raw(Box::new(0_u8)).cast()) };
    if FAULT.load(Ordering::Relaxed) == 1 {
        -1
    } else {
        0
    }
}
unsafe fn free(state: *mut c_void) -> i16 {
    unsafe { drop(Box::from_raw(state.cast::<u8>())) };
    LIVE.fetch_sub(1, Ordering::Relaxed);
    0
}
fn initialize() -> i16 {
    if FAULT.load(Ordering::Relaxed) == 2 {
        -1
    } else {
        0
    }
}

#[unsafe(export_name = "WebRtcIlbcfix_EncoderCreate")]
unsafe extern "C" fn encoder_create(out: *mut *mut c_void) -> i16 {
    unsafe { create(out) }
}
#[unsafe(export_name = "WebRtcIlbcfix_DecoderCreate")]
unsafe extern "C" fn decoder_create(out: *mut *mut c_void) -> i16 {
    unsafe { create(out) }
}
#[unsafe(export_name = "WebRtcIlbcfix_EncoderFree")]
unsafe extern "C" fn encoder_free(state: *mut c_void) -> i16 {
    unsafe { free(state) }
}
#[unsafe(export_name = "WebRtcIlbcfix_DecoderFree")]
unsafe extern "C" fn decoder_free(state: *mut c_void) -> i16 {
    unsafe { free(state) }
}
#[unsafe(export_name = "WebRtcIlbcfix_EncoderInit")]
extern "C" fn encoder_initialize(_: *mut c_void, _: i16) -> i16 {
    initialize()
}
#[unsafe(export_name = "WebRtcIlbcfix_DecoderInit")]
extern "C" fn decoder_initialize(_: *mut c_void, _: i16) -> i16 {
    initialize()
}
#[unsafe(export_name = "WebRtcIlbcfix_Encode")]
extern "C" fn failed_encode(_: *mut c_void, _: *const i16, _: i16, _: *mut i16) -> i16 {
    -1
}
#[unsafe(export_name = "WebRtcIlbcfix_Decode")]
extern "C" fn failed_decode(
    _: *mut c_void,
    _: *const i16,
    _: i16,
    _: *mut i16,
    _: *mut i16,
) -> i16 {
    -1
}
#[unsafe(export_name = "WebRtcIlbcfix_DecodePlc")]
extern "C" fn failed_conceal(_: *mut c_void, _: *mut i16, _: i16) -> i16 {
    -1
}

#[test]
fn failed_native_contexts_and_results_preserve_lifetime_and_status() {
    for mode_id in [20, 30] {
        let frames = if mode_id == 20 { 160 } else { 240 };
        let bytes = if mode_id == 20 { 38 } else { 50 };
        let config = Config {
            struct_size: size_of::<Config>() as u32,
            profile_id: 11,
            mode_id,
            packet_frames: frames as u32,
            max_packet_bytes: 65503,
        };
        for fault in [1, 2, 7] {
            FAULT.store(fault, Ordering::Relaxed);
            for encoder in [true, false] {
                assert!(matches!(Ilbc::open(&config, encoder), Err(Status::Failed)));
                assert_eq!(LIVE.load(Ordering::Relaxed), 0);
            }
        }
        FAULT.store(0, Ordering::Relaxed);
        {
            let mut encoder = Ilbc::open(&config, true).unwrap();
            let mut decoder = Ilbc::open(&config, false).unwrap();
            assert_eq!(
                encoder.encode(&vec![0.0; frames], &mut vec![0; bytes]),
                Err(Status::Failed)
            );
            assert_eq!(
                decoder.decode(&vec![0; bytes], &mut vec![0.0; frames]),
                Err(Status::Malformed)
            );
            assert_eq!(decoder.conceal(&mut vec![0.0; frames]), Err(Status::Failed));
            for invalid in [1, frames * 7] {
                assert_eq!(
                    decoder.conceal(&mut vec![0.0; invalid]),
                    Err(Status::Unsupported)
                );
            }
            FAULT.store(2, Ordering::Relaxed);
            assert_eq!(encoder.reset(), Err(Status::Failed));
            assert_eq!(decoder.reset(), Err(Status::Failed));
        }
        assert_eq!(LIVE.load(Ordering::Relaxed), 0);
    }
}
