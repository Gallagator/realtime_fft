mod application;
mod audio_input;
mod sliding_dft;

use std::time::{Duration, Instant};

fn main() {
    // application::Application::new();
    let stream = audio_input::InputStream::new();
    let mut dft = sliding_dft::SlidingDft::new(stream, Duration::from_secs_f64(0.001));
    let sleep_time = Duration::new(1, 0) / dft.sample_rate();
    loop {
        let now = Instant::now();
        dft.update();
        std::thread::sleep(sleep_time)
    }
}
