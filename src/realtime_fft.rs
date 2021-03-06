//! Module for computing realtime ffts given an audio source that implements
//! the RealtimeFftSrc trait.

use realfft::RealFftPlanner;
use rustfft::num_complex::Complex;
use std::cell::RefCell;
use std::ops::DerefMut;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Module for handling information about the audio souce.
pub mod realtime_fft_src {
    use ringbuf::{Consumer, Producer, RingBuffer};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// Describes the latency of the audio callback.
    pub struct LatencyInfo {
        /// The sample number and timestamp of the latest sample in the buffer.
        pub sample_at_instant: Option<(usize, Instant)>,
        /// Latency of audio callback.
        pub max_latency: Option<Duration>,
    }

    /// Trait that an audio source must implement in order to use RealtimeFft.
    pub trait RealtimeFftSrc {
        /// Fills the sample buffer and records the time that it received the samples.
        fn init(&mut self, sample_buffer_size: usize);
        /// Returns the sample rate of the dft source. Must be made available before init.
        fn sample_rate(&self) -> u32;
        /// Returns the buffer consumer
        /// Must be valid after call to init.
        fn sample_cons(&self) -> &Arc<Mutex<Consumer<f32>>>;
        /// Returns the max latency of the source (How long it takes for a callback).
        fn latency_info(&self) -> &Arc<Mutex<LatencyInfo>>;
    }

    /// Struct that contains info needed by RealtimeFft.
    #[derive(Clone)]
    pub struct SrcInfo {
        /// Samples are written to ringbuffer producer.
        sample_prod: Arc<Mutex<Producer<f32>>>,
        /// Samples are read by RealtimeFft from ringbuffer consumer.
        sample_cons: Arc<Mutex<Consumer<f32>>>,
        /// Gives information about latency of the source.
        latency_info: Arc<Mutex<LatencyInfo>>,
    }

    impl SrcInfo {
        /// Creates a new SourceInfo
        pub fn new(sample_buffer_size: usize) -> Self {
            let (sample_prod, sample_cons) = RingBuffer::new(sample_buffer_size).split();

            let sample_cons = Arc::new(Mutex::new(sample_cons));
            let sample_prod = Arc::new(Mutex::new(sample_prod));
            let latency_info = Arc::new(Mutex::new(LatencyInfo {
                sample_at_instant: None,
                max_latency: None,
            }));

            SrcInfo {
                sample_prod,
                sample_cons,
                latency_info,
            }
        }

        /// Puts sample data into the ringbuffer and updates latency information.
        pub fn push_callback_data(&mut self, data: &[f32], sample_buffer_size: usize) {
            let mut sample_prod = self.sample_prod.lock().unwrap();

            // Ringbuf is not big enough given the window len and data len.
            if data.len() + sample_buffer_size > sample_prod.capacity() {
                let mut old_cons = self.sample_cons.lock().unwrap();
                let (new_prod, new_cons) =
                    reallocate_ring_buf(&mut old_cons, data.len() + sample_buffer_size);
                *old_cons = new_cons;
                *sample_prod = new_prod;
            }
            // Unable to push entire slice. Drop earlier samples.
            let sample_prod_remaining = sample_prod.remaining();
            if data.len() > sample_prod_remaining {
                let mut sample_cons = self.sample_cons.lock().unwrap();
                sample_cons.discard(data.len() - sample_prod_remaining);
            }
            sample_prod.push_slice(data);

            // Calculate latency information
            let mut latency_info = self.latency_info.lock().unwrap();
            let prod_len = sample_prod.len();
            let now = Instant::now();
            latency_info.max_latency = latency_info
                .sample_at_instant
                .map_or(None, |(_, instant)| Some(now - instant));
            latency_info.sample_at_instant = Some((prod_len, now));
        }

        // Returns a reference to the consumer.
        pub fn sample_cons(&self) -> &Arc<Mutex<Consumer<f32>>> {
            &self.sample_cons
        }

        // Returns a reference to the latency_info.
        pub fn latency_info(&self) -> &Arc<Mutex<LatencyInfo>> {
            &self.latency_info
        }
    }

    /// Function to reallocate a ring buffer.
    fn reallocate_ring_buf<T>(
        consumer: &mut Consumer<T>,
        size: usize,
    ) -> (Producer<T>, Consumer<T>) {
        let (mut prod, cons) = RingBuffer::new(size).split();
        prod.move_from(consumer, None);
        (prod, cons)
    }
}

