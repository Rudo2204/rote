use epub_builder::{EpubBuilder, EpubContent, EpubVersion, ReferenceType, Zip, ZipLibrary};
use log::{debug, info};
use regex::Regex;
use serde::Deserialize;
use std::ffi::OsStr;
use std::fmt::Write;
use std::fs::{self, OpenOptions};
use std::path::Path;

use crate::librote::error;

#[derive(Deserialize)]
struct EpubPlan {
    title: String,
    author: String,
    lang: String,
    generator: String,
    toc_name: String,
    cover_image: String,
    raw: String,
}

enum Action {
    InsertToc,
    InsertPrefaceImage,
    InsertTitlePage,
    InsertContent,
    InsertContentWithChapter,
    InsertImage,
    InsertAtogaki,
    InsertColophon,
    InsertColophonText,
    InsertCopyright,
    InsertBibliography,
    InsertGaiji,
}

fn get_image_mime_type(path: &str) -> &str {
    let extension = Path::new(path).extension().and_then(OsStr::to_str).unwrap();
    if extension == "png" {
        "image/png"
    } else if extension == "jpg" {
        "image/jpeg"
    } else {
        panic!("Unknown image type")
    }
}

fn read_epub_plan(path: &str) -> EpubPlan {
    let raw_plan = fs::read_to_string(path).expect("Could not read epub plan");
    let epub_plan: EpubPlan = toml::from_str(&raw_plan).expect("Could not parse raw plan file");
    epub_plan
}

