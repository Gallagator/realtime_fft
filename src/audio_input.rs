use crate::sliding_dft::{LatencyInfo, SlidingDftSrc};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleRate;
use ringbuf::{Consumer, Producer, RingBuffer};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct BufferInfo {
    sample_prod: Producer<f32>,
}

struct InputStreamInner {
    stream: cpal::Stream,
    buffer_info: Arc<Mutex<BufferInfo>>,
    sample_cons: Arc<Mutex<Consumer<f32>>>,
}

pub struct InputStream {
    inner: Option<InputStreamInner>,
    sample_rate: cpal::SampleRate,
    latency_info: Arc<Mutex<LatencyInfo>>,
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
            latency_info: Arc::new(Mutex::new(
                    LatencyInfo {
                        sample_at_instant: None,
                        max_latency: None,
                    }
            )),
        }
    }
}

impl SlidingDftSrc for InputStream {
    fn init(&mut self, sample_buffer_size: usize) {
        let (sample_prod, sample_cons) = RingBuffer::new(sample_buffer_size).split();

        let sample_cons = Arc::new(Mutex::new(sample_cons));
        let buffer_info = Arc::new(Mutex::new(BufferInfo {
            sample_prod,
        }));

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
        let buffer_info_clone = buffer_info.clone();
        let sample_cons_clone = sample_cons.clone();
        let latency_info_clone = self.latency_info.clone();
        let input_stream = input_device
            .build_input_stream(
                &supported_config.into(),
                // Closure copies recieved samples into a buffer.
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mut buf_info = buffer_info_clone.lock().unwrap();

                    if data.len() > buf_info.sample_prod.capacity() {
                        let mut old_cons = sample_cons_clone.lock().unwrap();
                        let (new_prod, new_cons) =
                            reallocate_ring_buf(&mut old_cons, data.len() * 2 + sample_buffer_size);
                        *old_cons = new_cons;
                        buf_info.sample_prod = new_prod;
                    }
                    buf_info.sample_prod.push_slice(data);

                    let mut latency_info_locked = latency_info_clone.lock().unwrap();
                    let now = Instant::now();
                    latency_info_locked.max_latency = latency_info_locked.sample_at_instant.map_or(None, |(_, instant)| Some(now - instant));
                    latency_info_locked.sample_at_instant = Some((buf_info.sample_prod.len(), now));
                },
                |err| eprintln!("An error occurred on the audio input stream!\n{}", err),
            )
            .unwrap();

        input_stream.play().unwrap();

        self.inner = Some(InputStreamInner {
            stream: input_stream,
            sample_cons,
            buffer_info,
        });
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate.0
    }

    fn sample_cons(&self) -> &Arc<Mutex<Consumer<f32>>> {
        &self.inner.as_ref().unwrap().sample_cons
    }

    fn latency_info(&self) -> &Arc<Mutex<LatencyInfo>> {
        &self.latency_info
    }
}

pub fn reallocate_ring_buf<T>(
    consumer: &mut Consumer<T>,
    size: usize,
) -> (Producer<T>, Consumer<T>) {
    let (mut prod, cons) = RingBuffer::new(size).split();
    prod.move_from(consumer, None);
    (prod, cons)
}
