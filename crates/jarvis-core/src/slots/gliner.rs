// BASED ON: gline-rs crate source code
// https://github.com/fbilhaut/gline-rs

use ndarray::Array;
use once_cell::sync::OnceCell;
use ort::value::Tensor;
use std::collections::HashMap;
use std::sync::Arc;

use crate::commands::{SlotDefinition, SlotValue};
use crate::models::gliner::GlinerModel;

static MODEL: OnceCell<Arc<GlinerModel>> = OnceCell::new();

// GLiNER defaults
const THRESHOLD: f32 = 0.3;
const MAX_WIDTH: usize = 12;
const MAX_LENGTH: usize = 512;
const MIN_CONFIDENCE: f32 = 0.4;

pub fn init_with_model(model: Arc<GlinerModel>) -> Result<(), String> {
    MODEL
        .set(model)
        .map_err(|_| "GLiNER model already initialized".to_string())?;
    info!("GLiNER slot extraction ready");
    Ok(())
}

// word splitting

struct WordToken<'a> {
    start: usize,
    end: usize,
    text: &'a str,
}

fn split_words<'a>(text: &'a str, model: &GlinerModel, limit: Option<usize>) -> Vec<WordToken<'a>> {
    let mut tokens = Vec::new();
    for m in model.splitter.find_iter(text) {
        tokens.push(WordToken {
            start: m.start(),
            end: m.end(),
            text: m.as_str(),
        });
        if let Some(lim) = limit {
            if tokens.len() >= lim {
                break;
            }
        }
    }
    tokens
}

// prompt construction
//
// GLiNER prompt format:
//   [<<ENT>>, label1_w1, label1_w2, <<ENT>>, label2_w1, ..., <<SEP>>, word1, word2, ..., wordN]

fn build_prompt(entities: &[&str], words: &[WordToken]) -> (Vec<String>, usize) {
    let mut prompt = Vec::with_capacity(entities.len() * 2 + 1 + words.len());

    for entity in entities {
        prompt.push("<<ENT>>".to_string());
        prompt.push(entity.to_string());
    }
    prompt.push("<<SEP>>".to_string());

    let entities_len = prompt.len();

    for w in words {
        prompt.push(w.text.to_string());
    }

    (prompt, entities_len)
}

// encoding

struct EncodedBatch {
    input_ids: ndarray::Array2<i64>,
    attention_masks: ndarray::Array2<i64>,
    word_masks: ndarray::Array2<i64>,
    text_lengths: ndarray::Array2<i64>,
    num_words: usize,
}

fn encode_single(
    model: &GlinerModel,
    entities: &[&str],
    words: &[WordToken],
) -> Result<EncodedBatch, String> {
    let (prompt, ent_len) = build_prompt(entities, words);
    let text_word_count = words.len();

    let mut word_encodings: Vec<Vec<u32>> = Vec::with_capacity(prompt.len());
    let mut total_tokens: usize = 2; // BOS + EOS
    let mut entity_tokens: usize = 0;

    for (pos, word) in prompt.iter().enumerate() {
        let encoding = model
            .tokenizer
            .encode(word.as_str(), false)
            .map_err(|e| format!("Tokenizer encode error: {}", e))?;
        let ids = encoding.get_ids().to_vec();
        total_tokens += ids.len();
        if pos < ent_len {
            entity_tokens += ids.len();
        }
        word_encodings.push(ids);
    }

    let text_offset = entity_tokens + 1;

    if log::log_enabled!(log::Level::Debug) {
        debug!(
            "GLiNER prompt ({} total, ent_len={}, text_offset={}):",
            prompt.len(),
            ent_len,
            text_offset
        );
        for (i, (word, enc)) in prompt.iter().zip(word_encodings.iter()).enumerate() {
            debug!(
                "  [{}]{} '{}' -> {:?}",
                i,
                if i < ent_len { " ENT" } else { " TXT" },
                word,
                enc
            );
        }
    }

    let mut input_ids = Array::zeros((1, total_tokens));
    let mut attention_masks = Array::zeros((1, total_tokens));
    let mut word_masks = Array::zeros((1, total_tokens));

    let mut idx: usize = 0;
    let mut word_id: i64 = 0;

    // BOS
    input_ids[[0, idx]] = 1;
    attention_masks[[0, idx]] = 1;
    idx += 1;

    for word_enc in word_encodings.iter() {
        for (token_idx, &token_id) in word_enc.iter().enumerate() {
            input_ids[[0, idx]] = token_id as i64;
            attention_masks[[0, idx]] = 1;
            if idx >= text_offset && token_idx == 0 {
                word_masks[[0, idx]] = word_id;
            }
            idx += 1;
        }
        if idx >= text_offset {
            word_id += 1;
        }
    }

    // EOS
    input_ids[[0, idx]] = 2;
    attention_masks[[0, idx]] = 1;

    let mut text_lengths = Array::zeros((1, 1));
    text_lengths[[0, 0]] = (text_word_count + 1) as i64;

    if log::log_enabled!(log::Level::Debug) {
        debug!("GLiNER input_ids: {:?}", input_ids.as_slice().unwrap());
        debug!("GLiNER word_masks: {:?}", word_masks.as_slice().unwrap());
        debug!("GLiNER text_lengths: {}", text_word_count);
    }

    Ok(EncodedBatch {
        input_ids,
        attention_masks,
        word_masks,
        text_lengths,
        num_words: text_word_count + 1,
    })
}

