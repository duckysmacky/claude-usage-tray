use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use image::GenericImageView;
use tracing::warn;

use crate::state::UsageStatus;

struct IconAsset {
    png: Vec<u8>,
    pixmap: ksni::Icon,
}

static LOADING: LazyLock<Option<IconAsset>> = LazyLock::new(|| load("loading.png"));
static DEFAULT: LazyLock<Option<IconAsset>> = LazyLock::new(|| load("default.png"));
static LOGIN: LazyLock<Option<IconAsset>> = LazyLock::new(|| load("login.png"));
static ERROR: LazyLock<Option<IconAsset>> = LazyLock::new(|| load("error.png"));

fn asset(status: &UsageStatus) -> Option<&'static IconAsset> {
    match status {
        UsageStatus::Loading => LOADING.as_ref(),
        UsageStatus::Ok => DEFAULT.as_ref(),
        UsageStatus::NeedsLogin => LOGIN.as_ref(),
        UsageStatus::Error(_) => ERROR.as_ref(),
    }
}

pub fn theme_name(status: &UsageStatus) -> &'static str {
    match status {
        UsageStatus::Loading => "image-loading",
        UsageStatus::Ok => "utilities-system-monitor",
        UsageStatus::NeedsLogin => "dialog-password",
        UsageStatus::Error(_) => "dialog-error",
    }
}

/// Returns an empty name when a custom pixmap is available, causing tray hosts
/// to prefer the supplied image. Otherwise, falls back to the system icon theme.
pub fn icon_name(status: &UsageStatus) -> &'static str {
    if asset(status).is_some() {
        ""
    } else {
        theme_name(status)
    }
}

pub fn pixmap(status: &UsageStatus) -> Vec<ksni::Icon> {
    asset(status)
        .map(|asset| vec![asset.pixmap.clone()])
        .unwrap_or_default()
}

pub fn png(status: &UsageStatus) -> Vec<u8> {
    asset(status)
        .map(|asset| asset.png.clone())
        .unwrap_or_default()
}

fn load(filename: &str) -> Option<IconAsset> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(filename);

    let png = match fs::read(&path) {
        Ok(png) => png,
        Err(error) => {
            warn!(path = %path.display(), %error, "custom icon unavailable; using theme icon");
            return None;
        }
    };

    let image = match image::load_from_memory_with_format(&png, image::ImageFormat::Png) {
        Ok(image) => image,
        Err(error) => {
            warn!(path = %path.display(), %error, "invalid custom icon; using theme icon");
            return None;
        }
    };

    let (width, height) = image.dimensions();
    let mut data = image.into_rgba8().into_raw();

    // StatusNotifierItem pixmaps use ARGB32 in network byte order; image gives RGBA.
    for pixel in data.chunks_exact_mut(4) {
        pixel.rotate_right(1);
    }

    Some(IconAsset {
        png,
        pixmap: ksni::Icon {
            width: width as i32,
            height: height as i32,
            data,
        },
    })
}
