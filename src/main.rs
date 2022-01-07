mod application;
mod audio_input;
mod sliding_dft;

use std::time::Duration;

fn main() {
    // application::Application::new();
    let stream = audio_input::InputStream::new();
    let mut dft = sliding_dft::SlidingDft::new(stream, Duration::from_secs_f64(0.003));
    loop {
        dft.update();
    }
}
