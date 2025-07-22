use anyhow::{anyhow, Result};
use hdf5::{filters::blosc_set_nthreads, Dataset, File, Group};
use ndarray::{s, Array2, Array3};
use std::path::PathBuf;

/// HDF5Writer creates two groups (one per board) and routes events accordingly.
pub struct HDF5Writer {
    pub file: File,
    pub boards: Vec<BoardData>,
    n_channels: usize,
    n_samples: usize,
    max_events_per_board: usize,
    buffer_capacity: usize,
    subrun: usize,
    file_template: String,
    compression_level: u8,
    pub saved_events: usize,
}

impl HDF5Writer {
    pub fn new(
        filename: PathBuf,
        n_channels: usize,
        n_samples: usize,
        n_boards: usize,
        max_events_per_board: usize,
        buffer_capacity: usize,
        n_threads: u8,
        compression_level: u8,
    ) -> Result<Self> {
        let file_template = filename.to_str().unwrap().replace("_00", "_{}");
        let file = File::create(filename)?;
        // Create a scalar attribute "saved_events" and initialize to 0
        file.new_attr::<usize>().shape(()).create("saved_events")?;
        blosc_set_nthreads(n_threads);

        // Create BoardData for each board.
        let boards = Self::create_boards(
            &file,
            n_channels,
            n_samples,
            n_boards,
            max_events_per_board,
            buffer_capacity,
            compression_level,
        )?;

        Ok(Self {
            file,
            boards,
            n_channels,
            n_samples,
            max_events_per_board,
            buffer_capacity,
            subrun: 0,
            file_template,
            compression_level,
            saved_events: 0,
        })
    }

    fn create_boards(
        file: &File,
        n_channels: usize,
        n_samples: usize,
        n_boards: usize,
        max_events: usize,
        buffer_capacity: usize,
        compression_level: u8,
    ) -> Result<Vec<BoardData>> {
        let groups: Vec<Group> = (0..n_boards)
            .map(|board| file.create_group(&format!("board{}", board)))
            .collect::<Result<_, _>>()?;
        let boards: Vec<BoardData> = groups
            .iter()
            .map(|group| {
                BoardData::new(
                    group,
                    n_channels,
                    n_samples,
                    max_events,
                    buffer_capacity,
                    compression_level,
                )
            })
            .collect::<Result<_, _>>()?;
        Ok(boards)
    }

    /// Append an event for the specified board (0 or 1) along with its timestamp.
    pub fn append_event(
        &mut self,
        board: usize,
        timestamp: u64,
        waveforms: &Array2<u16>,
        trigger_id: u32,
        flag: u16,
        fail: bool,
    ) -> Result<()> {
        let result = self.boards[board].append_event(timestamp, waveforms, trigger_id, flag, fail);

        if let Err(e) = result {
            if e.to_string().contains("Maximum number of events reached") {
                self.rollover()?;
                return self.boards[board]
                    .append_event(timestamp, waveforms, trigger_id, flag, fail);
            } else {
                return Err(e);
            }
        }

        Ok(())
    }

    /// Flush any remaining buffered events for both boards.
    pub fn flush_all(&mut self) -> Result<()> {
        for board in self.boards.iter_mut() {
            board.flush()?;
        }
        // Update total saved_events after flushing
        self.saved_events = self.boards.iter().map(|b| b.current_event).sum();
        self.file
            .attr("saved_events")?
            .write_scalar(&self.saved_events)?;
        Ok(())
    }

    /// Rollover the current file:
    pub fn rollover(&mut self) -> Result<()> {
        // Retrieve the buffered events from each board (but do not flush them to disk in the current file).
        let vals: Vec<(Array2<u64>, Array3<u16>, usize)> = self
            .boards
            .iter_mut()
            .map(|board| board.take_buffer())
            .collect();

        // Flush any fully accumulated events in the buffers (if needed) so that we start fresh.
        // (You might decide to handle partially full buffers as shown below.)

        // Increment subrun.
        self.subrun += 1;
        // Build new filename using the base name and new subrun.
        // For example: run1_1.h5
        let new_filename = self
            .file_template
            .replace("_{}", &format!("_{:0>2}", self.subrun));
        let new_path = PathBuf::from(new_filename);
        // Create new file.
        let new_file = File::create(&new_path)?;
        new_file
            .new_attr::<usize>()
            .shape(())
            .create("saved_events")?;
        new_file.attr("saved_events")?.write_scalar(&0)?;
        // Create new groups and board data.
        let new_boards = Self::create_boards(
            &new_file,
            self.n_channels,
            self.n_samples,
            self.boards.len(),
            self.max_events_per_board,
            self.buffer_capacity,
            self.compression_level,
        )?;

        // Replace the current file and boards.
        self.file = new_file;
        self.boards = new_boards;

        // Write the buffered events into the new file.
        for (i, (ts, wf, count)) in vals.into_iter().enumerate() {
            if count > 0 {
                self.boards[i].append_buffer(ts, wf, count)?;
            }
        }
        // Reset and update saved_events after rollover
        self.saved_events = self.boards.iter().map(|b| b.current_event).sum();
        self.file
            .attr("saved_events")?
            .write_scalar(&self.saved_events)?;

        Ok(())
    }
}