pub fn gen_epub(epub_plan_path: &str, image_path: &str, output_epub_path: &str) {
    let epub_plan = read_epub_plan(epub_plan_path);
    let epub_file = OpenOptions::new()
        .write(true)
        .create(true)
        .open(output_epub_path)
        .unwrap();

    let book_style = fs::read_to_string("book-style.css").expect("Could not read `book-style.css`");
    let fit_style = fs::read_to_string("fit-style.css").expect("Could not read `fit-style.css`");
    let keep_space_img = fs::read("keep-space.jpg").expect("Could not read `keep-space.jpg");
    let cover_image_path = format!("{}/{}", image_path, &epub_plan.cover_image);
    let cover_image = fs::read(&cover_image_path).expect("Could not read cover image");
    let cover_image_mime_type = get_image_mime_type(&cover_image_path);

    let unprocessed_raw = fs::read_to_string(&epub_plan.raw).expect("Could not read `raw`");
    let raw = japanese_ize_raw(&unprocessed_raw);

    let dont_indent_re = Regex::new(r#"^　|『|「|（|＜|〔|｛|｟|〈|《|【|〖|〘|〚|─"#).unwrap();
    let custom_re = Regex::new(r#"#(.*)#"#).unwrap();
    let toc_replace_re = Regex::new(r#"REPLACE_ME"#).unwrap();
    let gaiji_replace_re = Regex::new(r#"#gaiji,(.*?)#"#).unwrap();

    let mut toc_content = generate_toc_xhtml(&epub_plan, &raw);
    let mut current_chapter_text = String::new();
    let mut current_mokuji: u16 = 1;
    let mut is_new_chapter = false;
    let mut chapter_vec = Vec::new();

    let mut actions: Vec<(Action, String)> = Vec::new();

    for line in raw.lines() {
        if line.contains("#toc#") {
            actions.push((Action::InsertToc, "".to_string()));
            debug!("Added InsertToc action");
        } else if line.contains("#gaiji,") {
            // add the image
            let mut gaiji_pics = vec![];
            for cap in gaiji_replace_re.captures_iter(&line) {
                gaiji_pics.push(cap.get(1).unwrap().as_str())
            }
            for pic in gaiji_pics {
                actions.push((Action::InsertGaiji, pic.to_string()));
                debug!("Added InsertGaiji action");
            }

            // regex replace the text
            let mut replaced_line: String = line.to_string();
            while gaiji_replace_re.is_match(&replaced_line) {
                replaced_line = gaiji_replace_re
                    .replace(&replaced_line, |caps: &regex::Captures| {
                        format!(
                            "<img class=\"gaiji\" src=\"../image/{}\" alt=\"\" />",
                            &caps[1]
                        )
                    })
                    .to_string()
            }

            // write the line
            let dont_indent = dont_indent_re.is_match(line);
            if dont_indent {
                write!(current_chapter_text, "<p>{}</p>\n", replaced_line).unwrap();
            } else {
                //intentionally use Japanese space
                write!(current_chapter_text, "<p>　{}</p>\n", replaced_line).unwrap();
            }
        } else if line.contains("#end-bibliography#") {
            actions.push((Action::InsertBibliography, current_chapter_text.clone()));
            debug!("Added InsertBibliography action");
            current_chapter_text = String::new();
        } else if line.contains("#end-copyright#") {
            actions.push((Action::InsertCopyright, current_chapter_text.clone()));
            debug!("Added InsertCopyright action");
            current_chapter_text = String::new();
        } else if line.contains("#end-colophon-text#") {
            actions.push((Action::InsertColophonText, current_chapter_text.clone()));
            debug!("Added InsertColophonText action");
            current_chapter_text = String::new();
        } else if line.contains("#end-atogaki#") {
            actions.push((Action::InsertAtogaki, current_chapter_text.clone()));
            debug!("Added InsertAtogaki action");
            current_chapter_text = String::new();
        } else {
            let try_capture = custom_re.captures(line);
            match try_capture {
                Some(caps) => {
                    let command = caps.get(1).unwrap().as_str();
                    debug!("Captured command `{}`", command);
                    let custom_command: Vec<&str> = command.split(",").collect();
                    debug!(
                        "command = `{}`, arg = `{}`",
                        custom_command[0], custom_command[1]
                    );
                    match custom_command[0] {
                        "title-page" => {
                            debug!("Added InsertTitlePage action");
                            actions.push((Action::InsertTitlePage, custom_command[1].to_string()));
                        }
                        "preface-img" => {
                            debug!("Added InsertPrefaceImage action");
                            actions
                                .push((Action::InsertPrefaceImage, custom_command[1].to_string()));
                        }
                        "gaiji" => {
                            continue;
                        }
                        "chapter" => {
                            if !current_chapter_text.is_empty() {
                                if is_new_chapter || chapter_vec.len() > 0 {
                                    debug!("Added InsertContentWithChapter action");
                                    actions.push((
                                        Action::InsertContentWithChapter,
                                        current_chapter_text.clone(),
                                    ));
                                } else {
                                    is_new_chapter = true;
                                    debug!("Added InsertContent action");
                                    actions.push((
                                        Action::InsertContent,
                                        current_chapter_text.clone(),
                                    ));
                                }
                                current_chapter_text = String::new();
                            }

                            write!(
                                current_chapter_text,
                                "<p class=\"mfont font-1em30\" id=\"mokuji-{:04}\">{}</p>\n<p><br/></p>\n",
                                current_mokuji, custom_command[1]
                            )
                            .unwrap();
                            chapter_vec.push(custom_command[1]);
                            current_mokuji += 1;
                        }
                        "img" => {
                            if !current_chapter_text.is_empty() {
                                if is_new_chapter {
                                    is_new_chapter = false;
                                    debug!("Added InsertContentWithChapter action");
                                    actions.push((
                                        Action::InsertContentWithChapter,
                                        current_chapter_text.clone(),
                                    ));
                                } else {
                                    debug!("Added InsertContent action");
                                    actions.push((
                                        Action::InsertContent,
                                        current_chapter_text.clone(),
                                    ));
                                }
                                current_chapter_text = String::new();
                            }
                            actions.push((Action::InsertImage, custom_command[1].to_string()));
                            debug!("Added InsertImage action");
                        }
                        "atogaki" => {
                            actions.push((Action::InsertContent, current_chapter_text.clone()));
                            debug!("Added InsertContent action");

                            current_chapter_text = String::new();
                            write!(
                                current_chapter_text,
                                "<p class=\"mfont font-1em30\" id=\"mokuji-{:04}\">　{}</p>\n<p><br/></p>\n",
                                current_mokuji, custom_command[1]
                            )
                            .unwrap();
                            current_mokuji += 1;
                        }
                        "bibliography" => {
                            debug!("Added InsertContentWithChapter action");
                            actions.push((
                                Action::InsertContentWithChapter,
                                current_chapter_text.clone(),
                            ));
                            current_chapter_text = String::new();
                        }
                        "fill" => {
                            write!(
                                current_chapter_text,
                                "<p><br/></p>\n<div class=\"align-end\">\n<p>{}</p>\n</div>\n",
                                custom_command[1]
                            )
                            .unwrap();
                        }
                        "colophon" => {
                            actions.push((Action::InsertColophon, custom_command[1].to_string()));
                            debug!("Added InsertColophon action");
                        }
                        "toc-chapter" => {
                            continue;
                        }
                        "no-indent" => {
                            write!(current_chapter_text, "<p>{}</p>\n", custom_command[1]).unwrap();
                        }
                        _ => {
                            log::error!("`{}` is an unimplemented command", custom_command[0]);
                            unimplemented!("Unimplemented custom command");
                        }
                    }
                }
                None => {
                    if line.contains("----------") {
                        continue;
                    } else if line.is_empty() {
                        write!(current_chapter_text, "<p><br/></p>\n").unwrap();
                    } else {
                        let dont_indent = dont_indent_re.is_match(line);
                        if dont_indent {
                            write!(current_chapter_text, "<p>{}</p>\n", line).unwrap();
                        } else {
                            //intentionally use Japanese space
                            write!(current_chapter_text, "<p>　{}</p>\n", line).unwrap();
                        }
                    }
                }
            }
        }
    }

    let mut tmp_toc_paragraph_number: u16 = 1;
    for (action, _) in &actions {
        match action {
            Action::InsertContent | Action::InsertImage => {
                tmp_toc_paragraph_number += 1;
            }
            Action::InsertCopyright | Action::InsertContentWithChapter | Action::InsertAtogaki => {
                toc_content = toc_replace_re
                    .replace(&toc_content, format!("{:03}", tmp_toc_paragraph_number))
                    .to_string();

                tmp_toc_paragraph_number += 1;
            }
            _ => (),
        }
    }

    let mut init_epub = EpubBuilder::new(ZipLibrary::new().unwrap()).unwrap();
    let mut epub = init_epub
        .epub_version(EpubVersion::V30)
        .metadata("author", &epub_plan.author)
        .unwrap()
        .metadata("title", &epub_plan.title)
        .unwrap()
        .metadata("lang", &epub_plan.lang)
        .unwrap()
        .metadata("generator", &epub_plan.generator)
        .unwrap()
        .metadata("toc_name", &epub_plan.toc_name)
        .unwrap()
        .add_resource("style/book-style.css", book_style.as_bytes(), "text/css")
        .unwrap()
        .add_resource("style/fit-style.css", fit_style.as_bytes(), "text/css")
        .unwrap()
        .add_resource(
            "image/keep-space.jpg",
            keep_space_img.as_slice(),
            "image/jpeg",
        )
        .unwrap()
        .add_cover_image(
            "image/cover-image.jpg",
            cover_image.as_slice(),
            cover_image_mime_type,
        )
        .unwrap()
        .add_content(
            EpubContent::new(
                "xhtml/p-cover.xhtml",
                generate_cover_image_xhtml(&epub_plan).as_bytes(),
            )
            .title("表紙")
            .reftype(ReferenceType::Cover),
        )
        .unwrap();

    let mut current_paragraph_number: u16 = 1;
    let mut current_preface_image_number = 1;
    let mut current_chapter_vec_index = 0;
    let mut text_inserted = false;

    for (action, action_content) in &actions {
        match action {
            Action::InsertToc => {
                epub = epub
                    .add_content(
                        EpubContent::new("xhtml/p-toc.xhtml", toc_content.as_bytes())
                            .title("目次")
                            .reftype(ReferenceType::Toc),
                    )
                    .expect("Could not add toc");
                info!("Inserted TOC");
            }
            Action::InsertPrefaceImage => {
                epub = add_preface_image(
                    epub,
                    &epub_plan,
                    image_path,
                    action_content,
                    current_preface_image_number,
                )
                .unwrap();
                info!(
                    "Inserted preface image `{}` with preface number `{:03}`",
                    action_content, current_preface_image_number
                );
                current_preface_image_number += 1;
            }
            Action::InsertTitlePage => {
                epub = add_title_page(epub, &epub_plan, image_path, action_content).unwrap();
                info!("Inserted title page image `{}`", action_content);
            }
            Action::InsertAtogaki => {
                let content_formatted = generate_content_xhtml(&epub_plan, action_content);

                epub.add_content(
                    EpubContent::new(
                        format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                        content_formatted.as_bytes(),
                    )
                    .title("奥付")
                    .reftype(ReferenceType::Afterword),
                )
                .unwrap();
                info!(
                    "Inserted atogaki content with paragraph number `{:03}`",
                    current_paragraph_number
                );
                current_paragraph_number += 1;
            }
            Action::InsertContentWithChapter => {
                let content_formatted = generate_content_xhtml(&epub_plan, action_content);

                if text_inserted {
                    epub.add_content(
                        EpubContent::new(
                            format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                            content_formatted.as_bytes(),
                        )
                        .title(chapter_vec[current_chapter_vec_index]),
                    )
                    .unwrap();
                } else {
                    epub.add_content(
                        EpubContent::new(
                            format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                            content_formatted.as_bytes(),
                        )
                        .title(chapter_vec[current_chapter_vec_index])
                        .reftype(ReferenceType::Text),
                    )
                    .unwrap();
                    text_inserted = true;
                }
                info!(
                    "Inserted content with chapter, paragraph number `{:03}`",
                    current_paragraph_number
                );
                current_chapter_vec_index += 1;
                current_paragraph_number += 1;
            }
            Action::InsertContent => {
                let content_formatted = generate_content_xhtml(&epub_plan, action_content);

                if text_inserted {
                    epub.add_content(EpubContent::new(
                        format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                        content_formatted.as_bytes(),
                    ))
                    .unwrap();
                } else {
                    epub.add_content(
                        EpubContent::new(
                            format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                            content_formatted.as_bytes(),
                        )
                        .reftype(ReferenceType::Text),
                    )
                    .unwrap();
                    text_inserted = true;
                }
                info!(
                    "Inserted content with paragraph number `{:03}`",
                    current_paragraph_number
                );
                current_paragraph_number += 1;
            }
            Action::InsertImage => {
                epub = add_normal_image(
                    epub,
                    &epub_plan,
                    image_path,
                    action_content,
                    current_paragraph_number,
                )
                .unwrap();
                info!(
                    "Inserted image `{}` with paragraph number `{:03}`",
                    action_content, current_paragraph_number,
                );
                current_paragraph_number += 1;
            }
            Action::InsertColophon => {
                epub = add_colophon_image(epub, &epub_plan, image_path, action_content).unwrap();
                info!("Inserted colophon image `{}`", action_content);
            }
            Action::InsertColophonText => {
                let content_formatted = generate_content_xhtml(&epub_plan, action_content);

                if text_inserted {
                    epub.add_content(EpubContent::new(
                        format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                        content_formatted.as_bytes(),
                    ))
                    .unwrap();
                } else {
                    epub.add_content(
                        EpubContent::new(
                            format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                            content_formatted.as_bytes(),
                        )
                        .reftype(ReferenceType::Colophon),
                    )
                    .unwrap();
                    text_inserted = true;
                }
                info!(
                    "Inserted colophon (text) content with paragraph number `{:03}`",
                    current_paragraph_number
                );
            }
            Action::InsertCopyright => {
                let content_formatted = generate_content_xhtml(&epub_plan, action_content);

                if text_inserted {
                    epub.add_content(EpubContent::new(
                        format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                        content_formatted.as_bytes(),
                    ))
                    .unwrap();
                } else {
                    epub.add_content(
                        EpubContent::new(
                            format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                            content_formatted.as_bytes(),
                        )
                        .reftype(ReferenceType::Copyright),
                    )
                    .unwrap();
                    text_inserted = true;
                }
                info!(
                    "Inserted copyright content with paragraph number `{:03}`",
                    current_paragraph_number
                );
                current_paragraph_number += 1;
            }
            Action::InsertBibliography => {
                let content_formatted = generate_content_xhtml(&epub_plan, action_content);

                if text_inserted {
                    epub.add_content(EpubContent::new(
                        format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                        content_formatted.as_bytes(),
                    ))
                    .unwrap();
                } else {
                    epub.add_content(
                        EpubContent::new(
                            format!("xhtml/p-{:03}.xhtml", current_paragraph_number),
                            content_formatted.as_bytes(),
                        )
                        .reftype(ReferenceType::Bibliography),
                    )
                    .unwrap();
                    text_inserted = true;
                }
                info!(
                    "Inserted bibliography content with paragraph number `{:03}`",
                    current_paragraph_number
                );
                current_paragraph_number += 1;
            }
            Action::InsertGaiji => {
                epub = add_gaiji_image(epub, image_path, action_content).unwrap();
                info!(
                    "Inserted image `{}` to paragraph number `{:03}`",
                    action_content, current_paragraph_number,
                );
            }
        }
    }

    epub.generate(epub_file).unwrap();
}

fn generate_content_xhtml(epub_plan: &EpubPlan, content: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html
 xmlns="http://www.w3.org/1999/xhtml"
 xmlns:epub="http://www.idpf.org/2007/ops"
 xml:lang="{}"
 class="vrtl"
>
<head>
<meta charset="UTF-8"/>
<title>{}</title>
<link rel="stylesheet" type="text/css" href="../style/book-style.css"/>
</head>
<body class="p-honmon top-left-on">
<p class="dummy"><img class="keep-space" src="../image/keep-space.jpg"/></p>
<div class="main">
{}</div>
</body>
</html>"#,
        epub_plan.lang, epub_plan.title, content
    )
}

fn japanese_ize_raw(unprocessed_raw: &str) -> String {
    let mut raw = unprocessed_raw.to_string();

    let three_dot = Regex::new(r#"\.\.\."#).unwrap();
    let question_mark = Regex::new(r#"\?"#).unwrap();
    let exclamation_mark = Regex::new(r#"!"#).unwrap();
    let left_parenthesis = Regex::new(r#"\("#).unwrap();
    let right_parenthesis = Regex::new(r#"\)"#).unwrap();
    let tilde = Regex::new(r#"~"#).unwrap();

    raw = three_dot.replace_all(&raw, "…").to_string();
    raw = question_mark.replace_all(&raw, "？").to_string();
    raw = left_parenthesis.replace_all(&raw, "（").to_string();
    raw = right_parenthesis.replace_all(&raw, "）").to_string();
    raw = exclamation_mark.replace_all(&raw, "！").to_string();
    raw = tilde.replace_all(&raw, "〜").to_string();

    raw
}

fn generate_image_xhtml(epub_plan: &EpubPlan, image_name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html
 xmlns="http://www.w3.org/1999/xhtml"
 xmlns:epub="http://www.idpf.org/2007/ops"
 xml:lang="{}"
 class="hltr"
>
<head>
<meta charset="UTF-8"/>
<title>{}</title>
<link rel="stylesheet" type="text/css" href="../style/book-style.css"/>
</head>
<body class="p-image middle-center-on">
<p class="dummy"><img class="keep-space" src="../image/keep-space.jpg"/></p>
<div class="main">
<p><img class="fit" src="../image/{}" alt=""/></p>
</div>
</body>
</html>"#,
        epub_plan.lang, epub_plan.title, image_name
    )
}

fn generate_preface_image_xhtml(epub_plan: &EpubPlan, image_name: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html
 xmlns="http://www.w3.org/1999/xhtml"
 xmlns:epub="http://www.idpf.org/2007/ops"
 xml:lang="{}"
 class="hltr"
>
<head>
<meta charset="UTF-8"/>
<title>{}</title>
<link rel="stylesheet" type="text/css" href="../style/fit-style.css"/>
<meta name="viewport" content="width=2048, height=1444"/>
</head>
<body class="p-image">
<div class="main align-center">
<p><img class="fit" src="../image/{}" alt=""/></p>
</div>
</body>
</html>"#,
        epub_plan.lang, epub_plan.title, image_name
    )
}

fn generate_cover_image_xhtml(epub_plan: &EpubPlan) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html
 xmlns="http://www.w3.org/1999/xhtml"
 xmlns:epub="http://www.idpf.org/2007/ops"
 xml:lang="{}"
 class="hltr"
>
<head>
<meta charset="UTF-8"/>
<title>{}</title>
<link rel="stylesheet" type="text/css" href="../style/book-style.css"/>
</head>
<body epub:type="cover" class="p-cover">
<div class="main">
<p><img class="fit" src="../image/cover-image.jpg"/></p>
</div>
</body>
</html>
        "#,
        epub_plan.lang, epub_plan.title
    )
}

fn add_title_page<'a, Z: Zip>(
    epub: &'a mut EpubBuilder<Z>,
    epub_plan: &'a EpubPlan,
    img_path: &'a str,
    img_name: &'a str,
) -> Result<&'a mut EpubBuilder<Z>, error::Error> {
    let title_page_content = generate_preface_image_xhtml(epub_plan, img_name);
    let img_full_path = format!("{}/{}", img_path, img_name);
    let title_page_img = fs::read(&img_full_path).expect("Could not read title page image");
    epub.add_resource(
        format!("image/{}", img_name),
        title_page_img.as_slice(),
        get_image_mime_type(&img_full_path),
    )
    .expect("Could not add image for title page");
    epub.add_content(
        EpubContent::new("xhtml/p-titlepage.xhtml", title_page_content.as_bytes())
            .reftype(ReferenceType::TitlePage),
    )
    .expect("Could not add content for title page");
    Ok(epub)
}

fn add_gaiji_image<'a, Z: Zip>(
    epub: &'a mut EpubBuilder<Z>,
    img_path: &'a str,
    img_name: &'a str,
) -> Result<&'a mut EpubBuilder<Z>, error::Error> {
    let img_full_path = format!("{}/{}", img_path, img_name);
    let img = fs::read(&img_full_path).expect("Could not read image");
    epub.add_resource(
        format!("image/{}", img_name),
        img.as_slice(),
        get_image_mime_type(&img_full_path),
    )
    .expect("Could not add image");
    Ok(epub)
}

fn add_normal_image<'a, Z: Zip>(
    epub: &'a mut EpubBuilder<Z>,
    epub_plan: &'a EpubPlan,
    img_path: &'a str,
    img_name: &'a str,
    paragraph_number: u16,
) -> Result<&'a mut EpubBuilder<Z>, error::Error> {
    let img_content = generate_image_xhtml(epub_plan, img_name);
    let img_full_path = format!("{}/{}", img_path, img_name);
    let img = fs::read(&img_full_path).expect("Could not read image");
    epub.add_resource(
        format!("image/{}", img_name),
        img.as_slice(),
        get_image_mime_type(&img_full_path),
    )
    .expect("Could not add image");
    epub.add_content(EpubContent::new(
        format!("xhtml/p-{:03}.xhtml", paragraph_number),
        img_content.as_bytes(),
    ))
    .expect("Could not add image as content");
    Ok(epub)
}

