use num_complex::*;
use ringbuf::{Consumer, Producer, RingBuffer};
use std::cell::RefCell;
use std::f32::consts::PI;
use std::rc::Rc;

pub trait SlidingDftSrc {
    /// Fills the sample buffer and records the time that it received the samples.
    fn fill_buffer(
        &mut self,
        sample_buffer: &mut Producer<f32>,
        fill_instant: &mut std::time::Instant,
    );
    /// Returns the sample rate of the dft source.
    fn sample_rate(&self) -> u32;
    /// Returns the maximum buffer size used by the source. Ideally, this value
    /// will be kept small to reduce latency.
    fn max_buffer_size(&self) -> usize;
}

pub struct SlidingDft<T: SlidingDftSrc> {
    sample_prod: Producer<f32>,
    sample_cons: Consumer<f32>,
    newest_fill_instant: std::time::Instant,
    latency: std::time::Duration,
    halt_instant: Option<std::time::Instant>,
    sliding_dft: Rc<RefCell<Vec<Complex<f32>>>>,
    dft_src: T,
}

impl<T: SlidingDftSrc> SlidingDft<T> {
    pub fn new(dft_src: T, window_size: std::time::Duration) -> SlidingDft<T> {
        let max_buffer_size = dft_src.max_buffer_size();
        let sample_rate = dft_src.sample_rate();

        let window_size: usize = (sample_rate as f64 * window_size.as_secs_f64()) as usize;
        // At this point,the max buffer size is unknown so the sample buffer will
        // likely have to be reallocated. 
        let sample_buffer_size = sample_buf_size(window_size, max_buffer_size);
        let (mut sample_prod, sample_cons) = RingBuffer::new(sample_buffer_size).split();

        for _ in 0..sample_buffer_size {
            sample_prod.push(0.0).unwrap();
        }

        SlidingDft {
            sample_prod: sample_prod,
            sample_cons: sample_cons,
            newest_fill_instant: std::time::Instant::now(),
            latency: std::time::Duration::ZERO,
            halt_instant: None,
            sliding_dft: Rc::new(RefCell::new(vec![
                Complex::<f32>::new(0.0, 0.0);
                window_size
            ])),
            dft_src: dft_src,
        }
    }

    /// Updates the value for the dtf. Should be called in a fairly tight loop.
    /// Perhaps even in its own thread.
    pub fn update(&mut self) {
        let sliding_dft = &self.sliding_dft;

        let window_size = sliding_dft.borrow().len();

        let sample_buf_size = sample_buf_size(window_size, self.dft_src.max_buffer_size());
        if self.sample_cons.capacity() <  sample_buf_size{
            let (prod, cons) = reallocate_ring_buf(&mut self.sample_cons, sample_buf_size);
            self.sample_prod = prod;
            self.sample_cons = cons; 
        }

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
        debug_assert!(window_start > 0);
        debug_assert!(window_start < sliding_dft.borrow().len());

        // Not enough samples to perform SDFT on. Latency must be increased and
        // SDFT cannot be calculated. Code assumes update is called in a tight
        // loop. This is a rather primitive attemt to minimise latency.
        if window_start + sliding_dft.borrow().len() < self.sample_cons.len()
            && self.halt_instant == None
        {
            self.halt_instant = Some(std::time::Instant::now());
            return;
        } else if let Some(instant) = self.halt_instant {
            self.latency += std::time::Instant::now() - instant;
            return;
        }

        // Performs sliding dft.
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

        // Samples are old and not needed anymore.
        self.sample_cons.discard(window_start);
    }

    /// Returns the dft of the singal.
    pub fn dft(&self) -> &Rc<RefCell<Vec<Complex<f32>>>> {
        &self.sliding_dft
    }
}

fn reallocate_ring_buf<T>(consumer: &mut Consumer<T>, size: usize) -> (Producer<T>, Consumer<T>) {
    let (mut prod, cons) = RingBuffer::new(size).split();
    prod.move_from(consumer, None);
    (prod, cons)
}

// window_size * 2 assumes the SDFT will be called frequently enough that the 
// window will be consumed quickly enough. And of course, the maximum buffer must
// be able to fit inside the sample buffer.
fn sample_buf_size(window_size: usize, max_buf_size: usize) -> usize {
    window_size * 2 + max_buf_size
}
