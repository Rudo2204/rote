use anyhow::Result;
use chrono::{Local, Utc};
use clap::{
    crate_authors, crate_description, crate_version, value_t, App, AppSettings, Arg, ArgMatches,
};
use fern::colors::{Color, ColoredLevelConfig};
use fs2::FileExt;
use log::{debug, info, LevelFilter};
use std::fs::OpenOptions;
use std::io::{stdout, Write};
use std::path::PathBuf;
use std::unreachable;

mod librote;
use librote::{epub_gen, gdrive, pdf, plan, process};

pub const PROGRAM_NAME: &str = "rote";
const MAGIC_THRESHOLD_MEAN_NUMBER: u32 = 750;

fn setup_logging(verbosity: u64, chain: bool, log_path: Option<&str>) -> Result<Option<&str>> {
    let colors_line = ColoredLevelConfig::new()
        .error(Color::Red)
        .warn(Color::Yellow)
        .info(Color::Green)
        .debug(Color::Blue)
        .trace(Color::BrightBlack); // this is the same as the background color

    let mut base_config = fern::Dispatch::new();

    base_config = match verbosity {
        0 => base_config.level(LevelFilter::Warn),
        1 => base_config.level(LevelFilter::Info),
        2 => base_config.level(LevelFilter::Debug),
        _3_or_more => base_config.level(LevelFilter::Trace),
    };

    // For stdout output we will just output local %H:%M:%S
    let stdout_config = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{date} {colored_level} > {colored_message}",
                date = Local::now().format("%H:%M:%S"),
                colored_level = format_args!(
                    "\x1B[{}m{}\x1B[0m",
                    colors_line.get_color(&record.level()).to_fg_str(),
                    record.level()
                ),
                colored_message = format_args!(
                    "\x1B[{}m{}\x1B[0m",
                    colors_line.get_color(&record.level()).to_fg_str(),
                    message
                ),
            ))
        })
        .chain(stdout());

    if chain {
        // Separate file config so we can include year, month and day (UTC) in file logs
        let log_file_path = PathBuf::from(
            shellexpand::full(log_path.unwrap())
                .expect("Could not find the correct path to log data")
                .into_owned(),
        );
        let file_config = fern::Dispatch::new()
            .format(move |out, message, record| {
                out.finish(format_args!(
                    "{date} {colored_level} {colored_target} > {colored_message}",
                    date = Utc::now().format("%Y-%m-%dT%H:%M:%SUTC"),
                    colored_level = format_args!(
                        "\x1B[{}m{}\x1B[0m",
                        colors_line.get_color(&record.level()).to_fg_str(),
                        record.level()
                    ),
                    colored_target = format_args!("\x1B[95m{}\x1B[0m", record.target()),
                    colored_message = format_args!(
                        "\x1B[{}m{}\x1B[0m",
                        colors_line.get_color(&record.level()).to_fg_str(),
                        message
                    ),
                ))
            })
            .chain(fern::log_file(log_file_path)?);

        base_config
            .chain(file_config)
            .chain(stdout_config)
            .apply()?;
    } else {
        base_config.chain(stdout_config).apply()?;
    }

    Ok(log_path)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let matches = cli_interface();
    let verbosity: u64 = matches.occurrences_of("verbose");

    let lock = matches.is_present("log");
    let log_path = if let Some(log) = matches.value_of("log") {
        setup_logging(verbosity, true, Some(log))?
    } else {
        setup_logging(verbosity, false, None)?
    };

    if lock {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(log_path.unwrap())
            .unwrap();
        file.lock_exclusive()?;
    }

    debug!("-----Logger is initialized. Starting main program!-----");

    match matches.subcommand() {
        ("plan", Some(plan_matches)) => {
            let input = plan_matches.value_of("input").unwrap();
            let image_threadhold = value_t!(plan_matches, "image-threadhold", u32)
                .unwrap_or(MAGIC_THRESHOLD_MEAN_NUMBER);
            let empty_page_threadhold =
                value_t!(plan_matches, "empty-threadhold", u32).unwrap_or(0);

            debug!(
                "image_threadhold = {}, empty_threadhold = {}",
                image_threadhold, empty_page_threadhold
            );

            let ocr_plan = plan::plan(input, image_threadhold, empty_page_threadhold)
                .expect("Could not generate a plan");

            let mut ocr_plan_file = OpenOptions::new()
                .write(true)
                .create(true)
                .open("ocr_plan.toml")
                .unwrap();
            write!(ocr_plan_file, "{}", ocr_plan)?;
            debug!("OCR plan written to `ocr_plan.toml`");
            println!("`ocr_plan.toml` file created. Now edit this file to proceed further");
        }
        ("ocr", Some(ocr_matches)) => {
            let input = ocr_matches.value_of("input").unwrap();
            let parent_id = ocr_matches.value_of("id").unwrap();
            let num_chunk = pdf::gen_pdf(input)?;
            gdrive::upload_pdf("rote_client_secret.json", parent_id, num_chunk).await?;
        }
        ("process", Some(process_matches)) => {
            let num_chunk =
                value_t!(process_matches, "input", u8).expect("Could not parse value of `input`");
            let font_size_threadhold =
                value_t!(process_matches, "font-size-threadhold", u8).unwrap_or(10);
            process::tidy(num_chunk);
            process::parse_ocr_html(num_chunk, font_size_threadhold);
        }
        ("epub", Some(epub_matches)) => {
            let plan_path = epub_matches.value_of("plan").unwrap();
            let image_path = epub_matches.value_of("input").unwrap();
            let output_path = epub_matches.value_of("output").unwrap();
            epub_gen::gen_epub(plan_path, image_path, output_path);
            info!("Finished generating epub file!");
        }
        _ => unreachable!(),
    }

    debug!("-----Everything is finished!-----");
    if lock {
        let file = OpenOptions::new()
            .write(true)
            .open(log_path.unwrap())
            .unwrap();
        file.unlock()?;
    }
    Ok(())
}

