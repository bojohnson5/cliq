# CLIQ - [C]OHERENT [L]iquid Argon [I]U DA[Q]

This application is meant to be used with CAEN VX2740 digitizers for the
COH-Ar-750 experiment. It wraps the CAEN FELib header file with Rust code
and has a simple UI to monitor data-taking. The output files are HDF5.
Below are details on how to install the code, the structure of the code
for those looking to understand it and make changes and the structure of
the HDF5 files for use in analysis.

## Installation

Make sure your system has Rust installed (the best way is via [rustup](https://rustup.rs/)).
You will then want to make sure the appropriate CAEN libraries are installed:
- CAEN VMELib
- CAEN FELib
- CAEN Dig2

Modify the first `println!` statement in the `build.rs` file to point to the correct
directory where those libraries are installed.

Typing `cargo install --path .` will install the binary to `~/.cargo/bin` and will make
it available to use anywhere.

## How to run

The `config.toml` file is an example of the only file that needs to be included to run
the program. The program is invoked as `cliq --config <config_file>` (which can also be
found by just running `cliq` or `cliq --help` and the program usage and help information
will be shown). The configuration file has different sections with notes on the available
options. The configuration file is in TOML format, see [here](https://toml.io/en/) for
its specifications. The program is designed to take the single configuration file and
loop indefinitely, creating new runs after the specified run duration in the config file.
The user can exit the program to load a new configuration file by pressing `q`. The program
automatically handles creating new runs and incrementing the run numbers appropriately.

### Run settings

General run settings such as the digitizers to use, how long runs should be, and where data
should be written to.

- `boards`: This is where you list the URLs or USB connections to the digitizer boards as an array
of strings
- `run_duration`: How long a run should last in seconds
- `output_dir`: Where the data files should be written to
- `campaign_num`: The campaign number, separate from the run number, so there can be two runs
with the same number but they will have different campaign numbers
(These next options will be moved to a separate section in the future)
- `zs_level`: What percentage of events should never be zero suppressed. This is done using a random
number generator pulling from a uniform distribution (0.0, 1.0]
- `zs_threshold`: After computing the baseline of a waveform this is the threshold level in ADC above
baseline for which to write zeros
- `zs_edge`: Specify whether the pulses are positive- or negative-going
- `zs_samples`: The number of samples to use at the beginning of the waveform to compute the baseline

### Board settings

This is comprised of different sections. The first, `common`, are settings common to each digitizer while
an array of tables of `boards` specifies the settings for each individual board. The sections are
delineated by `[board_settings.common]` and as many `[[board_settings.boards]]` sections as you need.

#### Common

- `record_len`: The waveform length in number of samples
- `pre_trig_len`: The number of samples to take before the trigger

#### Boards

- `en_chans`: Either "true", or an array of numbers specifying which channels to enable, basically if the
self trigger should be on
- `trig_source`: A string that specifies which trigger sources the board should be trigger on
- `io_level`: "TTL" or "NIM"
- `test_pulse_*`: Test pulse parameters to use when "TestPulse" is selected as a trigger source
- `dc_offset`: Can be a single number for each channel or a "map" where each channel has its own DC offset
- `trig_thr`: What threshold to use for internal triggering
- `trig_thr_mode`: "Relative" or "Absolute"
- `trig_edge`: "Fall" or "Rise"
- `samples_over_thr`: Number of samples of threshold to self-trigger
- `itl_*`: The various parameters related to ITL logic

### Sync settings

These options facilitate syncing the clocks and/or triggers of multiple boards. Similar to `[[board_settings.boards]]`
above the settings for each board is done by `[[sync_settings.boards]]` with the following options

- `clock_src`: Digitizer clock source
- `sync_out`: Sync signal to send out
- `start_src`: What to use as the start run source
- `clock_out_fp`: Whether to enable the clock out of the front panel of the digitizer
- `trig_out`: What signal to send on the trigger out
- `auto_disarm`: Whether to enable auto-disarm acquisition when the run stops

## Code structure

For those looking to work on or modify the codebase need to know a little bit about Rust. Good sources of
information for learning Rust are [here](https://www.rust-lang.org/learn). This section will outline
the major parts of the code and how they work along with Rust libraries to be familiar with to understand
how things work. It is also important to understand how the CAEN FELib code works and the particular
firmware (in this case SCOPE) options to understand this program.

### Outline

#### lib.rs

This is the entry point of the library code for use in `main.rs` and has things like the event format
string used to configure the endpoint of the digitizers.

#### main.rs

The `main` function in `src/main.rs` is the entry point to the program. Parsing the commandline arguments
and the configuration file happens here. There is also a log file created `daq.log` which is used to
log what happens when during the running of the program, just high level information. The configuration
file is parsed with the [`confique`](https://docs.rs/confique/latest/confique/) library. Logging uses the
[`simplelog`](https://docs.rs/simplelog/latest/simplelog/) and the macros from [`log`](https://docs.rs/log/latest/log/).
Command line parsing is done with [`clap`](https://docs.rs/clap/latest/clap/).

The bulk of the program runs inside TUI code using the [`ratatui`](https://docs.rs/ratatui/latest/ratatui/)
library. Once the TUI begins to run that's all there is to `main.rs`.

#### tui.rs

This is where the bulk of the logic of the program happens. The TUI holds the state of the program like run
number, the configuration options and when the user presses the exit key. These items can be found in the
`Tui` struct. The `run` method on the `Tui` struct resets and configures the digitizers according to the config file
at the beginning of each run and then draws the state of the program to the terminal. It will continue to loop
and create new runs after the specified run time until the user presses `q` to quit the program. The `run` function
will call the `begin_run` method which spawns a thread for each digitizer to take data and another thread
to process those events. The data-taking threads are pretty simple in that they just loop indefinitely until
a stop signal is received by the digitizer. Events are sent to the event processing thread via a
[`crossbeam_channel`](https://docs.rs/crossbeam-channel/latest/crossbeam_channel/). It should be noted that
because this is a multithreaded program understanding synchronization primitives and programming is important.
Things like atomic operations and mutexes are used to share state across threads. I find this [part](https://doc.rust-lang.org/book/ch16-00-concurrency.html) of
the Rust Book to be helpful in getting a grasp on these topics.

The `event_processing` thread receives events from each board and places them in a queue, one for each board.
This function will ensure the events are aligned, meaning event numbers are the same in each queue, before
writing the events to disk. It will also create a new `HDF5Writer` struct which handles all the file creation
and disk-writing. Zero suppression also happens here. Because the waveforms are read to 2D [`ndarray`](https://docs.rs/ndarray/latest/ndarray/)
structs they can be processed using parallel iterators. A random number is also rolled each time an event is received
from a data-taking thread to determine if it should or shouldn't be zero suppressed (see [here](#run-settings) for
the options to configure this).

#### writer.rs

This is where the `HDF5Writer` struct is defined. It will create a file according to the current run number. It
will create a buffer to hold 50 events before flushing them to the created file. If a file holds
more than 7500 events from each board it will roll the file over and create a new one, with the same run number
but appending a sub-run number, i.e. `_0` -> `_1`. This struct also has a settable number of threads for
compression and a settable compression level as seen in the configuration file example.

#### config.rs

This is where the configuration file format is defined.

#### event.rs

This is where the data read out by the digitizers is defined and how that data is then wrapped in
Rust owned structs. The `CEvent` struct has the underlying data and pointers while the `EventWrapper`
struct exposes that `CEvent` and `waveform_data` as the 2D `ndarray`.

#### felib.rs

Wrappers for the `FElib.h` C code.

#### utils.rs

Various utility functions and structs such as an event counter for printing stats in the TUI and
functions to configure the digitizers

## Output file format

The output file format for data is HDF5, chosen because that Rust library had the most support and
features (like file compression) when compared to the Rust `oxyroot` library that reads and writes
ROOT files. It does still offer good library support for analysis, like in Python, and has the similar
ability as ROOT to only read in certain amounts of data from disk rather than all the file at once.
Currently the structure of the output files are
- `/`: Root of file
  - `/board{id}`: Data relating to board with ID
    - `/board{id}/timestamps`: Timestamps of events in ns
    - `/board{id}/waveforms`: Waveforms from board as 2D array, always has 64 channels (rows)
    with `record_len` samples (columns)
    - `/board{id}/triggerids`: Trigger IDs of events
    - `/board{id}/flags`: A 16 bit number specifying error flags, see ![image](error_flags.png) for the
    corresponding errors
    - `/board{id}/boardfail`: Whether the board was in a fail state when the event was read
