use std::ffi::CString;

use bedterm_core::persistence::ffi::*;
use bedterm_core::term::Terminal;

#[test]
fn record_kill_then_list_returns_one() {
    let path = CString::new(":memory:").unwrap();
    let h = unsafe { bedterm_persistence_init(path.as_ptr()) };
    assert!(!h.is_null());

    let mut term = Terminal::new(80, 24);
    let snap_id = CString::new("s1").unwrap();
    let host_id = CString::new("h1").unwrap();
    unsafe {
        bedterm_persistence_attach(
            h,
            &mut term as *mut Terminal,
            snap_id.as_ptr(),
            host_id.as_ptr(),
        )
    };

    // Record kill — no cwd, no command, no exit code.
    unsafe {
        bedterm_persistence_record_kill(
            h,
            snap_id.as_ptr(),
            0,
            std::ptr::null(),
            std::ptr::null(),
            0,
            0,
        )
    };

    // List should now return one snapshot.
    let list = unsafe { bedterm_persistence_list(h, host_id.as_ptr()) };
    assert!(!list.is_null());
    let count = unsafe { (*list).count };
    assert_eq!(count, 1);
    unsafe { bedterm_persistence_free_list(list) };

    // Discard it; list should return zero.
    unsafe { bedterm_persistence_discard(h, snap_id.as_ptr()) };
    let list2 = unsafe { bedterm_persistence_list(h, host_id.as_ptr()) };
    assert!(!list2.is_null());
    assert_eq!(unsafe { (*list2).count }, 0);
    unsafe { bedterm_persistence_free_list(list2) };

    unsafe { bedterm_persistence_close(h) };
}