// span tensors

fn make_span_tensors(
    num_words: usize,
    max_width: usize,
) -> (ndarray::Array3<i64>, ndarray::Array2<bool>) {
    let num_spans = num_words * max_width;

    let mut span_idx = Array::zeros((1, num_spans, 2));
    let mut span_mask = Array::from_elem((1, num_spans), false);

    for start in 0..num_words {
        let remaining = num_words - start;
        let actual_max = max_width.min(remaining);
        for width in 0..actual_max {
            let dim = start * max_width + width;
            span_idx[[0, dim, 0]] = start as i64;
            span_idx[[0, dim, 1]] = (start + width) as i64;
            span_mask[[0, dim]] = true;
        }
    }

    (span_idx, span_mask)
}

// decode + greedy search

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

struct Entity {
    text: String,
    label: String,
    prob: f32,
    start: usize,
    end: usize,
}

fn decode_and_search(
    logits_data: &[f32],
    logits_shape: &[usize],
    words: &[WordToken],
    text: &str,
    entities: &[&str],
    max_width: usize,
    threshold: f32,
) -> Vec<Entity> {
    let num_tokens = words.len();

    let dim_mw = logits_shape.get(2).copied().unwrap_or(0);
    let dim_e = logits_shape.get(3).copied().unwrap_or(0);

    let mut spans: Vec<Entity> = Vec::new();

    for start in 1..=num_tokens {
        let max_end = (start + max_width).min(num_tokens + 1);
        for end in start..max_end {
            let width = end - start;
            for (class_idx, &entity_label) in entities.iter().enumerate() {
                let flat_idx = start * dim_mw * dim_e + width * dim_e + class_idx;
                if flat_idx >= logits_data.len() {
                    continue;
                }

                let raw_score = logits_data[flat_idx];
                let prob = sigmoid(raw_score);
                if prob >= threshold {
                    let w_start = start - 1;
                    let w_end = end - 1;
                    let start_offset = words[w_start].start;
                    let end_offset = words[w_end].end;
                    let span_text = text[start_offset..end_offset].to_string();
                    spans.push(Entity {
                        text: span_text,
                        label: entity_label.to_string(),
                        prob,
                        start: start_offset,
                        end: end_offset,
                    });
                }
            }
        }
    }

    spans.sort_unstable_by(|a, b| (a.start, a.end).cmp(&(b.start, b.end)));
    greedy_flat(spans)
}

// takes ownership, filters in place - no cloning
fn greedy_flat(mut spans: Vec<Entity>) -> Vec<Entity> {
    if spans.len() <= 1 {
        return spans;
    }

    let mut keep = vec![false; spans.len()];
    let mut prev = 0usize;

    for next in 1..spans.len() {
        let no_overlap =
            spans[next].start >= spans[prev].end || spans[prev].start >= spans[next].end;

        if no_overlap {
            keep[prev] = true;
            prev = next;
        } else if spans[prev].prob < spans[next].prob {
            prev = next;
        }
    }
    keep[prev] = true;

    let mut idx = 0;
    spans.retain(|_| {
        let k = keep[idx];
        idx += 1;
        k
    });
    spans
}

// public extract API

