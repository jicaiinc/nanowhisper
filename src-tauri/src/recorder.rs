use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use std::io::Cursor;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tauri::{AppHandle, Emitter};

pub struct RecordedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

enum Cmd {
    Stop(mpsc::Sender<RecordedAudio>),
    Cancel,
}

pub struct AudioRecorder {
    cmd_tx: Mutex<Option<mpsc::Sender<Cmd>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    is_recording: Arc<Mutex<bool>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            cmd_tx: Mutex::new(None),
            worker: Mutex::new(None),
            is_recording: Arc::new(Mutex::new(false)),
        }
    }

    pub fn is_recording(&self) -> bool {
        *self.is_recording.lock().unwrap()
    }

    pub fn start(&self, app_handle: AppHandle) -> Result<()> {
        if self.is_recording() {
            return Ok(());
        }

        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let is_recording = self.is_recording.clone();

        // Mark as recording before spawning thread to prevent double-start
        *is_recording.lock().unwrap() = true;

        // Build stream on worker thread so Stream doesn't need Send
        let worker = thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_input_device() {
                Some(d) => d,
                None => {
                    log::error!("No input device available");
                    *is_recording.lock().unwrap() = false;
                    return;
                }
            };
            let config = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Failed to get input config: {}", e);
                    *is_recording.lock().unwrap() = false;
                    return;
                }
            };
            let sample_rate = config.sample_rate().0;
            let channels = config.channels() as usize;

            let (audio_tx, audio_rx) = mpsc::channel::<Vec<f32>>();

            let stream = match config.sample_format() {
                SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), audio_tx, channels),
                SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), audio_tx, channels),
                SampleFormat::I32 => build_stream::<i32>(&device, &config.into(), audio_tx, channels),
                SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), audio_tx, channels),
                _ => {
                    log::error!("Unsupported sample format");
                    *is_recording.lock().unwrap() = false;
                    return;
                }
            };
            let stream = match stream {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Failed to build stream: {}", e);
                    *is_recording.lock().unwrap() = false;
                    return;
                }
            };
            if let Err(e) = stream.play() {
                log::error!("Failed to play stream: {}", e);
                *is_recording.lock().unwrap() = false;
                return;
            }

            let mut buffer: Vec<f32> = Vec::new();

            loop {
                // Drain audio data
                while let Ok(chunk) = audio_rx.try_recv() {
                    buffer.extend_from_slice(&chunk);

                    if buffer.len() % 512 < chunk.len() {
                        let recent = &buffer[buffer.len().saturating_sub(512)..];
                        let rms = (recent.iter().map(|s| s * s).sum::<f32>()
                            / recent.len() as f32)
                            .sqrt();
                        let _ = app_handle.emit("audio-level", rms.min(1.0));
                    }
                }

                // Check commands
                match cmd_rx.try_recv() {
                    Ok(Cmd::Stop(reply)) => {
                        *is_recording.lock().unwrap() = false;
                        let audio = RecordedAudio {
                            samples: std::mem::take(&mut buffer),
                            sample_rate,
                        };
                        let _ = reply.send(audio);
                        break;
                    }
                    Ok(Cmd::Cancel) => {
                        *is_recording.lock().unwrap() = false;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => break,
                }

                thread::sleep(std::time::Duration::from_millis(5));
            }

            drop(stream);
        });

        *self.cmd_tx.lock().unwrap() = Some(cmd_tx);
        *self.worker.lock().unwrap() = Some(worker);
        Ok(())
    }

    pub fn stop(&self) -> Result<RecordedAudio> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.send_cmd(Cmd::Stop(reply_tx));
        let audio = reply_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .context("Timeout waiting for audio data")?;
        self.join_worker();
        Ok(audio)
    }

    pub fn cancel(&self) {
        self.send_cmd(Cmd::Cancel);
        self.join_worker();
    }

    fn send_cmd(&self, cmd: Cmd) {
        if let Some(tx) = self.cmd_tx.lock().unwrap().as_ref() {
            let _ = tx.send(cmd);
        }
    }

    fn join_worker(&self) {
        if let Some(handle) = self.worker.lock().unwrap().take() {
            let _ = handle.join();
        }
        *self.cmd_tx.lock().unwrap() = None;
    }
}

fn build_stream<T: Sample + cpal::SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    tx: mpsc::Sender<Vec<f32>>,
    channels: usize,
) -> Result<cpal::Stream> {
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut mono = Vec::with_capacity(data.len() / channels);
            if channels == 1 {
                mono.extend(data.iter().map(|s| s.to_float_sample().to_sample::<f32>()));
            } else {
                for frame in data.chunks_exact(channels) {
                    let sum: f32 = frame
                        .iter()
                        .map(|s| s.to_float_sample().to_sample::<f32>())
                        .sum();
                    mono.push(sum / channels as f32);
                }
            }
            let _ = tx.send(mono);
        },
        |err| {
            log::error!("Audio stream error: {}", err);
        },
        None,
    )?;
    Ok(stream)
}

pub fn encode_wav(audio: &RecordedAudio) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        for &sample in &audio.samples {
            let s = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer.write_sample(s)?;
        }
        writer.finalize()?;
    }
    Ok(cursor.into_inner())
}
