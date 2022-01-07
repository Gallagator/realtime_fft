use num_complex::*;
use ringbuf::{Consumer, Producer, RingBuffer};
use std::cell::RefCell;
use std::f32::consts::PI;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub trait SlidingDftSrc {
    /// Fills the sample buffer and records the time that it received the samples.
    fn init( &mut self, sample_buffer_size: usize);
    /// Returns the sample rate of the dft source. Must be made available before init.
    fn sample_rate(&self) -> u32;
    /// Returns the instant that the sample buffer was most recently filled on.
    /// only valid after call to init.
    fn fill_instant(&self) -> Option<Instant>;
    /// Returns the buffer consumer
    /// only valid after call to init.
    fn sample_cons(&self) -> &Arc<Mutex<Consumer<f32>>>;
}

pub struct SlidingDft<T: SlidingDftSrc> {
    latency: Duration,
    sliding_dft: Rc<RefCell<Vec<Complex<f32>>>>,
    dft_src: T,
}

impl<T: SlidingDftSrc> SlidingDft<T> {
    pub fn new(mut dft_src: T, window_duration: Duration) -> SlidingDft<T> {
        let sample_rate = dft_src.sample_rate();

        let window_size: usize = (sample_rate as f64 * window_duration.as_secs_f64()) as usize;

        dft_src.init(window_size * 2);

        SlidingDft {
            latency: window_duration,
            sliding_dft: Rc::new(RefCell::new(vec![
                Complex::<f32>::new(0.0, 0.0);
                window_size
            ])),
            dft_src: dft_src,
        }
    }

    /// Updates the value for the SDFT. Should be called in a fairly tight loop.
    /// Perhaps even in its own thread.
    pub fn update(&mut self) {
        let sliding_dft = self.sliding_dft.borrow();
        
        let sample_rate = self.dft_src.sample_rate();
        let newest_fill_instant = self.dft_src.fill_instant();
        let window_size = sliding_dft.len();

        /* Window start time. */
        let window_start = Instant::now() - self.latency;

        // Buffer hasn't yet been filled, SDFT impossible return.
        if newest_fill_instant == None {
            return;
        }
        let newest_fill_instant = newest_fill_instant.unwrap();
                
        // Not enough samples to perform SDFT on. Latency must be increased and
        // SDFT cannot be calculated. 
        let window_len_secs = Duration::from_secs_f64(sliding_dft.len() as f64 / sample_rate as f64);
        if newest_fill_instant < window_start + window_len_secs {
            self.latency += window_start + window_len_secs - newest_fill_instant;
            return;
        }

        let sample_cons_lock = self.dft_src.sample_cons();
        let mut sample_cons = sample_cons_lock.lock().unwrap();

        // Time difference in samples between window sample and newest sample
        let window_start = ((newest_fill_instant - window_start).as_secs_f64() * sample_rate as f64) as usize;
        // Not enough samples in the buffer!
        if window_start > sample_cons.len() {
            return;
        }
        // This is calculated shitily
        let window_start = sample_cons.len() - window_start; 

        println!("window_start: {}, dft_len: {}, latency: {:?}, sample_cons len: {}", window_start, sliding_dft.len(), self.latency, sample_cons.len());
        debug_assert!(window_start > 0);
        debug_assert!(window_start < sliding_dft.len()); // TODO drop windows if this doesnt hold.
        debug_assert!(window_start + sliding_dft.len() < sample_cons.len());
    
        // Performs sliding dft.
        let mut dft_clone = sliding_dft.clone();
        sample_cons.access(|buf1, buf2| {
            let full_buf = [buf1, buf2].concat();
            let old = &full_buf[0..window_start];
            let new = &full_buf[window_size..(window_size + window_start)];

            debug_assert!(old.len() == new.len());
            
            let omega = 2.0 * PI / (window_size as f32);

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
        sample_cons.discard(window_start);
    }

    /// Returns the dft of the singal.
    pub fn dft(&self) -> &Rc<RefCell<Vec<Complex<f32>>>> {
        &self.sliding_dft
    }
}

pub fn reallocate_ring_buf<T>(consumer: &mut Consumer<T>, size: usize) -> (Producer<T>, Consumer<T>) {
    let (mut prod, cons) = RingBuffer::new(size).split();
    prod.move_from(consumer, None);
    (prod, cons)
}

