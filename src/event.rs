#[repr(C)]
pub struct CEvent {
    pub timestamp: u64,
    pub timestamp_us: f64,
    pub trigger_id: u32,
    pub event_size: usize,
    // waveform is an array of pointers (one per channel)
    pub waveform: *mut *mut u16,
    // Arrays (one element per channel) filled in by the C function
    pub n_samples: *mut usize,
    pub n_allocated_samples: *mut usize,
    pub n_channels: usize,
}

/// A safe wrapper that owns the allocated memory for a CEvent.
///
/// The inner `c_event` field can be passed to the C function, while the owned
/// buffers are automatically dropped when the wrapper goes out of scope.
#[allow(dead_code)]
pub struct EventWrapper {
    pub c_event: CEvent,

    // Owned memory: the actual waveform buffers.
    waveform_buffers: Vec<Box<[u16]>>,
    // Owned slice of waveform pointers. We need to keep this alive so that
    // `c_event.waveform` (a raw pointer into it) remains valid.
    waveform_ptrs: Box<[*mut u16]>,
    // Owned memory for the per-channel arrays.
    n_samples: Box<[usize]>,
    n_allocated_samples: Box<[usize]>,
}

impl EventWrapper {
    /// Create a new EventWrapper.
    ///
    /// # Arguments
    ///
    /// * `n_channels` - Number of waveforms/channels.
    /// * `waveform_len` - Number of samples per waveform.
    pub fn new(n_channels: usize, waveform_len: usize) -> Self {
        // Allocate the individual waveform buffers.
        let mut waveform_buffers = Vec::with_capacity(n_channels);
        let mut waveform_ptrs_vec = Vec::with_capacity(n_channels);
        for _ in 0..n_channels {
            // Create a waveform buffer with the desired length.
            let mut buffer = vec![0u16; waveform_len].into_boxed_slice();
            // Get a mutable pointer to the buffer’s data.
            let ptr = buffer.as_mut_ptr();
            waveform_ptrs_vec.push(ptr);
            waveform_buffers.push(buffer);
        }
        // Box the slice of waveform pointers. This memory is owned by our wrapper.
        let mut waveform_ptrs = waveform_ptrs_vec.into_boxed_slice();

        // Allocate the arrays for n_samples and n_allocated_samples.
        let mut n_samples = vec![0usize; n_channels].into_boxed_slice();
        let mut n_allocated_samples = vec![0usize; n_channels].into_boxed_slice();

        // IMPORTANT: Use as_mut_ptr() here so that the returned pointer
        // is actually mutable.
        let waveform_ptr = waveform_ptrs.as_mut_ptr();
        let n_samples_ptr = n_samples.as_mut_ptr();
        let n_allocated_samples_ptr = n_allocated_samples.as_mut_ptr();

        // Build the C-compatible event. We obtain raw pointers from the boxes.
        let c_event = CEvent {
            timestamp: 0,
            timestamp_us: 0.0,
            trigger_id: 0,
            event_size: 0,
            waveform: waveform_ptr,
            n_samples: n_samples_ptr,
            n_allocated_samples: n_allocated_samples_ptr,
            n_channels,
        };

        Self {
            c_event,
            waveform_buffers,
            n_samples,
            n_allocated_samples,
            waveform_ptrs,
        }
    }
}
