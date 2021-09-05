#![allow(dead_code)]
use glob::glob;
use log::{debug, info, trace};

use crate::librote::error;
use crate::librote::OcrPlan;

enum PagePropertise {
    Image,
    TextPage,
    EmptyPage,
}

pub fn plan(
    directory_input: &str,
    image_threadhold: u32,
    empty_page_threadhold: u32,
) -> Result<String, error::Error> {
    let mut empty_page = Vec::new();
    let mut image_page = Vec::new();

    for i in glob(&format!("{}/*", directory_input)).expect("Failed to read glob pattern") {
        match i {
            Ok(path) => {
                let image = image::open(&path)?.to_luma8();
                let hist = imageproc::stats::histogram(&image);
                let mut channel = hist.channels[0];
                channel.sort();
                let mean = channel[128];
                debug!("Processing: {:?}, mean = {}", &path.display(), mean);

                let _page_propertise = if mean <= empty_page_threadhold {
                    info!("{:?} is likely an empty page", &path.display());
                    empty_page.push(String::from(path.to_str().unwrap()));
                    PagePropertise::EmptyPage
                } else if mean > image_threadhold {
                    info!("{:?} is likely an image", &path.display());
                    image_page.push(String::from(path.to_str().unwrap()));
                    PagePropertise::Image
                } else {
                    trace!("{:?} is likely a normal text page", &path.display());
                    PagePropertise::TextPage
                };
            }
            Err(_e) => (),
        }
    }

    let ocr_plan = OcrPlan::new(empty_page, image_page, Vec::new());
    let toml = toml::to_string(&ocr_plan).unwrap();
    Ok(toml)
}
