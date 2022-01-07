use crate::sliding_dft::{SlidingDftSrc, reallocate_ring_buf};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::{Consumer, Producer, RingBuffer};
use std::sync::{Arc, Mutex};
use std::time::Instant;

struct BufferInfo {
    sample_prod: Producer<f32>,
    time_of_fill: Option<Instant>,
}

struct InputStreamInner {
    stream: cpal::Stream,
    buffer_info: Arc<Mutex<BufferInfo>>,
    sample_cons: Arc<Mutex<Consumer<f32>>>,
}

pub struct InputStream {
    inner: Option<InputStreamInner>,
    sample_rate: cpal::SampleRate,
}

impl InputStream {
    pub fn new() -> InputStream {
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

        InputStream {
            inner: None,
            sample_rate,
        }
    }
}

impl SlidingDftSrc for InputStream {
    
    fn init(&mut self, sample_buffer_size: usize) {
        let (sample_prod, sample_cons) = RingBuffer::new(sample_buffer_size).split();
           
        let sample_cons = Arc::new(Mutex::new(sample_cons));
        let buffer_info = Arc::new(Mutex::new(BufferInfo {
            sample_prod: sample_prod,
            time_of_fill: None,
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

        // Share buffer info accross threads And initialise input stream.
        let buffer_info_clone = buffer_info.clone();
        let sample_cons_clone = sample_cons.clone(); 
        let input_stream = input_device
            .build_input_stream(
                &supported_config.into(),
                // Closure copies recieved samples into a buffer.
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    println!("BUFFER_FILL!");
                    let mut buf_info = buffer_info_clone.lock().unwrap();
                    buf_info.time_of_fill = Some(std::time::Instant::now());

                    if data.len() > buf_info.sample_prod.capacity() { 
                        let mut old_cons = sample_cons_clone.lock().unwrap();
                        let (new_prod, new_cons) = reallocate_ring_buf(&mut old_cons, data.len() + sample_buffer_size);
                        *old_cons = new_cons;
                        buf_info.sample_prod = new_prod;
                    }
                    buf_info.sample_prod.push_slice(data);
                },
                |err| eprintln!("An error occurred on the audio input stream!\n{}", err),
            )
            .unwrap();

        input_stream.play().unwrap();

        self.inner = Some( InputStreamInner {
            stream: input_stream,
            sample_cons: sample_cons,
            buffer_info: buffer_info,
        });
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate.0
    }

    fn fill_instant(&self) -> Option<Instant> {
        self.inner.as_ref().unwrap().buffer_info.lock().unwrap().time_of_fill
    }
    fn sample_cons(&self) -> &Arc<Mutex<Consumer<f32>>> {
        &self.inner.as_ref().unwrap().sample_cons
    }
}
