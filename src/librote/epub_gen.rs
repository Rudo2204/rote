use epub_builder::{EpubBuilder, EpubContent, EpubVersion, ReferenceType, Zip, ZipLibrary};
use log::{debug, info};
use regex::Regex;
use serde::Deserialize;
use std::fmt::Write;
use std::fs::{self, OpenOptions};

use crate::librote::error;

#[derive(Deserialize)]
struct EpubPlan {
    title: String,
    author: String,
    lang: String,
    generator: String,
    toc_name: String,
    image_mime_type: String,
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

    let image_mime_type = &epub_plan.image_mime_type;
    let book_style = fs::read_to_string("book-style.css").expect("Could not read `book-style.css`");
    let fit_style = fs::read_to_string("fit-style.css").expect("Could not read `fit-style.css`");
    let keep_space_img = fs::read("keep-space.jpg").expect("Could not read `keep-space.jpg");
    let cover_image = fs::read(format!("{}/{}", image_path, &epub_plan.cover_image))
        .expect("Could not read cover image");

    let unprocessed_raw = fs::read_to_string(&epub_plan.raw).expect("Could not read `raw`");
    let raw = japanese_ize_raw(&unprocessed_raw);

    let dont_indent_re = Regex::new(r#"^「|（|＜|〔|｛|｟|〈|《|【|〖|〘|〚|─"#).unwrap();
    let custom_re = Regex::new(r#"#(.*)#"#).unwrap();
    let toc_re = Regex::new(r#"#toc#"#).unwrap();
    let toc_replace_re = Regex::new(r#"REPLACE_ME"#).unwrap();

    let mut toc_content = generate_toc_xhtml(&epub_plan, &raw);
    let mut current_chapter_text = String::new();
    let mut current_mokuji: u16 = 1;
    let mut is_new_chapter = false;

    let mut actions: Vec<(Action, String)> = Vec::new();

    for line in raw.lines() {
        if toc_re.is_match(line) {
            actions.push((Action::InsertToc, "".to_string()));
            debug!("Added InsertToc action");
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
                        "chapter" => {
                            if !current_chapter_text.is_empty() {
                                if is_new_chapter {
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
                                r#"<p class="mfont font-1em30" id="mokuji-{:04}">{}</p>
                                <p><br/></p>"#,
                                current_mokuji, custom_command[1]
                            )
                            .unwrap();
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
                                r#"<p class="mfont font-1em30" id="mokuji-{:04}">　{}</p>
                                <p><br/></p>"#,
                                current_mokuji, custom_command[1]
                            )
                            .unwrap();
                            current_mokuji += 1;
                        }
                        "fill" => {
                            write!(
                                current_chapter_text,
                                r#"<p><br/></p>
                                <div class="align-end">
                                <p>{}</p>
                                </div>"#,
                                custom_command[1]
                            )
                            .unwrap();
                            actions.push((Action::InsertAtogaki, current_chapter_text.clone()));
                            debug!("Added InsertAtogaki action");
                            current_chapter_text = String::new();
                        }
                        "colophon" => {
                            actions.push((Action::InsertColophon, custom_command[1].to_string()));
                            debug!("Added InsertColophon action");
                        }
                        "toc-chapter" => {
                            continue;
                        }
                        _ => unimplemented!("Unimplemented custom command"),
                    }
                }
                None => {
                    if line.contains("----------") {
                        continue;
                    } else if line.is_empty() {
                        write!(current_chapter_text, "<p><br/></p>").unwrap();
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
            Action::InsertContentWithChapter | Action::InsertAtogaki => {
                toc_content = toc_replace_re
                    .replace(&toc_content, format!("{:03}", tmp_toc_paragraph_number))
                    .to_string();
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
            image_mime_type,
        )
        .unwrap()
        .add_content(
            EpubContent::new(
                "xhtml/p-cover.xhtml",
                generate_cover_image_xhtml(&epub_plan).as_bytes(),
            )
            .reftype(ReferenceType::Cover),
        )
        .unwrap();

    let mut current_paragraph_number: u16 = 1;
    let mut current_preface_image_number = 1;
    let mut text_inserted = false;

    for (action, action_content) in &actions {
        match action {
            Action::InsertToc => {
                epub = epub
                    .add_content(
                        EpubContent::new("xhtml/p-toc.xhtml", toc_content.as_bytes())
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
            Action::InsertContent | Action::InsertContentWithChapter | Action::InsertAtogaki => {
                let content_formatted = format!(
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
                    epub_plan.lang, epub_plan.title, action_content
                );

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
                epub = add_colophone_image(epub, &epub_plan, image_path, action_content).unwrap();
                info!("Inserted colophon image `{}`", action_content);
            }
        }
    }

    epub.generate(epub_file).unwrap();
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
    raw = right_parenthesis.replace_all(&raw, "！").to_string();
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
    let title_page_img =
        fs::read(format!("{}/{}", img_path, img_name)).expect("Could not read title page image");
    epub.add_resource(
        format!("image/{}", img_name),
        title_page_img.as_slice(),
        &epub_plan.image_mime_type,
    )
    .expect("Could not add image for title page");
    epub.add_content(
        EpubContent::new("xhtml/p-titlepage.xhtml", title_page_content.as_bytes())
            .reftype(ReferenceType::TitlePage),
    )
    .expect("Could not add content for title page");
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
    let img = fs::read(format!("{}/{}", img_path, img_name)).expect("Could not read image");
    epub.add_resource(
        format!("image/{}", img_name),
        img.as_slice(),
        &epub_plan.image_mime_type,
    )
    .expect("Could not add image");
    epub.add_content(EpubContent::new(
        format!("xhtml/p-{:03}.xhtml", paragraph_number),
        img_content.as_bytes(),
    ))
    .expect("Could not add image as content");
    Ok(epub)
}

fn add_colophone_image<'a, Z: Zip>(
    epub: &'a mut EpubBuilder<Z>,
    epub_plan: &'a EpubPlan,
    img_path: &'a str,
    img_name: &'a str,
) -> Result<&'a mut EpubBuilder<Z>, error::Error> {
    let img_content = generate_image_xhtml(epub_plan, img_name);
    let img =
        fs::read(format!("{}/{}", img_path, img_name)).expect("Could not read colophon image");
    epub.add_resource(
        format!("image/{}", img_name),
        img.as_slice(),
        &epub_plan.image_mime_type,
    )
    .expect("Could not add colophone image");
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
    let preface_img =
        fs::read(format!("{}/{}", img_path, img_name)).expect("Could not read preface page image");
    epub.add_resource(
        format!("image/{}", img_name),
        preface_img.as_slice(),
        &epub_plan.image_mime_type,
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
<div class="font-1em10">"#,
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
                    r#"<p><a href="p-REPLACE_ME.xhtml#mokuji-{:04}">{}</a></p>"#,
                    current_chapter_number, chapter_name,
                ));
                current_chapter_number += 1;
            }
        }
    }

    match atogaki_re.captures(raw) {
        Some(caps) => {
            toc.push_str(&format!(
                r#"<p><br/></p>
<div class="h-indent-1em">
<p>　<a href="p-REPLACE_ME#mokuji-{:04}">{}</a></p>
</div>"#,
                current_chapter_number,
                caps.get(1).unwrap().as_str()
            ));
        }
        None => (),
    }

    toc.push_str("</div>\n</div>\n</div>\n</body>\n</html>");
    toc
}
