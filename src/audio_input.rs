use crate::sliding_dft::SlidingDftSrc;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::Producer;
use std::sync::{Arc, Mutex};

struct BufferInfo {
    buf: Vec<f32>,
    time_of_fill: std::time::Instant,
    just_filled: bool,
}

pub struct InputStream {
    stream: cpal::Stream,
    sample_rate: cpal::SampleRate,
    max_buffer_size: cpal::FrameCount,
    buffer_info: Arc<Mutex<BufferInfo>>,
}

impl InputStream {
    pub fn new() -> InputStream {
        let buffer_info = Arc::new(Mutex::new(BufferInfo {
            buf: Vec::new(),
            time_of_fill: std::time::Instant::now(),
            just_filled: false,
        }));

        // Find input device and input configs.
        let host = cpal::default_host();
        let input_device = host.default_input_device().expect("No input device found!");
        let mut input_configs = input_device
            .supported_input_configs()
            .expect("Error while querying configs!");

        // Get supported config
        let supported_config = input_configs
            .next()
            .expect("No supported config!")
            .with_max_sample_rate();

        let sample_rate = supported_config.sample_rate();
        println!("{:?}", supported_config);

        // Share buffer info accross threads And initialise input stream.
        let buffer_info_clone = buffer_info.clone();
        let input_stream = input_device
            .build_input_stream(
                &supported_config.into(),
                // Closure copies recieved samples into a buffer.
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut buf_info = buffer_info_clone.lock().unwrap();
                    buf_info.time_of_fill = std::time::Instant::now();

                    if data.len() > buf_info.buf.len() {
                        buf_info.buf.resize(data.len(), 0.0);
                    }
                    buf_info.buf[0..data.len()].copy_from_slice(data);
                    buf_info.just_filled = true;
                },
                |err| eprintln!("An error occurred on the audio input stream!\n{}", err),
            )
            .unwrap();

        input_stream.play().unwrap();
        InputStream {
            stream: input_stream,
            sample_rate: sample_rate,
            max_buffer_size: 10000,
            buffer_info: buffer_info,
        }
    }
}

impl SlidingDftSrc for InputStream {
    fn fill_buffer(
        &mut self,
        sample_buffer: &mut Producer<f32>,
        fill_instant: &mut std::time::Instant,
    ) {
    }
    fn sample_rate(&self) -> u32 {
        0
    }

    fn max_buffer_size(&self) -> usize {
        0
    }
}
