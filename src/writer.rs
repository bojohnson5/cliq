use anyhow::{anyhow, Result};
use hdf5::{filters::blosc_set_nthreads, Dataset, File, Group};
use ndarray::{s, Array2, Array3};
use std::path::PathBuf;

/// HDF5Writer creates two groups (one per board) and routes events accordingly.
pub struct HDF5Writer {
    pub file: File,
    pub board0: BoardData,
    pub board1: BoardData,
    n_channels: usize,
    n_samples: usize,
    max_events: usize,
    buffer_capacity: usize,
    subrun: usize,
    file_template: String,
}

impl HDF5Writer {
    pub fn new(
        filename: PathBuf,
        n_channels: usize,
        n_samples: usize,
        max_events: usize,
        buffer_capacity: usize,
    ) -> Result<Self> {
        let file_template = filename.to_str().unwrap().replace("_0", "_{}");
        let file = File::create(filename)?;
        blosc_set_nthreads(10);

        // Create BoardData for each board.
        let (board0, board1) =
            Self::create_boards(&file, n_channels, n_samples, max_events, buffer_capacity)?;

        Ok(Self {
            file,
            board0,
            board1,
            n_channels,
            n_samples,
            max_events,
            buffer_capacity,
            subrun: 0,
            file_template,
        })
    }

    fn create_boards(
        file: &File,
        n_channels: usize,
        n_samples: usize,
        max_events: usize,
        buffer_capacity: usize,
    ) -> Result<(BoardData, BoardData)> {
        let group0 = file.create_group("board0")?;
        let group1 = file.create_group("board1")?;
        let board0 = BoardData::new(&group0, n_channels, n_samples, max_events, buffer_capacity)?;
        let board1 = BoardData::new(&group1, n_channels, n_samples, max_events, buffer_capacity)?;
        Ok((board0, board1))
    }

    /// Append an event for the specified board (0 or 1) along with its timestamp.
    pub fn append_event(
        &mut self,
        board: usize,
        timestamp: u64,
        event_data: &Array2<u16>,
    ) -> Result<()> {
        let result = match board {
            0 => self.board0.append_event(timestamp, event_data),
            1 => self.board1.append_event(timestamp, event_data),
            _ => Err(anyhow!("Invalid board number")),
        };

        if let Err(e) = result {
            if e.to_string().contains("Maximum number of events reached") {
                self.rollover()?;
                return match board {
                    0 => self.board0.append_event(timestamp, event_data),
                    1 => self.board1.append_event(timestamp, event_data),
                    _ => Err(anyhow!("Invalid board number")),
                };
            } else {
                return Err(e);
            }
        }

        Ok(())
    }

    /// Flush any remaining buffered events for both boards.
    pub fn flush_all(&mut self) -> Result<()> {
        self.board0.flush()?;
        self.board1.flush()?;
        Ok(())
    }

    /// Rollover the current file:
    pub fn rollover(&mut self) -> Result<()> {
        // Retrieve the buffered events from each board (but do not flush them to disk in the current file).
        let (ts0, wf0, count0) = self.board0.take_buffer();
        let (ts1, wf1, count1) = self.board1.take_buffer();

        // Flush any fully accumulated events in the buffers (if needed) so that we start fresh.
        // (You might decide to handle partially full buffers as shown below.)

        // Increment subrun.
        self.subrun += 1;
        // Build new filename using the base name and new subrun.
        // For example: run1_1.h5
        let new_filename = self
            .file_template
            .replace("_{}", &format!("_{}", self.subrun));
        let new_path = PathBuf::from(new_filename);
        // Create new file.
        let new_file = File::create(&new_path)?;
        // Create new groups and board data.
        let (new_board0, new_board1) = Self::create_boards(
            &new_file,
            self.n_channels,
            self.n_samples,
            self.max_events,
            self.buffer_capacity,
        )?;

        // Replace the current file and boards.
        self.file = new_file;
        self.board0 = new_board0;
        self.board1 = new_board1;

        // Write the buffered events into the new file.
        if count0 > 0 {
            self.board0.append_buffer(ts0, wf0, count0)?;
        }
        if count1 > 0 {
            self.board1.append_buffer(ts1, wf1, count1)?;
        }
        Ok(())
    }
}

/// Holds HDF5 datasets and buffering for one board.
pub struct BoardData {
    pub current_event: usize,
    pub max_events: usize,
    pub timestamps: Dataset,
    pub waveforms: Dataset,
    pub buffer_capacity: usize,
    pub buffer_count: usize,
    pub ts_buffer: Array2<u64>,
    pub wf_buffer: Array3<u16>,
    pub n_channels: usize,
    pub n_samples: usize,
}