/// Holds HDF5 datasets and buffering for one board.
pub struct BoardData {
    pub current_event: usize,
    pub max_events: usize,
    pub timestamps: Dataset,
    pub waveforms: Dataset,
    pub trigids: Dataset,
    pub flags: Dataset,
    pub fails: Dataset,
    pub buffer_capacity: usize,
    pub buffer_count: usize,
    pub ts_buffer: Array2<u64>,
    pub wf_buffer: Array3<u16>,
    pub trigid_buffer: Array2<u32>,
    pub flag_buffer: Array2<u16>,
    pub fail_buffer: Array2<bool>,
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
        compression_level: u8,
    ) -> Result<Self> {
        // Create datasets
        // For timestamps we use shape (max_events, 1) to allow writing a 1D slice later.
        let ts_shape = (max_events, 1);
        let timestamps = group
            .new_dataset::<u64>()
            .shape(ts_shape)
            .blosc_zstd(compression_level, true)
            .chunk((buffer_capacity, 1))
            .create("timestamps")?;

        let wf_shape = (max_events, n_channels, n_samples);
        let waveforms = group
            .new_dataset::<u16>()
            .shape(wf_shape)
            // Set chunking and compression if desired.
            .blosc_zstd(compression_level, true)
            .chunk((buffer_capacity, n_channels, n_samples))
            .create("waveforms")?;

        let trigid_shape = (max_events, 1);
        let trigids = group
            .new_dataset::<u32>()
            .shape(trigid_shape)
            .blosc_zstd(compression_level, true)
            .chunk((buffer_capacity, 1))
            .create("triggerids")?;

        let flags_shape = (max_events, 1);
        let flags = group
            .new_dataset::<u16>()
            .shape(flags_shape)
            .blosc_zstd(compression_level, true)
            .chunk((buffer_capacity, 1))
            .create("flags")?;

        let fail_shape = (max_events, 1);
        let fails = group
            .new_dataset::<bool>()
            .shape(fail_shape)
            .blosc_zstd(compression_level, true)
            .chunk((buffer_capacity, 1))
            .create("boardfail")?;

        // Create the in-memory buffers.
        let ts_buffer = Array2::<u64>::zeros((buffer_capacity, 1));
        let wf_buffer = Array3::<u16>::zeros((buffer_capacity, n_channels, n_samples));
        let trigid_buffer = Array2::<u32>::zeros((buffer_capacity, 1));
        let flag_buffer = Array2::<u16>::zeros((buffer_capacity, 1));
        let fail_buffer = Array2::<bool>::default((buffer_capacity, 1));

        Ok(Self {
            current_event: 0,
            max_events,
            timestamps,
            waveforms,
            trigids,
            flags,
            fails,
            buffer_capacity,
            buffer_count: 0,
            ts_buffer,
            wf_buffer,
            trigid_buffer,
            flag_buffer,
            fail_buffer,
            n_channels,
            n_samples,
        })
    }

    /// Append an event to the board’s buffers. When the buffer fills, flush it to disk.
    pub fn append_event(
        &mut self,
        timestamp: u64,
        waveforms: &Array2<u16>,
        trigger_id: u32,
        flag: u16,
        fail: bool,
    ) -> Result<()> {
        // Verify that the incoming event has the expected shape.
        let (channels, samples) = waveforms.dim();
        if channels != self.n_channels || samples != self.n_samples {
            return Err(anyhow!("Event dimensions do not match dataset dimensions",));
        }
        if self.current_event + self.buffer_count >= self.max_events {
            return Err(anyhow!("Maximum number of events reached"));
        }

        // Place the new data into the buffers.
        self.ts_buffer[[self.buffer_count, 0]] = timestamp;
        self.trigid_buffer[[self.buffer_count, 0]] = trigger_id;
        self.flag_buffer[[self.buffer_count, 0]] = flag;
        self.fail_buffer[[self.buffer_count, 0]] = fail;
        // Copy the 2D waveform event into the corresponding slice of the buffer.
        self.wf_buffer
            .slice_mut(s![self.buffer_count, .., ..])
            .assign(waveforms);
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
