use glob::glob;
use log::{debug, info, trace};
use serde::Serialize;

const MAGIC_THRESHOLD_MEAN_NUMBER: u32 = 750;

use crate::librote::error;

enum PagePropertise {
    Image,
    TextPage,
    EmptyPage,
}

#[derive(Serialize)]
struct OcrPlan {
    plan: Plan,
}

#[derive(Serialize)]
struct Plan {
    empty_page: Vec<String>,
    image_page: Vec<String>,
    ignore_page: Vec<String>,
}

pub fn planning(directory_input: &str) -> Result<String, error::Error> {
    let mut empty_page = Vec::new();
    let mut image_page = Vec::new();

    for i in glob(&format!("{}/*", directory_input)).expect("Failed to read glob pattern") {
        match i {
            Ok(path) => {
                let image = image::open(&path)?.to_luma8();
                let hist = imageproc::stats::histogram(&image);
                let mut channel = hist.channels[0];
                //debug!("{:?}", channel);
                channel.sort();
                let mean = channel[128];
                debug!("Processing: {:?}, mean = {}", &path.display(), mean);

                let _page_propertise = if mean == 0 {
                    info!("{:?} is likely an empty page", &path.display());
                    empty_page.push(String::from(path.to_str().unwrap()));
                    PagePropertise::EmptyPage
                } else if mean > MAGIC_THRESHOLD_MEAN_NUMBER {
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
    let plan = Plan {
        empty_page,
        image_page,
        ignore_page: Vec::new(),
    };

    let ocr_plan = OcrPlan { plan };

    let toml = toml::to_string(&ocr_plan).unwrap();
    Ok(toml)
}
