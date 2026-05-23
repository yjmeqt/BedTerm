use std::ffi::CString;

#[test]
fn ffi_init_and_close_inmem_marker() {
    let path = CString::new(":memory:").unwrap();
    let h = unsafe { bedterm_core::persistence::ffi::bedterm_persistence_init(path.as_ptr()) };
    assert!(!h.is_null(), "init returned null");
    unsafe { bedterm_core::persistence::ffi::bedterm_persistence_close(h) };
}

#[test]
fn ffi_init_null_path_returns_null() {
    let h = unsafe { bedterm_core::persistence::ffi::bedterm_persistence_init(std::ptr::null()) };
    assert!(h.is_null());
}