fn add_colophon_image<'a, Z: Zip>(
    epub: &'a mut EpubBuilder<Z>,
    epub_plan: &'a EpubPlan,
    img_path: &'a str,
    img_name: &'a str,
) -> Result<&'a mut EpubBuilder<Z>, error::Error> {
    let img_content = generate_image_xhtml(epub_plan, img_name);
    let img_full_path = format!("{}/{}", img_path, img_name);
    let img = fs::read(&img_full_path).expect("Could not read image");
    epub.add_resource(
        format!("image/{}", img_name),
        img.as_slice(),
        get_image_mime_type(&img_full_path),
    )
    .expect("Could not add colophon image");
    epub.add_content(
        EpubContent::new(format!("xhtml/p-colophon.xhtml"), img_content.as_bytes())
            .reftype(ReferenceType::Colophon),
    )
    .expect("Could not add colophon image as content");
    Ok(epub)
}

fn add_preface_image<'a, Z: Zip>(
    epub: &'a mut EpubBuilder<Z>,
    epub_plan: &'a EpubPlan,
    img_path: &'a str,
    img_name: &'a str,
    preface_number: u16,
) -> Result<&'a mut EpubBuilder<Z>, error::Error> {
    let preface_img_content = generate_preface_image_xhtml(epub_plan, img_name);
    let img_full_path = format!("{}/{}", img_path, img_name);
    let preface_img = fs::read(&img_full_path).expect("Could not read preface page image");
    epub.add_resource(
        format!("image/{}", img_name),
        preface_img.as_slice(),
        get_image_mime_type(&img_full_path),
    )
    .expect("Could not add preface image");
    epub.add_content(
        EpubContent::new(
            format!("xhtml/p-fmatter-{:03}.xhtml", preface_number),
            preface_img_content.as_bytes(),
        )
        .reftype(ReferenceType::Preface),
    )
    .expect("Could not add content for preface image");
    Ok(epub)
}

