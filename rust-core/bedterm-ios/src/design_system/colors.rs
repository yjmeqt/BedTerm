//! iOS-specific dynamic UIColors built from `bedterm_app` named colour
//! constants. Each accessor returns a `Retained<UIColor>` backed by
//! `+[UIColor colorWithDynamicProvider:]`.

pub use bedterm_app::design_system::colors::{Rgba, TokenPair};

mod ios {
    use bedterm_app::design_system::colors::{
        self as token, Rgba, TokenPair, BACKGROUND, BORDER, CARD, DESTRUCTIVE, INPUT,
        MUTED_FOREGROUND, PRIMARY, PRIMARY_FOREGROUND,
    };
    use block2::StackBlock;
    use objc2::rc::Retained;
    use objc2_ui_kit::{UIColor, UITraitCollection, UIUserInterfaceStyle};
    use std::cell::OnceCell;
    use std::ptr::NonNull;

    fn solid(rgba: Rgba) -> Retained<UIColor> {
        UIColor::colorWithRed_green_blue_alpha(rgba.r, rgba.g, rgba.b, rgba.a)
    }

    fn make_dynamic(pair: TokenPair) -> Retained<UIColor> {
        let light = solid(pair.light);
        let dark = solid(pair.dark);
        let light_keep = light.clone();
        let dark_keep = dark.clone();
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
        let block: block2::RcBlock<dyn Fn(NonNull<UITraitCollection>) -> NonNull<UIColor>> =
            block.copy();
        unsafe { UIColor::colorWithDynamicProvider(&block) }
    }

    macro_rules! cached {
        ($fn_name:ident, $token:path) => {
            pub fn $fn_name() -> Retained<UIColor> {
                thread_local! {
                    static CACHE: OnceCell<Retained<UIColor>> = const { OnceCell::new() };
                }
                CACHE.with(|cell| cell.get_or_init(|| make_dynamic($token)).clone())
            }
        };
    }

    cached!(shadcn_primary, PRIMARY);
    cached!(shadcn_primary_foreground, PRIMARY_FOREGROUND);
    cached!(shadcn_background, BACKGROUND);
    cached!(shadcn_border, BORDER);
    cached!(shadcn_input, INPUT);
    cached!(shadcn_muted_foreground, MUTED_FOREGROUND);
    cached!(shadcn_destructive, DESTRUCTIVE);
    cached!(shadcn_card, CARD);
}

pub use ios::*;
