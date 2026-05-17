//! Metal renderer. Exposed via FFI as `bt_renderer_*`.

// Pre-flight: link the heavyweight deps so we catch iOS build failures
// before any real renderer code lands.
#[allow(unused_imports)]
use core_text::font::CTFont;
#[allow(unused_imports)]
use metal::Device as MtlDevice;
