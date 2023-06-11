use crate::{config, stt};

pub fn init() -> Result<(), ()> {
    Ok(()) // nothing to init for Vosk
}

// @TODO. Make it better somehow (more accurate or with higher sensitivity).
pub fn data_callback(frame_buffer: &[i16]) -> Option<i32> {
    // recognize & convert to sequence
    let recognized_phrase = stt::recognize(&frame_buffer, true).unwrap_or("".into());

    if !recognized_phrase.trim().is_empty() {
        info!("Vosk wake-word debug info:");
        info!("rec: {}", recognized_phrase);
        let recognized_phrases = recognized_phrase.split_whitespace();
        for phrase in recognized_phrases {
            let recognized_phrase_chars = phrase.trim().to_lowercase().chars().collect::<Vec<_>>();

            // compare
            let compare_ratio = seqdiff::ratio(&config::VOSK_FETCH_PHRASE.chars().collect::<Vec<_>>(), &recognized_phrase_chars);
            info!("og phrase: {:?}", &config::VOSK_FETCH_PHRASE);
            info!("recognized phrase: {:?}", &recognized_phrase_chars);
            info!("compare ratio: {}", compare_ratio);

            if compare_ratio >= config::VOSK_MIN_RATIO {
                info!("Phrase activated.");
                return Some(0)
            }
        }
    }

    None
}