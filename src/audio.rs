use cpal::traits::{DeviceTrait, HostTrait};
use std::sync::{Arc, Mutex};

pub struct AudioDevice {
    pub name: String,
    pub index: usize,
}

pub fn list_input_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devices| {
            devices
                .enumerate()
                .filter_map(|(i, d)| d.name().ok().map(|name| AudioDevice { name, index: i }))
                .collect()
        })
        .unwrap_or_default()
}

pub struct AudioStream {
    _stream: cpal::Stream,
}

impl AudioStream {
    pub fn new(
        device_index: Option<usize>,
        sample_buffer: Arc<Mutex<Vec<f32>>>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();

        let device = match device_index {
            Some(idx) => host
                .input_devices()
                .map_err(|e| e.to_string())?
                .nth(idx)
                .ok_or_else(|| format!("No input device at index {idx}"))?,
            None => host.default_input_device().ok_or("No default input device")?,
        };

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(44100),
            buffer_size: cpal::BufferSize::Fixed(128),
        };

        let buffer = sample_buffer.clone();
        let err_fn = |err| eprintln!("audio error: {err}");

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut buf) = buffer.lock() {
                        buf.extend_from_slice(data);
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| e.to_string())?;

        use cpal::traits::StreamTrait;
        stream.play().map_err(|e| e.to_string())?;

        Ok(Self { _stream: stream })
    }
}
