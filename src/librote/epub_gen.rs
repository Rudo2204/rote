use regex::Regex;
use std::fs::{self, OpenOptions};
use std::io::Write;

pub fn gen_raw_epub(title: &str) {
    let raw_text = fs::read_to_string("raw.txt").unwrap();

    let mut output_file = OpenOptions::new()
        .write(true)
        .create(true)
        .open("p1.xhtml")
        .unwrap();

    writeln!(
        output_file,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html
 xmlns="http://www.w3.org/1999/xhtml"
 xmlns:epub="http://www.idpf.org/2007/ops"
 xml:lang="ja"
 class="vrtl"
>
<head>
<meta charset="UTF-8"/>
<title>{}</title>
<link rel="stylesheet" type="text/css" href="../style/book-style.css"/>
</head>
<body class="p-honmon top-left-on">
<p class="dummy"><img class="keep-space" src="../image/keep-space.jpg"/></p>
<div class="main">"#,
        title
    )
    .unwrap();

    let three_dot = Regex::new(r#"\.\.\."#).unwrap();
    let question_mark = Regex::new(r#"\?"#).unwrap();
    let left_parenthesis = Regex::new(r#"\("#).unwrap();
    let right_parenthesis = Regex::new(r#"\)"#).unwrap();
    let start_with_bracket = Regex::new(r#"^「|（|＜|〔|｛|｟|〈|《|【|〖|〘|〚"#).unwrap();

    for line in raw_text.lines() {
        if line.contains("----------") {
            writeln!(output_file, r#"<!-- pagebreak -->"#).unwrap();
        } else if line.is_empty() {
            writeln!(output_file, "<p><br/></p>").unwrap();
        } else {
            let mut proc_text = line.to_string();
            proc_text = three_dot.replace_all(&proc_text, "…").to_string();
            proc_text = question_mark.replace_all(&proc_text, "？").to_string();
            proc_text = left_parenthesis.replace_all(&proc_text, "（").to_string();
            proc_text = right_parenthesis.replace_all(&proc_text, "）").to_string();
            let text_start_with_bracket = start_with_bracket.is_match(&proc_text);
            if text_start_with_bracket {
                writeln!(output_file, "<p>{}</p>", proc_text).unwrap();
            } else {
                writeln!(output_file, "<p>　{}</p>", proc_text).unwrap(); //intentionally use Japanese space
            }
        }
    }

    writeln!(output_file, "</div>\n</body>\n</html>").unwrap();
}