pub fn extract(
    text: &str,
    slots: &HashMap<String, SlotDefinition>,
) -> Result<HashMap<String, SlotValue>, String> {
    let model = MODEL.get().ok_or("GLiNER not initialized")?;

    let mut label_to_slots: HashMap<&str, Vec<&str>> = HashMap::new();
    for (slot_name, def) in slots {
        if !def.entity.is_empty() {
            label_to_slots
                .entry(def.entity.as_str())
                .or_default()
                .push(slot_name.as_str());
        }
    }

    if label_to_slots.is_empty() {
        return Ok(HashMap::new());
    }

    let labels: Vec<&str> = label_to_slots.keys().copied().collect();

    debug!("GLiNER extract: text='{}', labels={:?}", text, labels);

    let words = split_words(text, &model, Some(MAX_LENGTH));
    if words.is_empty() {
        return Ok(HashMap::new());
    }

    let encoded = encode_single(&model, &labels, &words)?;

    let (span_idx, span_mask) = make_span_tensors(encoded.num_words, MAX_WIDTH);

    let t_input_ids =
        Tensor::from_array(encoded.input_ids).map_err(|e| format!("tensor: {}", e))?;
    let t_attn =
        Tensor::from_array(encoded.attention_masks).map_err(|e| format!("tensor: {}", e))?;
    let t_words = Tensor::from_array(encoded.word_masks).map_err(|e| format!("tensor: {}", e))?;
    let t_lengths =
        Tensor::from_array(encoded.text_lengths).map_err(|e| format!("tensor: {}", e))?;
    let t_span_idx = Tensor::from_array(span_idx).map_err(|e| format!("tensor: {}", e))?;
    let t_span_mask = Tensor::from_array(span_mask).map_err(|e| format!("tensor: {}", e))?;

    let mut session = model.session.lock();
    let outputs = session
        .run(ort::inputs! {
            "input_ids" => t_input_ids,
            "attention_mask" => t_attn,
            "words_mask" => t_words,
            "text_lengths" => t_lengths,
            "span_idx" => t_span_idx,
            "span_mask" => t_span_mask,
        })
        .map_err(|e| format!("ort inference error: {}", e))?;

    let (shape, logits_data) = outputs["logits"]
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("Failed to extract logits: {}", e))?;

    let logits_shape: Vec<usize> = shape.iter().map(|&d| d as usize).collect();

    // debug dump - gated so sigmoid/loop don't run in release
    if log::log_enabled!(log::Level::Debug) {
        debug!(
            "GLiNER logits shape: {:?}, data len: {}",
            logits_shape,
            logits_data.len()
        );
        let max_logit = logits_data
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        debug!(
            "GLiNER max logit: {:.4}, sigmoid: {:.4}",
            max_logit,
            sigmoid(max_logit)
        );

        let num_words = logits_shape.get(1).copied().unwrap_or(0);
        let dim_mw = logits_shape.get(2).copied().unwrap_or(0);
        let dim_e = logits_shape.get(3).copied().unwrap_or(0);
        for start in 0..num_words {
            for width in 0..dim_mw.min(num_words - start) {
                for class_idx in 0..dim_e {
                    let flat_idx = start * dim_mw * dim_e + width * dim_e + class_idx;
                    if flat_idx < logits_data.len() {
                        let score = logits_data[flat_idx];
                        let prob = sigmoid(score);
                        if prob > 0.05 {
                            let end = start + width;
                            let w_start = if start < words.len() {
                                words[start].text
                            } else {
                                "?"
                            };
                            let w_end = if end < words.len() {
                                words[end].text
                            } else {
                                "?"
                            };
                            debug!(
                                "  span[{}..{}] '{}'->'{}' label={} score={:.3} prob={:.3}",
                                start,
                                end,
                                w_start,
                                w_end,
                                labels.get(class_idx).unwrap_or(&"?"),
                                score,
                                prob
                            );
                        }
                    }
                }
            }
        }
    }

    let entities = decode_and_search(
        logits_data,
        &logits_shape,
        &words,
        text,
        &labels,
        MAX_WIDTH,
        THRESHOLD,
    );

    let mut result = HashMap::new();

    for entity in &entities {
        if entity.prob < MIN_CONFIDENCE {
            continue;
        }

        debug!(
            "GLiNER entity: '{}' -> '{}' ({:.1}%)",
            entity.text,
            entity.label,
            entity.prob * 100.0
        );

        if let Some(slot_names) = label_to_slots.get(entity.label.as_str()) {
            for &slot_name in slot_names {
                if !result.contains_key(slot_name) {
                    let value = parse_slot_value(&entity.text);
                    result.insert(slot_name.to_string(), value);
                }
            }
        }
    }

    Ok(result)
}

fn parse_slot_value(text: &str) -> SlotValue {
    if let Ok(n) = text.parse::<f64>() {
        return SlotValue::Number(n);
    }
    SlotValue::Text(text.to_string())
}
