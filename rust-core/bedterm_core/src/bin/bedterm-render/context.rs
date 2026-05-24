//! Unified rendering context shared between `bedterm-record` and `bedterm-render`.
//!
//! Ensures the recording PTY and the rendering terminal use identical dimensions,
//! font metrics, and palette — the same bytes produce the same visual output.
//!
//! Unit convention: all spatial values are in **points** unless suffixed `_px`.
//! Device pixels = points × scale (UIScreen.scale / device-pixel ratio).

use std::fs;

// Re-export the shared device preset lookup from the core library.
pub use bedterm_core::device_presets::find_device;

fn device_names() -> Vec<&'static str> {
    bedterm_core::device_presets::device_names()
}

// ── RenderContext ─────────────────────────────────────────────────────────

/// Complete rendering context bundle.
///
/// When passed to both the recorder and the renderer, guarantees identical
/// terminal geometry (cols × rows), font rasterisation (font × dpr), and
/// palette — so the recorded byte stream renders faithfully.
#[derive(Clone, Debug)]
pub struct RenderContext {
    /// Logical viewport in points.
    pub viewport_pt: (u32, u32),
    /// Device-pixel ratio.
    pub scale: f32,
    /// Font size in points (passed to `renderer.set_font` as pixel_size).
    pub font_size_pt: f32,
    /// Explicit terminal columns. Derived from viewport_px / cell_px when `None`.
    pub cols: Option<u16>,
    /// Explicit terminal rows.
    pub rows: Option<u16>,
    /// Palette preset name (e.g. "bedterm-dark").
    pub palette: Option<String>,
    /// Appearance: "light" or "dark".
    pub appearance: Option<String>,
}

impl Default for RenderContext {
    fn default() -> Self {
        Self {
            viewport_pt: (1200, 800),
            scale: 2.0,
            font_size_pt: 14.0,
            cols: None,
            rows: None,
            palette: None,
            appearance: None,
        }
    }
}

impl RenderContext {
    /// Viewport in device pixels (what the offscreen texture gets).
    pub fn viewport_px(&self) -> (u32, u32) {
        (
            (self.viewport_pt.0 as f32 * self.scale).round() as u32,
            (self.viewport_pt.1 as f32 * self.scale).round() as u32,
        )
    }

    /// Apply `--device <name>` preset: sets viewport_pt + scale.
    pub fn apply_device(&mut self, name: &str) -> Result<(), String> {
        let d = find_device(name).ok_or_else(|| {
            format!(
                "unknown device '{}'; known: {}",
                name,
                device_names().join(", ")
            )
        })?;
        self.viewport_pt = d.viewport_pt;
        self.scale = d.scale;
        Ok(())
    }

    // ── JSON sidecar ──────────────────────────────────────────────────

    /// Serialise to the `.meta.json` sidecar format.
    #[allow(dead_code)]
    pub fn to_json(&self) -> String {
        let cols = self.cols.map_or("null".to_string(), |v| v.to_string());
        let rows = self.rows.map_or("null".to_string(), |v| v.to_string());
        let palette = self
            .palette
            .as_ref()
            .map_or("null".to_string(), |v| format!("\"{v}\""));
        let appearance = self
            .appearance
            .as_ref()
            .map_or("null".to_string(), |v| format!("\"{v}\""));
        format!(
            "{{\n  \"viewport_pt\": [{}, {}],\n  \"scale\": {},\n  \"font_size_pt\": {},\n  \"cols\": {},\n  \"rows\": {},\n  \"palette\": {},\n  \"appearance\": {}\n}}\n",
            self.viewport_pt.0,
            self.viewport_pt.1,
            self.scale,
            self.font_size_pt,
            cols,
            rows,
            palette,
            appearance,
        )
    }

