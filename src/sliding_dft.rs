use realfft::RealFftPlanner;
use rustfft::num_complex::Complex;
use rustfft::num_traits::Zero;
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
    /// Returns the buffer consumer
    /// only valid after call to init.
    fn sample_cons(&self) -> &Arc<Mutex<Consumer<f32>>>;
}

pub struct SlidingDft<T: SlidingDftSrc> {
    prev_window_start: Option<Instant>,
    time_left_over: Duration,
    fft_planner: Rc<RefCell<RealFftPlanner<f32>>>,
    sliding_dft: Rc<RefCell<Vec<Complex<f32>>>>,
    dft_src: T,
}

impl<T: SlidingDftSrc> SlidingDft<T> {
    pub fn new(mut dft_src: T, window_duration: Duration) -> SlidingDft<T> {
        let sample_rate = dft_src.sample_rate();

        let window_size: usize = (sample_rate as f64 * window_duration.as_secs_f64()) as usize;

        dft_src.init(window_size * 2);

        SlidingDft {
            prev_window_start: None,
            time_left_over: Duration::ZERO,
            fft_planner: Rc::new(RefCell::new(RealFftPlanner::new())),
            sliding_dft: Rc::new(RefCell::new(vec![
                Complex::<f32>::new(0.0, 0.0);
                (window_size / 2) + 1
            ])),
            dft_src: dft_src,
        }
    }

    /// Updates the value for the SDFT. Should be called in a fairly tight loop.
    /// Perhaps even in its own thread.
    pub fn update(&mut self) {
        let now = Instant::now();
        let time_elapsed = self.prev_window_start.map_or(Duration::ZERO, |instant| now - instant);
        
        let sliding_dft = self.sliding_dft.borrow();
        
        let sample_rate = self.dft_src.sample_rate();
        let window_size = (sliding_dft.len() - 1) * 2;
       
        let window_start = time_elapsed * sample_rate + self.time_left_over;
        self.time_left_over = Duration::from_nanos(window_start.subsec_nanos().into());
        
        let window_start = window_start.as_secs() as usize;

        let sample_cons_lock = self.dft_src.sample_cons();
        let mut sample_cons = sample_cons_lock.lock().unwrap();
        // Nothing yet to consume
        
        // Samples are too old here.
        sample_cons.discard(window_start);
      
        println!("window_size: {}, window_start: {}, cons_len: {}, cons_cap: {}", window_size, window_start, sample_cons.len(), sample_cons.capacity());

        if window_size > sample_cons.len() {
            println!("returning!");
            return;
        }

        self.prev_window_start = Some(now);

        // Performs sliding dft.
        let mut dft_clone = sliding_dft.clone();
        let fft_planner_clone = self.fft_planner.clone();
        sample_cons.access(|buf1, buf2| {
            let full_buf = [buf1, buf2].concat();
            let window = &full_buf[0..window_size];

            let real_to_complex = fft_planner_clone.borrow_mut().plan_fft_forward(window_size);
            // make input and output vectors
            let mut indata = real_to_complex.make_input_vec();
            
            indata[0..window_size].copy_from_slice(window);

            // Apply hanning window.

            real_to_complex.process(&mut indata, &mut dft_clone[..]).unwrap();
            println!("{}", dft_clone[10]);
            assert!((indata.len() / 2) + 1 == dft_clone.len());
        });

    }

    /// Returns the dft of the singal.
    pub fn dft(&self) -> &Rc<RefCell<Vec<Complex<f32>>>> {
        &self.sliding_dft
    }

    pub fn sample_rate(&self) -> u32 {
        self.dft_src.sample_rate()
    }
}

pub fn reallocate_ring_buf<T>(consumer: &mut Consumer<T>, size: usize) -> (Producer<T>, Consumer<T>) {
    let (mut prod, cons) = RingBuffer::new(size).split();
    prod.move_from(consumer, None);
    (prod, cons)
}

