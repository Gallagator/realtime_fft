use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub fn audio_init() -> cpal::Stream {
    let host = cpal::default_host();
    let input_device = host.default_input_device().expect("No input device found!");
    let mut input_configs = input_device
        .supported_input_configs()
        .expect("Error while querying configs!");

    let supported_config = input_configs
        .next()
        .expect("No supported config!")
        .with_max_sample_rate();

    let input_stream = input_device
        .build_input_stream(
            &supported_config.into(),
            |data: &[f32], _: &cpal::InputCallbackInfo| {
                println!("data[0]: {}, datasize: {}", data[0], data.len());
            },
            |err| eprintln!("An error occurred on the audio input stream!\n{}", err),
        )
        .unwrap();

    input_stream.play().unwrap();
    loop {}
    input_stream
}