    /// Deserialise from a `.meta.json` sidecar file.
    pub fn from_json_file(path: &str) -> Result<Self, String> {
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("cannot read context file '{path}': {e}"))?;
        Self::from_json_str(&raw)
    }

    fn from_json_str(s: &str) -> Result<Self, String> {
        let mut ctx = Self::default();
        ctx.viewport_pt = (
            extract_u32(s, "\"viewport_pt\": [").unwrap_or(ctx.viewport_pt.0),
            extract_u32_after_comma(s, "\"viewport_pt\": [").unwrap_or(ctx.viewport_pt.1),
        );
        ctx.scale = extract_f32(s, "\"scale\": ").unwrap_or(ctx.scale);
        ctx.font_size_pt = extract_f32(s, "\"font_size_pt\": ").unwrap_or(ctx.font_size_pt);
        ctx.cols = extract_optional_u16(s, "\"cols\": ");
        ctx.rows = extract_optional_u16(s, "\"rows\": ");
        ctx.palette = extract_optional_string(s, "\"palette\": ");
        ctx.appearance = extract_optional_string(s, "\"appearance\": ");
        Ok(ctx)
    }
}

// ── Tiny JSON value extractors ────────────────────────────────────────────

fn extract_u32(s: &str, key: &str) -> Option<u32> {
    let rest = s.split(key).nth(1)?;
    rest.split(',').next()?.trim().parse().ok()
}

fn extract_u32_after_comma(s: &str, key: &str) -> Option<u32> {
    let rest = s.split(key).nth(1)?;
    rest.split(',')
        .nth(1)?
        .trim()
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}

fn extract_f32(s: &str, key: &str) -> Option<f32> {
    let rest = s.split(key).nth(1)?;
    rest.split(',').next()?.trim().parse().ok()
}

fn extract_optional_u16(s: &str, key: &str) -> Option<u16> {
    let rest = s.split(key).nth(1)?;
    let val = rest.split(',').next()?.trim();
    if val == "null" {
        None
    } else {
        val.parse().ok()
    }
}

fn extract_optional_string(s: &str, key: &str) -> Option<String> {
    let rest = s.split(key).nth(1)?;
    let val = rest
        .split(|c: char| c == ',' || c == '\n' || c == '}')
        .next()?
        .trim();
    if val == "null" {
        None
    } else {
        Some(val.trim_matches('"').to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_defaults() {
        let ctx = RenderContext::default();
        let json = ctx.to_json();
        let parsed = RenderContext::from_json_str(&json).unwrap();
        assert_eq!(ctx.viewport_pt, parsed.viewport_pt);
        assert!((ctx.scale - parsed.scale).abs() < 0.01);
        assert_eq!(ctx.cols, parsed.cols);
        assert_eq!(ctx.rows, parsed.rows);
    }

    #[test]
    fn round_trip_full() {
        let ctx = RenderContext {
            viewport_pt: (400, 850),
            scale: 3.0,
            font_size_pt: 13.0,
            cols: Some(50),
            rows: Some(48),
            palette: Some("bedterm-dark".into()),
            appearance: Some("dark".into()),
        };
        let json = ctx.to_json();
        let parsed = RenderContext::from_json_str(&json).unwrap();
        assert_eq!(ctx.viewport_pt, parsed.viewport_pt);
        assert!((ctx.scale - parsed.scale).abs() < 0.01);
        assert_eq!(ctx.font_size_pt, parsed.font_size_pt);
        assert_eq!(ctx.cols, parsed.cols);
        assert_eq!(ctx.rows, parsed.rows);
        assert_eq!(ctx.palette, parsed.palette);
        assert_eq!(ctx.appearance, parsed.appearance);
    }

    #[test]
    fn device_preset_iphone17() {
        let mut ctx = RenderContext::default();
        ctx.apply_device("iphone17").unwrap();
        assert_eq!(ctx.viewport_pt, (402, 874));
        assert!((ctx.scale - 3.0).abs() < 0.01);
    }

    #[test]
    fn viewport_px_scaling() {
        let ctx = RenderContext {
            viewport_pt: (402, 874),
            scale: 3.0,
            ..Default::default()
        };
        assert_eq!(ctx.viewport_px(), (1206, 2622));
    }
}
