use anyhow::Result;
use crossterm::{
    cursor::{MoveTo, MoveToColumn, MoveToNextLine},
    execute,
    terminal::{Clear, ClearType},
};

use crate::{Conf, EventWrapper};
use std::{
    fs::DirEntry,
    io::{stdout, Write},
    path::PathBuf,
    time::Instant,
};

/// Structure representing an event coming from a board.
#[derive(Debug)]
#[allow(dead_code)]
pub struct BoardEvent {
    pub board_id: usize,
    pub event: EventWrapper,
}

/// A helper structure to track statistics.
#[derive(Clone, Copy, Debug)]
pub struct Counter {
    pub total_size: usize,
    pub n_events: usize,
    pub t_begin: Instant,
}

impl std::default::Default for Counter {
    fn default() -> Self {
        Self {
            total_size: 0,
            n_events: 0,
            t_begin: Instant::now(),
        }
    }
}

#[allow(dead_code)]
impl Counter {
    pub fn new() -> Self {
        Self {
            total_size: 0,
            n_events: 0,
            t_begin: Instant::now(),
        }
    }

    pub fn from(counter: &Self) -> Self {
        Self {
            total_size: counter.total_size,
            n_events: counter.n_events,
            t_begin: counter.t_begin,
        }
    }

    pub fn rate(&self) -> f64 {
        (self.total_size as f64) / self.t_begin.elapsed().as_secs_f64() / (1024.0 * 1024.0)
    }

    pub fn increment(&mut self, size: usize) {
        self.total_size += size;
        self.n_events += 1;
    }
}

pub fn print_status(status: &str, clear_screen: bool, move_line: bool, clear_line: bool) {
    let mut stdout = stdout();
    if clear_screen {
        execute!(stdout, Clear(ClearType::All), MoveTo(0, 0)).unwrap();
    }
    if move_line {
        execute!(stdout, MoveToNextLine(1)).unwrap();
    }
    if clear_line {
        execute!(stdout, Clear(ClearType::CurrentLine), MoveToColumn(0)).unwrap();
    }
    write!(stdout, "{}", status).unwrap();
    stdout.flush().unwrap();
}

pub fn create_run_file(config: &Conf) -> Result<(PathBuf, usize)> {
    let mut camp_dir = create_camp_dir(&config).unwrap();
    let runs: Vec<DirEntry> = std::fs::read_dir(&camp_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    let max_run = runs
        .iter()
        .filter_map(|path| {
            path.file_name()
                .to_str() // Get file name (OsStr)
                .and_then(|filename| {
                    // Ensure the filename starts with "run"
                    if let Some(stripped) = filename.strip_prefix("run") {
                        // Split at '_' and take the first part
                        let parts: Vec<&str> = stripped.split('_').collect();
                        parts.first()?.parse::<usize>().ok()
                    } else {
                        None
                    }
                })
        })
        .max();

    if let Some(max) = max_run {
        let file = format!("run{}_0.h5", max + 1);
        camp_dir.push(&file);
        Ok((camp_dir, max + 1))
    } else {
        Ok((camp_dir.join("run0_0.h5"), 0))
    }
}

pub fn create_camp_dir(config: &Conf) -> Result<PathBuf> {
    let camp_dir = format!(
        "{}/camp{}",
        config.run_settings.output_dir, config.run_settings.campaign_num
    );
    let path = PathBuf::from(camp_dir);
    if !std::fs::exists(&path).unwrap() {
        match std::fs::create_dir_all(&path) {
            Ok(_) => {
                print_status("Create campaign directory\n", false, true, false);
            }
            Err(e) => {
                print_status(
                    &format!("error creating dir: {:?}\n", e),
                    false,
                    true,
                    false,
                );
            }
        }
    }

    Ok(path)
}
