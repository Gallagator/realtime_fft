use crate::realtime_fft::realtime_fft_src::{LatencyInfo, RealtimeFftSrc, SrcInfo};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleRate;
use ringbuf::Consumer;
use std::sync::{Arc, Mutex};

struct InputStreamInner {
    stream: cpal::Stream,
    src_info: SrcInfo,
}

pub struct InputStream {
    inner: Option<InputStreamInner>,
    sample_rate: cpal::SampleRate,
}

const DEFAULT_SAMPLE_RATE: SampleRate = SampleRate(44100);

impl InputStream {
    pub fn new() -> InputStream {
        // Find input device and input configs.
        let host = cpal::default_host();
        let input_device = host.default_input_device().expect("No input device found!");
        let mut input_configs = input_device
            .supported_input_configs()
            .expect("Error while querying configs!");

        // Get supported config
        let supported_config = input_configs.next().expect("No supported config!");

        let sample_rate = std::cmp::max(supported_config.min_sample_rate(), DEFAULT_SAMPLE_RATE);

        InputStream {
            inner: None,
            sample_rate,
        }
    }
}

impl RealtimeFftSrc for InputStream {
    fn init(&mut self, sample_buffer_size: usize) {
        // Find input device and input configs.
        let host = cpal::default_host();
        let input_device = host.default_input_device().expect("No input device found!");
        let mut input_configs = input_device
            .supported_input_configs()
            .expect("Error while querying configs!");

        // Get supported config
        let supported_config = input_configs.next().expect("No supported config!");

        let sample_rate = std::cmp::max(supported_config.min_sample_rate(), DEFAULT_SAMPLE_RATE);

        let supported_config = supported_config.with_sample_rate(sample_rate);

        // Share buffer info accross threads And initialise input stream.
        let src_info = SrcInfo::new(sample_buffer_size);
        let mut src_info_clone = src_info.clone();

        let input_stream = input_device
            .build_input_stream(
                &supported_config.into(),
                // Closure copies recieved samples into a buffer.
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    src_info_clone.push_callback_data(data, sample_buffer_size);
                },
                |err| eprintln!("An error occurred on the audio input stream!\n{}", err),
            )
            .unwrap();

        input_stream.play().unwrap();

        self.inner = Some(InputStreamInner {
            stream: input_stream,
            src_info,
        });
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate.0
    }

    fn sample_cons(&self) -> &Arc<Mutex<Consumer<f32>>> {
        &self.inner.as_ref().unwrap().src_info.sample_cons()
    }

    fn latency_info(&self) -> &Arc<Mutex<LatencyInfo>> {
        &self.inner.as_ref().unwrap().src_info.latency_info()
    }
}


