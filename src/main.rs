mod application;
mod audio_input;
mod realtime_fft;

use std::time::{Duration, Instant};

fn main() {
    application::Application::new();
    //let stream = audio_input::InputStream::new();
    //let mut dft = realtime_fft::RealtimeFft::new(stream, Duration::from_secs_f64(0.02));
    //let sleep_time = Duration::new(1, 0) / dft.sample_rate();
    //loop {
    //    //        let now = Instant::now();
    //    dft.update();
    //    println!("{}", dft.dft().borrow()[3]);
    //    std::thread::sleep(sleep_time)
    //}
}
