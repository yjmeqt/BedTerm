//! iOS device presets — single source of truth for `bedterm-render` and
//! `bedterm-record`. Logical viewport in points (`UIScreen.bounds.size`)
//! and device-pixel ratio (`UIScreen.scale`).
//!
//! Source: <https://www.ios-resolution.com/>

/// Logical viewport in points + device-pixel ratio.
#[derive(Clone, Copy, Debug)]
pub struct DevicePreset {
    pub name: &'static str,
    pub viewport_pt: (u32, u32),
    pub scale: f32,
}

/// All supported device presets.
pub fn all_devices() -> &'static [DevicePreset] {
    &[
        // ── iPhone 17 series (@3x) ──────────────────────────────
        DevicePreset {
            name: "iphone17",
            viewport_pt: (402, 874),
            scale: 3.0,
        },
        DevicePreset {
            name: "iphone17-pro",
            viewport_pt: (402, 874),
            scale: 3.0,
        },
        DevicePreset {
            name: "iphone17-promax",
            viewport_pt: (440, 956),
            scale: 3.0,
        },
        // ── iPhone 16 series (@3x) ──────────────────────────────
        DevicePreset {
            name: "iphone16",
            viewport_pt: (393, 852),
            scale: 3.0,
        },
        DevicePreset {
            name: "iphone16-pro",
            viewport_pt: (402, 874),
            scale: 3.0,
        },
        DevicePreset {
            name: "iphone16-promax",
            viewport_pt: (440, 956),
            scale: 3.0,
        },
        // ── iPhone 15 series (@3x) ──────────────────────────────
        DevicePreset {
            name: "iphone15",
            viewport_pt: (393, 852),
            scale: 3.0,
        },
        DevicePreset {
            name: "iphone15-pro",
            viewport_pt: (393, 852),
            scale: 3.0,
        },
        DevicePreset {
            name: "iphone15-promax",
            viewport_pt: (430, 932),
            scale: 3.0,
        },
        // ── iPhone 14 series (@3x) ──────────────────────────────
        DevicePreset {
            name: "iphone14",
            viewport_pt: (390, 844),
            scale: 3.0,
        },
        DevicePreset {
            name: "iphone14-pro",
            viewport_pt: (393, 852),
            scale: 3.0,
        },
        DevicePreset {
            name: "iphone14-promax",
            viewport_pt: (430, 932),
            scale: 3.0,
        },
        // ── iPhone SE (@2x) ─────────────────────────────────────
        DevicePreset {
            name: "iphone-se3",
            viewport_pt: (375, 667),
            scale: 2.0,
        },
        // ── iPad Pro (@2x) ──────────────────────────────────────
        DevicePreset {
            name: "ipad-pro13",
            viewport_pt: (1032, 1376),
            scale: 2.0,
        },
        DevicePreset {
            name: "ipad-pro11",
            viewport_pt: (834, 1210),
            scale: 2.0,
        },
        // ── iPad Air (@2x) ──────────────────────────────────────
        DevicePreset {
            name: "ipad-air13",
            viewport_pt: (1024, 1366),
            scale: 2.0,
        },
        DevicePreset {
            name: "ipad-air11",
            viewport_pt: (820, 1180),
            scale: 2.0,
        },
        // ── iPad mini (@2x) ─────────────────────────────────────
        DevicePreset {
            name: "ipad-mini",
            viewport_pt: (744, 1133),
            scale: 2.0,
        },
        // ── Mac (CLI default) ───────────────────────────────────
        DevicePreset {
            name: "mac",
            viewport_pt: (1200, 800),
            scale: 2.0,
        },
    ]
}

pub fn find_device(name: &str) -> Option<&'static DevicePreset> {
    all_devices().iter().find(|d| d.name == name)
}

pub fn device_names() -> Vec<&'static str> {
    all_devices().iter().map(|d| d.name).collect()
}
