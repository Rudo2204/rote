use anyhow::Result;
use chrono::{Local, Utc};
use clap::{crate_authors, crate_description, crate_version, App, AppSettings, Arg};
use fern::colors::{Color, ColoredLevelConfig};
use fs2::FileExt;
use log::{debug, info, LevelFilter};
use std::fs::OpenOptions;
use std::io::{stdout, Write};
use std::path::PathBuf;

mod librote;
use librote::{gdrive, pdf, plan};

pub const PROGRAM_NAME: &str = "rote";

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
    let matches = App::new(PROGRAM_NAME)
        .setting(AppSettings::DisableHelpSubcommand)
        .version(crate_version!())
        .author(crate_authors!())
        .about(crate_description!())
        .arg(
            Arg::with_name("input")
                .help("Input directory")
                .index(1)
                .takes_value(true)
                .required(true),
        )
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
        .get_matches();

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

    let input = matches.value_of("input").unwrap();

    let ocr_plan = plan::plan(&input).expect("Could not generate a plan");
    let mut ocr_plan_file = OpenOptions::new()
        .write(true)
        .create(true)
        .open("ocr_plan.toml")
        .unwrap();
    write!(ocr_plan_file, "{}", ocr_plan)?;
    debug!("OCR plan written to `ocr_plan.toml`");
    println!("`ocr_plan.toml` file created. Now edit this file to proceed further");

    let num_chunk = pdf::gen_pdf(&input)?;
    let parent_id = "11qCubuAqEWvG0pu63_wHFkUWHhR7itAz";
    gdrive::upload_pdf("rote_client_secret.json", parent_id, num_chunk).await?;

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
