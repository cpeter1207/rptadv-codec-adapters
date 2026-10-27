use super::*;

// Return a documented short decode without replacing state allocation/free.
// Real library signal-output equality is tested in the dynamic-provider suite.
#[unsafe(export_name = "g726_decode")]
extern "C" fn short_decode(_: *mut c_void, _: *mut i16, _: *const u8, _: c_int) -> c_int {
    0
}

#[test]
fn short_native_output_is_not_published_as_a_complete_frame() {
    let mut codec = Span::new(5, false).unwrap();
    assert_eq!(codec.decode(&[0; 1], &mut [0.0; 2]), Err(Status::Failed));
}
