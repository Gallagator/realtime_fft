use num_complex::*;
use ringbuf::{Consumer, Producer, RingBuffer};
use std::rc::Rc;
use std::cell::RefCell;
use std::f32::consts::PI;

pub trait SlidingDftSrc {
    /// Fills the sample buffer and records the time that it received the samples.
    fn fill_buffer(
        &mut self,
        sample_buffer: &mut Producer<f32>,
        fill_instant: &mut std::time::Instant,
    );
    /// Returns the sample rate of the dft source.
    fn sample_rate(&self) -> u32;
    /// Returns the maximum buffer size used by the source. This is needed
    /// to calculate the latency of the Dft. Ideally, this value will be kept
    /// small.
    fn max_buffer_size(&self) -> usize;
}

pub struct SlidingDft<T: SlidingDftSrc> {
    sample_prod: Producer<f32>,
    sample_cons: Consumer<f32>,
    newest_fill_instant: std::time::Instant,
    latency: std::time::Duration,
    sliding_dft: Rc<RefCell<Vec<Complex<f32>>>>,
    dft_src: T,
}

impl<T: SlidingDftSrc> SlidingDft<T> {
    pub fn new(dft_src: T, window_size: std::time::Duration) -> SlidingDft<T> {
        let max_buffer_size = dft_src.max_buffer_size();
        let sample_rate = dft_src.sample_rate();

        let window_length: usize = (sample_rate as f64 * window_size.as_secs_f64()) as usize;
        // Should be enough to ensure current window isn't overwritten as long
        // as the latency isn't large enough to push the window back by more than a window.
        let sample_buffer_size = window_length * 2 + max_buffer_size;
        let (mut sample_prod, sample_cons) = RingBuffer::new(sample_buffer_size).split();

        for _ in 0..sample_buffer_size {
            sample_prod.push(0.0).unwrap();
        }

        SlidingDft {
            sample_prod: sample_prod,
            sample_cons: sample_cons,
            newest_fill_instant: std::time::Instant::now(),
            latency: std::time::Duration::from_secs_f64(
                (max_buffer_size + window_length / 10) as f64 / sample_rate as f64,
            ),
            sliding_dft: Rc::new(RefCell::new(vec![Complex::<f32>::new(0.0, 0.0); window_length])),
            dft_src: dft_src,
        }
    }

    pub fn update(&mut self) {
        let window_size = self.sample_cons.len();
        let sliding_dft = &self.sliding_dft;
        self.dft_src
            .fill_buffer(&mut self.sample_prod, &mut self.newest_fill_instant);
        /* Window start time. */
        let window_start = std::time::Instant::now() - self.latency;
        debug_assert!(window_start < self.newest_fill_instant);

        // Window start in samples relative the the newest fill instant
        let window_start = ((self.newest_fill_instant - window_start).as_secs_f64()
            * self.dft_src.sample_rate() as f64) as usize;
        // Window start index. Also the number of samples the window has moved by
        let window_start = window_size - window_start;
        debug_assert!(window_start + sliding_dft.borrow().len() < self.sample_cons.len());
        debug_assert!(window_start > 0);
        debug_assert!(window_start < sliding_dft.borrow().len());

        let dft_clone = sliding_dft.clone();
        self.sample_cons.access_mut(|buf1, buf2| {
            let full_buf = [buf1, buf2].concat();
            let old = &full_buf[0..window_start];
            let new = &full_buf[window_size..(window_size + window_start)];

            debug_assert!(old.len() == new.len());
            
            let omega = 2.0 * PI / (window_size as f32);
            let mut dft_clone = dft_clone.borrow_mut();

            for k in 0..dft_clone.len() {
                for n in 0..old.len() {
                    let en = Complex::<f32>::from_polar(1.0, k as f32 * omega * n as f32);
                    dft_clone[k] = dft_clone[k] - old[n] * en + new[n] * en;
                }
                let em = Complex::<f32>::from_polar(1.0, k as f32 * omega * old.len() as f32);
                dft_clone[k] *= em;
            }
        });

        self.sample_cons.discard(window_start);
    }

    pub fn dft(&self) -> &Rc<RefCell<Vec<Complex<f32>>>> {
        &self.sliding_dft
    }
}