impl BoardData {
    pub fn new(
        group: &Group,
        n_channels: usize,
        n_samples: usize,
        max_events: usize,
        buffer_capacity: usize,
    ) -> Result<Self> {
        // Create datasets for timestamps and waveforms.
        // For timestamps we use shape (max_events, 1) to allow writing a 1D slice later.
        let ts_shape = (max_events, 1);
        let timestamps = group
            .new_dataset::<u64>()
            .shape(ts_shape)
            .blosc_zstd(2, true)
            .chunk((buffer_capacity, 1))
            .create("timestamps")?;

        let wf_shape = (max_events, n_channels, n_samples);
        let waveforms = group
            .new_dataset::<u16>()
            .shape(wf_shape)
            // Set chunking and compression if desired.
            .blosc_zstd(2, true)
            .chunk((buffer_capacity, n_channels, n_samples))
            .create("waveforms")?;

        // Create the in-memory buffers.
        let ts_buffer = Array2::<u64>::zeros((buffer_capacity, 1));
        let wf_buffer = Array3::<u16>::zeros((buffer_capacity, n_channels, n_samples));

        Ok(Self {
            current_event: 0,
            max_events,
            timestamps,
            waveforms,
            buffer_capacity,
            buffer_count: 0,
            ts_buffer,
            wf_buffer,
            n_channels,
            n_samples,
        })
    }

    /// Append an event to the board’s buffers. When the buffer fills, flush it to disk.
    pub fn append_event(&mut self, timestamp: u64, event_data: &Array2<u16>) -> Result<()> {
        // Verify that the incoming event has the expected shape.
        let (channels, samples) = event_data.dim();
        if channels != self.n_channels || samples != self.n_samples {
            return Err(anyhow!("Event dimensions do not match dataset dimensions",));
        }
        if self.current_event + self.buffer_count >= self.max_events {
            return Err(anyhow!("Maximum number of events reached"));
        }

        // Place the new data into the buffers.
        self.ts_buffer[[self.buffer_count, 0]] = timestamp;
        // Copy the 2D waveform event into the corresponding slice of the buffer.
        self.wf_buffer
            .slice_mut(s![self.buffer_count, .., ..])
            .assign(event_data);
        self.buffer_count += 1;

        // Flush the buffers if they've reached capacity.
        if self.buffer_count == self.buffer_capacity {
            self.flush()?;
        }

        Ok(())
    }

    /// Flush the buffered events to the HDF5 datasets.
    pub fn flush(&mut self) -> Result<()> {
        if self.buffer_count == 0 {
            return Ok(());
        }

        // Write the timestamp buffer.
        // The dataset was created with shape (max_events, 1), so we write a 2D slice.
        let ts_to_write = self
            .ts_buffer
            .slice(s![0..self.buffer_count, ..])
            .to_owned();
        self.timestamps.write_slice(
            &ts_to_write,
            (
                self.current_event..self.current_event + self.buffer_count,
                ..,
            ),
        )?;

        // Write the waveform buffer.
        let wf_to_write = self
            .wf_buffer
            .slice(s![0..self.buffer_count, .., ..])
            .to_owned();
        self.waveforms.write_slice(
            &wf_to_write,
            (
                self.current_event..self.current_event + self.buffer_count,
                ..,
                ..,
            ),
        )?;

        // Update the overall event count and reset the buffer.
        self.current_event += self.buffer_count;
        self.buffer_count = 0;
        Ok(())
    }

    /// Take the current buffered events (without flushing them to disk) and reset the buffer.
    /// Returns (timestamps, waveforms, number_of_events).
    pub fn take_buffer(&mut self) -> (Array2<u64>, Array3<u16>, usize) {
        let count = self.buffer_count;
        let ts = self.ts_buffer.slice(s![0..count, ..]).to_owned();
        let wf = self.wf_buffer.slice(s![0..count, .., ..]).to_owned();
        self.buffer_count = 0;
        (ts, wf, count)
    }

    /// Append a previously buffered set of events to the new datasets.
    /// This writes the provided arrays starting at the current event index.
    pub fn append_buffer(
        &mut self,
        ts_buffer: Array2<u64>,
        wf_buffer: Array3<u16>,
        count: usize,
    ) -> Result<()> {
        // Ensure we have enough room.
        if self.current_event + count > self.max_events {
            return Err(anyhow!(
                "Not enough space in the new file for rollover buffer"
            ));
        }
        self.timestamps.write_slice(
            &ts_buffer,
            (self.current_event..self.current_event + count, ..),
        )?;
        self.waveforms.write_slice(
            &wf_buffer,
            (self.current_event..self.current_event + count, .., ..),
        )?;
        self.current_event += count;
        Ok(())
    }
}
