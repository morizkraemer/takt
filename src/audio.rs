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
    sample_rate: u32,
    error: Arc<Mutex<Option<String>>>,
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let channels = (config.channels as usize).max(1);
    let mut mono: Vec<f32> = Vec::new();
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            mono.clear();
            mono.extend(data.chunks(channels).map(|frame| {
                let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
                sum / frame.len() as f32
            }));
            if let Ok(mut buf) = buffer.lock() {
                buf.extend_from_slice(&mono);
            }
        },
        err_fn,
        None,
    )
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

        let supported = match device.default_input_config() {
            Ok(c) => c,
            Err(default_err) => device
                .supported_input_configs()
                .map_err(|e| format!("{default_err}; {e}"))?
                .next()
                .ok_or_else(|| format!("No supported input config ({default_err})"))?
                .with_max_sample_rate(),
        };

        let sample_format = supported.sample_format();
        let mut config: cpal::StreamConfig = supported.into();
        config.buffer_size = cpal::BufferSize::Default;
        let sample_rate = config.sample_rate.0;

        let error = Arc::new(Mutex::new(None));
        let err_slot = error.clone();
        let err_fn = move |err: cpal::StreamError| {
            eprintln!("audio error: {err}");
            if let Ok(mut slot) = err_slot.lock() {
                *slot = Some(format!("Audio stream error: {err}"));
            }
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, sample_buffer, err_fn),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, sample_buffer, err_fn),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, sample_buffer, err_fn),
            other => return Err(format!("Unsupported input sample format: {other:?}")),
        }
        .map_err(|e| e.to_string())?;

        use cpal::traits::StreamTrait;
        stream.play().map_err(|e| e.to_string())?;

        Ok(Self { _stream: stream, sample_rate, error })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Take a pending runtime stream error, if any.
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|mut e| e.take())
    }
}