fn cli_interface() -> ArgMatches<'static> {
    App::new(PROGRAM_NAME)
        .setting(AppSettings::DisableHelpSubcommand)
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(
            Arg::with_name("log")
                .long("log")
                .takes_value(true)
                .help("Also log output to file (for debugging)"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .multiple(true)
                .help("Sets the level of debug information verbosity"),
        )
        .subcommand(
            App::new("plan")
                .about("Create a ocr plan")
                .arg(
                    Arg::with_name("input")
                        .help("Input directory")
                        .index(1)
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("image-threadhold")
                        .help("Input threadhold number for image")
                        .short("i")
                        .long("image-threadhold")
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("empty-threadhold")
                        .help("Input threadhold number for empty page")
                        .short("e")
                        .long("empty-threadhold")
                        .takes_value(true),
                ),
        )
        .subcommand(
            App::new("ocr")
                .about("Start creating pdf files, OCR them and output raw html result")
                .arg(
                    Arg::with_name("input")
                        .help("Input directory")
                        .index(1)
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("id")
                        .help("Input parent id")
                        .index(2)
                        .takes_value(true)
                        .required(true),
                ),
        )
        .subcommand(
            App::new("process")
                .about("Process and output raw text from raw html for further editing")
                .arg(
                    Arg::with_name("input")
                        .help("Input number of chunk")
                        .index(1)
                        .takes_value(true)
                        .required(true),
                )
                .arg(
                    Arg::with_name("font-size-threadhold")
                        .help("Font size threadhold, default 10")
                        .short("f")
                        .long("font-size-threadhold")
                        .takes_value(true),
                ),
        )
        .subcommand(
            App::new("epub")
                .about("Generate epub")
                .arg(
                    Arg::with_name("plan")
                        .help("Input plan file")
                        .index(1)
                        .required(true)
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("input")
                        .help("Input image path")
                        .index(2)
                        .required(true)
                        .takes_value(true),
                )
                .arg(
                    Arg::with_name("output")
                        .help("Output epub file name")
                        .index(3)
                        .required(true)
                        .takes_value(true),
                ),
        )
        .get_matches()
}
