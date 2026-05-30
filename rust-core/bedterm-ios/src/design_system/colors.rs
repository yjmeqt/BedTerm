//! iOS-specific dynamic UIColors that wrap the pure `Rgba` / `TOKEN_TABLE`
//! data from `bedterm_app::design_system::colors`.
//!
//! Re-exports the pure types so callers can still do
//! `use crate::design_system::colors::Rgba`.

pub use bedterm_app::design_system::colors::{rgba_pair, Rgba, TOKEN_TABLE};

// ---- iOS-only: dynamic-color construction ----------------------------------

#[cfg(target_os = "ios")]
pub use ios::*;

#[cfg(target_os = "ios")]
mod ios {
    use bedterm_app::design_system::colors::Rgba;
    use block2::{RcBlock, StackBlock};
    use objc2::rc::Retained;
    use objc2_ui_kit::{UIColor, UITraitCollection, UIUserInterfaceStyle};
    use std::cell::OnceCell;
    use std::ptr::NonNull;

    fn solid(rgba: Rgba) -> Retained<UIColor> {
        UIColor::colorWithRed_green_blue_alpha(rgba.r, rgba.g, rgba.b, rgba.a)
    }

    fn make_dynamic(light: Rgba, dark: Rgba) -> Retained<UIColor> {
        let light_color = solid(light);
        let dark_color = solid(dark);
        let light_keep = light_color.clone();
        let dark_keep = dark_color.clone();
        let block = StackBlock::new(
            move |traits: NonNull<UITraitCollection>| -> NonNull<UIColor> {
                let style: UIUserInterfaceStyle = unsafe { traits.as_ref().userInterfaceStyle() };
                let chosen: &UIColor = if style == UIUserInterfaceStyle::Dark {
                    &dark_keep
                } else {
                    &light_keep
                };
                NonNull::from(chosen)
            },
        );
        let block: RcBlock<dyn Fn(NonNull<UITraitCollection>) -> NonNull<UIColor>> = block.copy();
        unsafe { UIColor::colorWithDynamicProvider(&block) }
    }

    macro_rules! token_fn {
        ($fn_name:ident, $name:literal) => {
            #[allow(dead_code)]
            pub fn $fn_name() -> Retained<UIColor> {
                thread_local! {
                    static CACHE: OnceCell<Retained<UIColor>> = const { OnceCell::new() };
                }
                CACHE.with(|cell| {
                    cell.get_or_init(|| {
                        let (light, dark) =
                            super::rgba_pair($name).expect("token must exist in TOKEN_TABLE");
                        make_dynamic(light, dark)
                    })
                    .clone()
                })
            }
        };
    }

    token_fn!(shadcn_primary, "ShadcnPrimary");
    token_fn!(shadcn_primary_foreground, "ShadcnPrimaryForeground");
    token_fn!(shadcn_background, "ShadcnBackground");
    token_fn!(shadcn_border, "ShadcnBorder");
    token_fn!(shadcn_input, "ShadcnInput");
    token_fn!(shadcn_muted_foreground, "ShadcnMutedForeground");
    token_fn!(shadcn_destructive, "ShadcnDestructive");
    token_fn!(shadcn_card, "ShadcnCard");
}