/// Structure for calculating a realtime fft.
pub struct RealtimeFft<T: realtime_fft_src::RealtimeFftSrc> {
    /// Used to calculate fft.
    fft_planner: Rc<RefCell<RealFftPlanner<f32>>>,
    /// Spectrum of the fft.
    /// Note: It's len is always half that of the window len because the input
    /// signal is real. As such, the values are mirrored.
    sliding_dft: Rc<RefCell<Vec<Complex<f32>>>>,
    /// Audio source implementing the RealtimeFftSrc trait.
    dft_src: T,
    /// Latency due to window length.
    latency: Duration,
}

impl<T: realtime_fft_src::RealtimeFftSrc> RealtimeFft<T> {
    /// Returns a new RealtimeFft given an audio source and a window duration.
    pub fn new(mut dft_src: T, window_duration: Duration) -> RealtimeFft<T> {
        let sample_rate = dft_src.sample_rate();

        let window_size: usize = (sample_rate as f64 * window_duration.as_secs_f64()) as usize;

        dft_src.init(window_size * 2);

        RealtimeFft {
            fft_planner: Rc::new(RefCell::new(RealFftPlanner::new())),
            sliding_dft: Rc::new(RefCell::new(vec![
                Complex::<f32>::new(0.0, 0.0);
                (window_size / 2) + 1
            ])),
            dft_src,
            latency: window_duration,
        }
    }

    /// Updates the value for the SDFT. Should be called in a fairly tight loop.
    /// Perhaps even in its own thread.
    pub fn update(&mut self) {
        let window_size = (self.sliding_dft.borrow().len() - 1) * 2;
        let latency_info_ref = self.dft_src.latency_info();

        // If Latency and sample at instant are present, calculate starting
        // sample for dft. Otherwise return.
        let window_start_sample = match latency_info_ref.lock().unwrap().deref_mut() {
            realtime_fft_src::LatencyInfo {
                sample_at_instant: Some((sample_at, sample_instant)),
                max_latency: Some(src_latency),
            } => {
                let window_end_instant = Instant::now() - *src_latency;
                let window_start_instant = window_end_instant - self.latency;

                // Latency is longer than expected.) Return and try again later.
                if window_end_instant > *sample_instant {
                    return;
                }

                // Start sample is the number of samples behind the sample at sample_instant.
                let window_start_sample = (*sample_at)
                    .checked_sub(
                        ((*sample_instant - window_start_instant) * self.dft_src.sample_rate())
                            .as_secs() as usize,
                    )
                    .unwrap_or(0);

                *sample_at -= window_start_sample;
                window_start_sample
            }
            _ => return,
        };

        self.process_fft(window_size, window_start_sample);
    }

    /// Returns the dft of the singal.
    pub fn dft(&self) -> &Rc<RefCell<Vec<Complex<f32>>>> {
        &self.sliding_dft
    }

    /// Returns sample rate of audio source.
    pub fn sample_rate(&self) -> u32 {
        self.dft_src.sample_rate()
    }

    /// Performs an fft given a window size and its start sample.
    fn process_fft(&mut self, window_size: usize, window_start_sample: usize) {
        // Acquire consumer lock.
        let sample_cons_lock = self.dft_src.sample_cons();
        let mut sample_cons = sample_cons_lock.lock().unwrap();

        //println!(
        //    "window_size: {}, window_start: {}, cons_len: {}, cons_cap: {}",
        //    window_size,
        //    window_start_sample,
        //    sample_cons.len(),
        //    sample_cons.capacity()
        //);

        // Window has moved past these samples. Discard them.
        sample_cons.discard(window_start_sample);

        // Cannot continue as there aren't enough samples.
        if window_size > sample_cons.len() {
            return;
        }

        // Performs dft.
        let mut dft_clone = self.sliding_dft.clone();
        let fft_planner_clone = self.fft_planner.clone();
        sample_cons.access(|buf1, buf2| {
            let full_buf = [buf1, buf2].concat();
            let window = &full_buf[0..window_size];

            let real_to_complex = fft_planner_clone.borrow_mut().plan_fft_forward(window_size);
            // make input and output vectors
            let mut indata = real_to_complex.make_input_vec();

            indata[0..window_size].copy_from_slice(window);

            // TODO: Apply hanning window.

            real_to_complex
                .process(&mut indata, &mut dft_clone.borrow_mut()[..])
                .unwrap();
        });
    }
}
