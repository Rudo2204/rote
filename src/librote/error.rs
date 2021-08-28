#![allow(clippy::enum_variant_names)]
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Glob Error: {0}")]
    GlobErr(#[from] glob::GlobError),
    #[error("ImageError when translating image to luma8")]
    ImageErr(#[from] image::ImageError),
}
