use std::sync::Mutex;
use vosk::{DecodingState, Model, Recognizer};

use crate::config::VOSK_MODEL_PATH;

lazy_static! {
    static ref MODEL: Model = Model::new(VOSK_MODEL_PATH).unwrap();
}

lazy_static! {
    static ref RECOGNIZER: Mutex<Recognizer> =
        Mutex::new(Recognizer::new(&MODEL, 16000.0).unwrap());
}

pub fn init_vosk() {
    RECOGNIZER.lock().unwrap().set_max_alternatives(10);
    RECOGNIZER.lock().unwrap().set_words(true);
    RECOGNIZER.lock().unwrap().set_partial_words(true);
}

pub fn recognize(data: &[i16], include_partial: bool) -> Option<String> {
    let state = RECOGNIZER.lock().unwrap().accept_waveform(data);

    match state {
        DecodingState::Running => {
            if include_partial {
                Some(RECOGNIZER.lock().unwrap().partial_result().partial.into())
            } else {
                None
            }
        }
        DecodingState::Finalized => {
            // Result will always be multiple because we called set_max_alternatives
            Some(
                RECOGNIZER
                    .lock()
                    .unwrap()
                    .result()
                    .multiple()
                    .unwrap()
                    .alternatives
                    .first()
                    .unwrap()
                    .text
                    .into(),
            )
        }
        DecodingState::Failed => None,
    }
}

// pub fn stereo_to_mono(input_data: &[i16]) -> Vec<i16> {
//     let mut result = Vec::with_capacity(input_data.len() / 2);
//     result.extend(
//         input_data
//             .chunks_exact(2)
//             .map(|chunk| chunk[0] / 2 + chunk[1] / 2),
//     );

//     result
// }
