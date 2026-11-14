//! Shared product identity and embedded branding assets.

use std::sync::Arc;

use egui::{ColorImage, Context, IconData, TextureHandle, TextureOptions};
use image::GenericImageView;

pub const APP_NAME: &str = "NØVA Flash Studio";
pub const BUNDLE_ID: &str = "studio.nova.flash";

/// Edition-aware display name (M19): base name, Edu or Enterprise variant.
pub fn app_name() -> &'static str {
    crate::edition::profile().app_name
}

/// Edition-aware window-manager / viewport identifier.
pub fn bundle_id() -> &'static str {
    crate::edition::profile().bundle_id
}

/// Edition tagline shown on the splash screen.
pub fn tagline() -> &'static str {
    crate::edition::profile().tagline
}

/// Edition audience line shown on the splash screen.
pub fn audience() -> &'static str {
    crate::edition::profile().audience
}
pub const COMPANY: &str = "NØVA Flash Studio contributors";
pub const COPYRIGHT: &str = "MIT OR Apache-2.0";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const LOGO_PNG: &[u8] = include_bytes!("../../../assets/branding/nova-logo-512.png");
const WINDOW_ICON_PNG: &[u8] = include_bytes!("../../../assets/icons/nova-flash-studio-256.png");

/// Decode the 512 px splash asset into an egui texture.
pub fn load_logo_texture(ctx: &Context) -> Result<TextureHandle, String> {
    let image = image::load_from_memory(LOGO_PNG)
        .map_err(|error| format!("Logo konnte nicht dekodiert werden: {error}"))?;
    let (width, height) = image.dimensions();
    let rgba = image.to_rgba8();
    let color =
        ColorImage::from_rgba_unmultiplied([width as usize, height as usize], rgba.as_raw());
    Ok(ctx.load_texture("nova-branding-logo", color, TextureOptions::LINEAR))
}

/// Window icon passed to the window manager. Failure is non-fatal: the app can
/// still start with the platform default icon.
pub fn window_icon() -> Result<Arc<IconData>, String> {
    let image = image::load_from_memory(WINDOW_ICON_PNG)
        .map_err(|error| format!("Fenster-Icon konnte nicht dekodiert werden: {error}"))?;
    let (width, height) = image.dimensions();
    let rgba = image.to_rgba8();
    Ok(Arc::new(IconData {
        rgba: rgba.into_raw(),
        width,
        height,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_logo_is_a_square_icon() {
        let image = image::load_from_memory(LOGO_PNG).expect("embedded logo");
        assert_eq!(image.dimensions(), (512, 512));
    }

    #[test]
    fn window_icon_matches_branding_asset() {
        let icon = window_icon().expect("window icon");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        assert!(!icon.is_empty());
    }

    #[test]
    fn identity_is_consistent() {
        assert_eq!(APP_NAME, "NØVA Flash Studio");
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
        assert_eq!(BUNDLE_ID, "studio.nova.flash");
    }
}
