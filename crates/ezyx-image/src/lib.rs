//! # ezyx-image
//!
//! Turning pictures into things a router can cut.
//!
//! | Pipeline | Function | Result |
//! |---|---|---|
//! | Sketch / logo → outline vectors (2D cut, pocket, V-carve) | [`trace::trace_outlines`] | closed polylines |
//! | Sketch (pen lines) → centreline vectors (single-line engrave) | [`trace::trace_centerlines`] | open polylines |
//! | Photo / greyscale → 2.5D relief | [`relief::image_to_relief`] | heightmap |
//! | Photo → V-carve grooves (shaded "photo V-carve") | [`photo_vcarve::grooves`] | 3D polylines |
//! | Silhouette → inflated 3D (image-to-3D baseline) | [`inflate::inflate`] | front/back heightmaps |
//!
//! Image space: pixel (0,0) top-left. Output geometry is in mm with Y up, origin bottom-left.

pub mod bitmap;
pub mod gray;
pub mod inflate;
pub mod photo_vcarve;
pub mod relief;
pub mod trace;

pub use bitmap::Bitmap;
pub use gray::GrayImage;

/// Load any supported image file as greyscale (alpha composited onto white).
pub fn load_gray(path: &std::path::Path) -> Result<GrayImage, image::ImageError> {
    Ok(GrayImage::from_dynamic(&image::open(path)?))
}
