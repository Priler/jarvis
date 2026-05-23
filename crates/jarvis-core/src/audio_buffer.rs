use std::collections::VecDeque;

pub struct AudioRingBuffer {
    buffer: VecDeque<Vec<i16>>,
    max_frames: usize,
}

impl AudioRingBuffer {
    // Create buffer that holds `seconds` worth of audio at given frame_size and sample_rate
    pub fn new(seconds: f32, frame_size: usize, sample_rate: usize) -> Self {
        let frames_per_second = sample_rate / frame_size;
        let max_frames = (frames_per_second as f32 * seconds) as usize;
        
        Self {
            buffer: VecDeque::with_capacity(max_frames),
            max_frames,
        }
    }
    
    // Push a frame, dropping oldest if full
    pub fn push(&mut self, frame: &[i16]) {
        if self.buffer.len() >= self.max_frames {
            self.buffer.pop_front();
        }
        self.buffer.push_back(frame.to_vec());
    }
    
    // Drain all buffered frames into a single vec
    pub fn drain_all(&mut self) -> Vec<Vec<i16>> {
        self.buffer.drain(..).collect()
    }
    
    // Get frame count
    pub fn len(&self) -> usize {
        self.buffer.len()
    }
    
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buf(seconds: f32) -> AudioRingBuffer {
        AudioRingBuffer::new(seconds, 512, 16000)
    }

    #[test]
    fn new_buffer_is_empty() {
        let buf = make_buf(1.0);
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn push_single_frame_increments_len() {
        let mut buf = make_buf(1.0);
        buf.push(&[0i16; 512]);
        assert_eq!(buf.len(), 1);
    }

    #[test]
    fn push_beyond_capacity_evicts_oldest() {
        // 1s at 16kHz/512 = 31 frames max
        let mut buf = make_buf(1.0);
        let max = buf.max_frames;

        for i in 0..=max {
            buf.push(&vec![i as i16; 512]);
        }
        // capacity not exceeded
        assert_eq!(buf.len(), max);
        // oldest frame (0) was evicted — first remaining is frame 1
        let frames = buf.drain_all();
        assert_eq!(frames[0][0], 1);
    }

    #[test]
    fn drain_all_returns_frames_in_fifo_order() {
        let mut buf = make_buf(2.0);
        buf.push(&[1i16; 512]);
        buf.push(&[2i16; 512]);
        buf.push(&[3i16; 512]);
        let frames = buf.drain_all();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0][0], 1);
        assert_eq!(frames[1][0], 2);
        assert_eq!(frames[2][0], 3);
    }

    #[test]
    fn drain_all_empties_buffer() {
        let mut buf = make_buf(1.0);
        buf.push(&[0i16; 512]);
        buf.drain_all();
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn clear_empties_buffer() {
        let mut buf = make_buf(1.0);
        buf.push(&[0i16; 512]);
        buf.push(&[0i16; 512]);
        buf.clear();
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn drain_all_on_empty_returns_empty_vec() {
        let mut buf = make_buf(1.0);
        assert!(buf.drain_all().is_empty());
    }
}