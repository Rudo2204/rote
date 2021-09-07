use log::{debug, info};
use regex::Regex;
use scraper::{Html, Selector};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::process::Command;

pub fn parse_ocr_html(num_chunk: u8, font_size_threadhold: u8) {
    let font_size_regex = Regex::new("font-size:(\\d+)pt").unwrap();
    for i in 1..=num_chunk {
        let html = fs::read_to_string(format!("tidy_{:02}.html", i)).unwrap();

        let document = Html::parse_document(&html);
        let selector_span = Selector::parse("span").unwrap();

        let mut current_line: String = String::new();

        let mut font_size_vec: Vec<u8> = Vec::new();
        let mut proc_text_vec: Vec<String> = Vec::new();
        for s in document.select(&selector_span) {
            let inner_html = s.inner_html();
            if inner_html.contains("\n") {
                proc_text_vec.push(current_line.clone());
                debug!("`{}`", inner_html);
                current_line = inner_html.replace("\n", "");
                let caps = font_size_regex
                    .captures(s.value().attr("style").unwrap())
                    .unwrap();
                let font_size: u8 = caps.get(1).unwrap().as_str().parse().unwrap();
                debug!("font-size = {}", font_size);
                font_size_vec.push(font_size);
            } else {
                current_line.push_str(&inner_html);
            }
        }
        proc_text_vec.push(current_line);

        let mut final_text = String::new();
        for (index, font_size) in font_size_vec.into_iter().enumerate() {
            if font_size > font_size_threadhold {
                let text = &proc_text_vec[index + 1];
                if text.contains("PAGE") {
                    final_text.push_str("-----");
                } else if text.contains("MARKER") {
                    final_text.push_str("-----\n");
                } else {
                    final_text.push_str(&format!("{}\n", text));
                }
            }
        }

        let mut output_file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(format!("raw_{:02}.txt", i))
            .unwrap();
        write!(output_file, "{}", final_text).expect("could not write output to `raw.txt`");
        info!("Finished writing raw_{:02}.txt", i);
    }
}

pub fn tidy(num_chunk: u8) {
    for chunk_number in 1..=num_chunk {
        Command::new("tidy")
            .arg("--show-warnings")
            .arg("false")
            .arg("-quiet")
            .arg("-output")
            .arg(format!("tidy_{:02}.html", chunk_number))
            .arg(format!("ocr_{:02}.html", chunk_number))
            .status()
            .expect("Could not spawn `tidy`");
    }
}