fn generate_toc_xhtml(epub_plan: &EpubPlan, raw: &str) -> String {
    let chapter_re = Regex::new(r#"#toc-chapter,(.*)#"#).unwrap();
    let atogaki_re = Regex::new(r#"#atogaki,(.*)#"#).unwrap();

    let mut toc = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html
 xmlns="http://www.w3.org/1999/xhtml"
 xmlns:epub="http://www.idpf.org/2007/ops"
 xml:lang="{}"
 class="vrtl"
>
<head>
<meta charset="UTF-8"/>
<title>{}</title>
<link rel="stylesheet" type="text/css" href="../style/book-style.css"/>
</head>
<body class="p-toc top-left-off">
<p class="dummy"><img class="keep-space" src="../image/keep-space.jpg"/></p>
<div class="main">
<div class="start-2em">
<p>　<span class="mfont font-1em30">{}</span></p>
<p><br/></p>
<div class="font-1em10">
"#,
        epub_plan.lang, epub_plan.title, epub_plan.toc_name
    );

    let mut current_chapter_number: u8 = 1;
    for caps in chapter_re.captures_iter(raw) {
        let chapter_name = caps.get(1).unwrap().as_str();
        match chapter_name {
            "phantom" => {
                toc.push_str("<p><br/></p>\n");
            }
            _ => {
                toc.push_str(&format!(
                    "<p><a href=\"p-REPLACE_ME.xhtml#mokuji-{:04}\" class=\"mokuji-{:04}\">{}</a></p>\n",
                    current_chapter_number,
                    current_chapter_number,
                    chapter_name,
                ));
                current_chapter_number += 1;
            }
        }
    }

    match atogaki_re.captures(raw) {
        Some(caps) => {
            toc.push_str(&format!(
                "<p><br/></p>\n<div class=\"h-indent-1em\">\n<p>　<a href=\"p-REPLACE_ME.xhtml#mokuji-{:04}\" class=\"mokuji-{:04}\">{}</a></p>\n</div>\n",
                current_chapter_number,
                current_chapter_number,
                caps.get(1).unwrap().as_str()
            ));
        }
        None => (),
    }

    toc.push_str("</div>\n</div>\n</div>\n</body>\n</html>");
    toc
}
