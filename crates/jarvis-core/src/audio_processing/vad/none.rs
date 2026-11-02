// Always returns voice detected (no vad)
pub fn detect(_input: &[i16]) -> (bool, f32) {
    (true, 1.0)
}
