use anyhow::{anyhow, Result};
use hdf5::{Dataset, File, Group};
use ndarray::{s, Array2, Array3};

/// HDF5Writer creates two groups (one per board) and routes events accordingly.
pub struct HDF5Writer {
    pub file: File,
    pub board0: BoardData,
    pub board1: BoardData,
}

impl HDF5Writer {
    pub fn new(
        filename: &str,
        n_channels: usize,
        n_samples: usize,
        max_events: usize,
        buffer_capacity: usize,
    ) -> Result<Self> {
        let file = File::create(filename)?;

        // Create groups for each board.
        let group0 = file.create_group("board0")?;
        let group1 = file.create_group("board1")?;

        // Create BoardData for each board.
        let board0 = BoardData::new(&group0, n_channels, n_samples, max_events, buffer_capacity)?;
        let board1 = BoardData::new(&group1, n_channels, n_samples, max_events, buffer_capacity)?;

        Ok(Self {
            file,
            board0,
            board1,
        })
    }

    /// Append an event for the specified board (0 or 1) along with its timestamp.
    pub fn append_event(
        &mut self,
        board: usize,
        timestamp: u64,
        event_data: &Array2<u16>,
    ) -> Result<()> {
        match board {
            0 => self.board0.append_event(timestamp, event_data),
            1 => self.board1.append_event(timestamp, event_data),
            _ => Err(anyhow!("Invalid board number")),
        }
    }

    /// Flush any remaining buffered events for both boards.
    pub fn flush_all(&mut self) -> Result<()> {
        self.board0.flush()?;
        self.board1.flush()?;
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
            .chunk((buffer_capacity, 1))
            .create("timestamps")?;

        let wf_shape = (max_events, n_channels, n_samples);
        let waveforms = group
            .new_dataset::<u16>()
            .shape(wf_shape)
            // Set chunking and compression if desired.
            .chunk((buffer_capacity, n_channels, n_samples))
            // .deflate(3)
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
}
