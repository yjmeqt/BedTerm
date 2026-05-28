//! LAN-host detection for iOS local-network permission prewarming.
//!
//! Mirrors the previous Swift `LocalNetworkPrewarmer.isLAN(host:)` logic:
//! RFC1918 IPv4, link-local, and unique-local/ULA IPv6 are LAN; loopback
//! and public addresses are not. Hostnames ending in `.local` are
//! treated as LAN (mDNS/Bonjour scope). Pure logic — no UIKit, host-
//! testable via `cargo test`.
//!
//! The FFI export `bt_ios_net_is_lan_host` is the single entry point from
//! Swift; Swift callers feed `host` as a UTF-8 C string.

use std::ffi::CStr;
use std::net::{Ipv4Addr, Ipv6Addr};

/// Returns `true` when `host` is on the local network in the sense iOS
/// gates with the local-network permission prompt: RFC1918 / link-local /
/// ULA / mDNS `.local`. Loopback (`127.x.x.x`, `::1`, `localhost`) and
/// public addresses return `false`.
pub fn is_lan_host(host: &str) -> bool {
    let trimmed = host.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed == "localhost" {
        return false;
    }
    if trimmed.ends_with(".local") {
        return true;
    }

    // Try IPv4.
    if let Ok(v4) = trimmed.parse::<Ipv4Addr>() {
        if v4.is_loopback() {
            return false;
        }
        if v4.is_private() || v4.is_link_local() {
            return true;
        }
        return false;
    }

    // Try IPv6.
    if let Ok(v6) = trimmed.parse::<Ipv6Addr>() {
        if v6.is_loopback() {
            return false;
        }
        if v6.is_unicast_link_local() || v6.is_unique_local() {
            return true;
        }
        return false;
    }

    false
}

/// FFI entry point — `bt_ios_net_is_lan_host(host)` returns `true` when
/// `host` is a LAN address, matching `is_lan_host`.
///
/// # Safety
/// `host` must be a valid nullable UTF-8 C string. NULL yields `false`.
#[no_mangle]
pub extern "C" fn bt_ios_net_is_lan_host(host: *const std::ffi::c_char) -> bool {
    if host.is_null() {
        return false;
    }
    let Ok(s) = (unsafe { CStr::from_ptr(host) }).to_str() else {
        return false;
    };
    is_lan_host(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── RFC1918 private ranges ────────────────────────────────────────────

    #[test]
    fn is_lan_for_10_dot_dot_dot() {
        assert!(is_lan_host("10.0.0.1"));
        assert!(is_lan_host("10.255.255.255"));
        assert!(is_lan_host("10.0.0.0"));
    }

    #[test]
    fn is_lan_for_192_168_dot_dot() {
        assert!(is_lan_host("192.168.0.1"));
        assert!(is_lan_host("192.168.255.255"));
        assert!(is_lan_host("192.168.1.100"));
    }

    #[test]
    fn is_lan_for_172_16_31() {
        assert!(is_lan_host("172.16.0.1"));
        assert!(is_lan_host("172.31.255.255"));
        assert!(is_lan_host("172.20.0.50"));
        assert!(!is_lan_host("172.32.0.1")); // outside /12
        assert!(!is_lan_host("172.15.0.1")); // below /12
    }

    // ── Link-local ────────────────────────────────────────────────────────

    #[test]
    fn is_lan_for_link_local_ipv4() {
        assert!(is_lan_host("169.254.1.1"));
        assert!(is_lan_host("169.254.255.255"));
    }

    #[test]
    fn is_lan_for_link_local_ipv6() {
        assert!(is_lan_host("fe80::1"));
        assert!(is_lan_host("fe80::"));
        assert!(is_lan_host("feb0::1")); // fe80::/10 upper bound
    }

    // ── Unique-local (ULA) ────────────────────────────────────────────────

    #[test]
    fn is_lan_for_unique_local_ipv6() {
        assert!(is_lan_host("fc00::1"));
        assert!(is_lan_host("fd00::dead:beef"));
        assert!(is_lan_host("fdfe::1")); // fc00::/7 upper bound
        assert!(is_lan_host("fcff::")); // fc00::/7 includes fcff::
    }

    // ── Loopback ─────────────────────────────────────────────────────────

    #[test]
    fn loopback_ipv4_is_not_lan() {
        assert!(!is_lan_host("127.0.0.1"));
        assert!(!is_lan_host("127.255.255.255"));
        assert!(!is_lan_host("127.0.0.0"));
    }

    #[test]
    fn loopback_ipv6_is_not_lan() {
        assert!(!is_lan_host("::1"));
    }

    #[test]
    fn localhost_string_is_not_lan() {
        assert!(!is_lan_host("localhost"));
    }

    #[test]
    fn localhost_with_whitespace_is_not_lan() {
        assert!(!is_lan_host("  localhost  "));
    }

    // ── mDNS / Bonjour ────────────────────────────────────────────────────

    #[test]
    fn dot_local_is_lan() {
        assert!(is_lan_host("my-mac.local"));
        assert!(is_lan_host("printer.local"));
    }

    // ── Public addresses ──────────────────────────────────────────────────

    #[test]
    fn public_ipv4_is_not_lan() {
        assert!(!is_lan_host("8.8.8.8"));
        assert!(!is_lan_host("1.1.1.1"));
        assert!(!is_lan_host("203.0.113.1"));
    }

    #[test]
    fn public_ipv6_is_not_lan() {
        assert!(!is_lan_host("2001:4860:4860::8888"));
        assert!(!is_lan_host("2606:4700:4700::1111"));
    }

    // ── Edge cases ────────────────────────────────────────────────────────

    #[test]
    fn empty_string_is_not_lan() {
        assert!(!is_lan_host(""));
        assert!(!is_lan_host("   "));
    }

    #[test]
    fn hostname_without_dot_local_is_not_lan() {
        assert!(!is_lan_host("my-server"));
        assert!(!is_lan_host("example.com"));
    }

    #[test]
    fn whitespace_trimmed_before_check() {
        assert!(is_lan_host("  10.0.0.1  "));
        assert!(!is_lan_host("  8.8.8.8  "));
    }

    // ── FFI ───────────────────────────────────────────────────────────────

    #[test]
    fn ffi_null_is_false() {
        assert!(!bt_ios_net_is_lan_host(std::ptr::null()));
    }

    #[test]
    fn ffi_private_ip() {
        let s = std::ffi::CString::new("10.0.0.5").unwrap();
        assert!(bt_ios_net_is_lan_host(s.as_ptr()));
    }

    #[test]
    fn ffi_public_ip() {
        let s = std::ffi::CString::new("8.8.8.8").unwrap();
        assert!(!bt_ios_net_is_lan_host(s.as_ptr()));
    }

    #[test]
    fn ffi_localhost() {
        let s = std::ffi::CString::new("localhost").unwrap();
        assert!(!bt_ios_net_is_lan_host(s.as_ptr()));
    }

    #[test]
    fn ffi_dot_local() {
        let s = std::ffi::CString::new("my-mac.local").unwrap();
        assert!(bt_ios_net_is_lan_host(s.as_ptr()));
    }
}
