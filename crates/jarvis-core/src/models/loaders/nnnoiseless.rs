// nnnoiseless - used for both noise suppression and VAD.
// each consumer needs its own DenoiseState (stateful per-stream),
// so this doesn't go through the registry. just centralizes creation.

use crate::config;
use nnnoiseless::DenoiseState;

// noise suppression instance
pub struct NnnoiselessNS {
    state: Box<DenoiseState<'static>>,
    buffer: Vec<f32>,
}

impl NnnoiselessNS {
    pub fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            buffer: Vec::with_capacity(config::NNNOISELESS_FRAME_SIZE * 2),
        }
    }

    pub fn process(&mut self, input: &[i16]) -> Vec<i16> {
        self.buffer.extend(input.iter().map(|&s| s as f32));

        let frame_size = config::NNNOISELESS_FRAME_SIZE;
        let full_frames = self.buffer.len() / frame_size;

        if full_frames == 0 {
            return input.to_vec();
        }

        let mut output: Vec<i16> = Vec::with_capacity(full_frames * frame_size);
        let mut input_frame = [0.0f32; 480];
        let mut output_frame = [0.0f32; 480];

        let consumed = full_frames * frame_size;
        for i in 0..full_frames {
            let offset = i * frame_size;
            input_frame.copy_from_slice(&self.buffer[offset..offset + frame_size]);

            let _ = self.state.process_frame(&mut output_frame, &input_frame);

            for &sample in &output_frame {
                let clamped = sample.clamp(i16::MIN as f32, i16::MAX as f32);
                output.push(clamped as i16);
            }
        }

        // keep leftover samples (single drain at the end)
        self.buffer.drain(..consumed);

        output
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

// VAD instance
pub struct NnnoiselessVAD {
    state: Box<DenoiseState<'static>>,
    buffer: Vec<f32>,
}

impl NnnoiselessVAD {
    pub fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            buffer: Vec::with_capacity(config::NNNOISELESS_FRAME_SIZE * 2),
        }
    }

    pub fn detect(&mut self, input: &[i16]) -> (bool, f32) {
        self.buffer.extend(input.iter().map(|&s| s as f32));

        let frame_size = config::NNNOISELESS_FRAME_SIZE;
        let full_frames = self.buffer.len() / frame_size;

        if full_frames == 0 {
            return (true, 0.5);
        }

        let mut total_vad = 0.0f32;
        let mut input_frame = [0.0f32; 480];
        let mut output_frame = [0.0f32; 480];

        let consumed = full_frames * frame_size;
        for i in 0..full_frames {
            let offset = i * frame_size;
            input_frame.copy_from_slice(&self.buffer[offset..offset + frame_size]);

            let vad_prob = self.state.process_frame(&mut output_frame, &input_frame);
            total_vad += vad_prob;
        }

        // single drain
        self.buffer.drain(..consumed);

        let avg_vad = total_vad / full_frames as f32;
        let is_voice = avg_vad >= config::VAD_NNNOISELESS_THRESHOLD;

        (is_voice, avg_vad)
    }

    pub fn reset(&mut self) {
        self.state = DenoiseState::new();
        self.buffer.clear();
    }
}
