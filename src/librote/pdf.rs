use filesize::PathExt;
use genpdf::{elements, fonts};
use glob::glob;
use log::{debug, info};
use std::convert::From;
use std::fs;
use std::process::Command;

use crate::librote::error;
use crate::librote::OcrPlan;

// Google drive OCR for PDF file has a 2 MB hard limit
const GOOGLE_DRIVE_OCR_LIMIT: u64 = 2_000_000;

const FONT_DIRS: &[&str] = &[
    "/usr/share/fonts/liberation",
    "/usr/share/fonts/truetype/liberation",
];

const DEFAULT_FONT_NAME: &'static str = "LiberationSans";

pub fn gen_pdf(input: &str) -> Result<(), error::Error> {
    let ocr_plan: OcrPlan =
        toml::from_str(&fs::read_to_string("ocr_plan.toml").expect("could not read ocr_plan.toml"))
            .expect("Could not read OCR plan");
    let mut current_chunk: u8 = 1;
    let mut current_size = 0;

    let mut current_pdf_vec: Vec<String> = Vec::new();

    for i in glob(&format!("{}/*", input)).expect("Failed to read glob pattern") {
        match i {
            Ok(path) => {
                if ocr_plan.ignore(String::from(path.to_str().unwrap())) {
                    continue;
                } else {
                    let current_file_size = path.size_on_disk().expect("Could not read file size");
                    if current_size + current_file_size > GOOGLE_DRIVE_OCR_LIMIT {
                        write_pdf(current_pdf_vec, current_chunk)?;
                        current_pdf_vec = Vec::new();
                        current_chunk += 1;
                        current_size = 0;
                    }
                    current_pdf_vec.push(String::from(path.to_str().unwrap()));
                    debug!(
                        "Added `{}` size `{}` to pdf_chunk {}",
                        path.display(),
                        current_file_size,
                        current_chunk
                    );
                    current_size += current_file_size;
                }
            }
            Err(_e) => (),
        }
    }
    Ok(())
}

fn write_pdf(image_vec: Vec<String>, chunk_number: u8) -> Result<(), error::Error> {
    let a6_paper_size = genpdf::Size::new(105, 148);
    let font_dir = FONT_DIRS
        .iter()
        .filter(|path| std::path::Path::new(path).exists())
        .next()
        .expect("Could not find font directory");
    let default_font =
        fonts::from_files(font_dir, DEFAULT_FONT_NAME, Some(fonts::Builtin::Helvetica))
            .expect("Failed to load the default font family");
    let mut doc = genpdf::Document::new(default_font);
    doc.set_minimal_conformance();
    doc.set_paper_size(a6_paper_size);
    for path in image_vec {
        doc.push(elements::Image::from_path(path).expect("could not push image to pdf file"));
        doc.push(elements::PageBreak::new());
    }
    doc.render_to_file(format!("tmp_{:03}.pdf", chunk_number))
        .expect("Could not write to pdf file");
    // pass the output pdf to `ps2pdf` to significantly reduce size due to a known issue of genpdf
    Command::new("ps2pdf")
        .arg(format!("tmp_{:03}.pdf", chunk_number))
        .arg(format!("chunk_{:03}.pdf", chunk_number))
        .status()
        .expect("Could not spawn `ps2pdf`");
    fs::remove_file(format!("tmp_{:03}.pdf", chunk_number))
        .expect("could not remove the pdf from `genpdf`");
    info!("Finished writing pdf file for chunk {}", chunk_number);
    Ok(())
}
