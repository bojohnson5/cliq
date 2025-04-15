use anyhow::Result;
use clap::Parser;
use confique::Config;
use rust_daq::*;

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

    let mut terminal = ratatui::init();
    let status = Status::new(config, boards, args.runs).run(&mut terminal);
    ratatui::restore();

    println!("\nTTFN!");
    status
}
