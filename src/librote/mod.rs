pub mod epub_gen;
pub mod error;
pub mod gdrive;
pub mod pdf;
pub mod plan;
pub mod process;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct OcrPlan {
    plan: Plan,
}

impl OcrPlan {
    pub fn new(empty_page: Vec<String>, image_page: Vec<String>, ignore_page: Vec<String>) -> Self {
        Self {
            plan: Plan {
                empty_page,
                image_page,
                ignore_page,
            },
        }
    }
    pub fn ignore(&self, path: String) -> bool {
        self.plan.empty_page.contains(&path)
            || self.plan.image_page.contains(&path)
            || self.plan.ignore_page.contains(&path)
    }
}

#[derive(Serialize, Deserialize)]
struct Plan {
    empty_page: Vec<String>,
    image_page: Vec<String>,
    ignore_page: Vec<String>,
}
