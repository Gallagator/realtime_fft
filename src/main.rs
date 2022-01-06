mod application;
mod audio_input;
mod sliding_dft;

fn main() {
    // application::Application::new();
    let stream = audio_input::InputStream::new();
    loop {}
}
