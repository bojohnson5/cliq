use anyhow::Result;
use clap::Parser;
use cliq::*;
use confique::Config;
use simplelog::{format_description, ConfigBuilder, WriteLogger};
use std::fs::OpenOptions;

/// LAr DAQ program
#[derive(Parser, Debug)]
struct Args {
    /// Config file used for data acquisition
    #[arg(long, short)]
    pub config: String,
    /// Optional number of runs if indefinite isn't desired
    runs: Option<usize>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = Conf::from_file(args.config)?;

    // List of board connection strings. Add as many as needed.
    let board_urls = &config.run_settings.boards;

    // Open boards and store their handles along with an assigned board ID.
    let mut boards = Vec::new();
    for (i, url) in board_urls.iter().enumerate() {
        let dev_handle = felib_open(url)?;
        boards.push((i, dev_handle));
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("daq.log")
        .unwrap();

    let log_config = ConfigBuilder::new()
        .set_time_format_custom(format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .build();

    WriteLogger::init(simplelog::LevelFilter::Debug, log_config, log_file).unwrap();

    let mut terminal = ratatui::init();
    let config_file = args.config.clone();
    let status = Tui::new(config, boards, args.runs, config_file).run(&mut terminal);
    ratatui::restore();

    println!("\nTTFN!");
    status
}
