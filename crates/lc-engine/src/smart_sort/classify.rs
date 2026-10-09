//! Pure, deterministic prototype classification. Prompt changes reuse cached image embeddings;
//! exemplars refine the text direction without training or uploading any model.

use super::store::normalized;
use super::{SortPreset, Store, Tagger};
use serde::Serialize;
use std::collections::BTreeMap;

const BACKGROUND: [&str; 4] = ["a photo", "a blurry photo", "a photo of an empty room", "a screenshot"];

#[derive(Clone, Debug, Serialize)]
pub struct Classification {
    pub key: String,
    pub scores: BTreeMap<String, f32>,
    pub assigned: Vec<String>,
    pub manual: bool,
}

pub struct Classifier {
    names: Vec<String>,
    prototypes: Vec<Option<Vec<f32>>>,
    threshold: f32,
    multi: bool,
}

fn mean(vectors: &[Vec<f32>], dim: usize) -> Result<Vec<f32>, String> {
    let mut m = vec![0.0; dim];
    for v in vectors {
        let v = normalized(v.clone(), dim)?;
        for (a, b) in m.iter_mut().zip(v) {
            *a += b;
        }
    }
    normalized(m, dim)
}

pub fn prompt(text: &str) -> String {
    let text = text.trim();
    let lower = text.to_lowercase();
    if ["a ", "an ", "the ", "photo"].iter().any(|p| lower.starts_with(p)) { text.into() } else { format!("a photo of {text}") }
}

impl Classifier {
    pub fn new(tagger: &dyn Tagger, store: &Store, preset: &SortPreset) -> Result<Self, String> {
        preset.validate()?;
        let dim = tagger.dim();
        let mut prototypes = Vec::new();
        for c in &preset.categories {
            let texts: Vec<_> = c.prompts.iter().filter(|p| !p.trim().is_empty()).map(|p| tagger.embed_text(&prompt(p))).collect::<Result<_, _>>()?;
            let mut text = if texts.is_empty() { None } else { Some(mean(&texts, dim)?) };
            // Duplicate keys must not inflate the few-shot weight.
            let keys: std::collections::BTreeSet<_> = c.exemplars.iter().collect();
            let examples: Vec<_> = keys.into_iter().filter_map(|key| store.get(tagger.model_id(), key).map(|v| v.to_vec())).collect();
            if !examples.is_empty() {
                let e = mean(&examples, dim)?;
                let gamma = examples.len().min(10) as f32 / 5.0;
                text = Some(match text {
                    Some(t) => normalized(t.iter().zip(e).map(|(t, e)| t + gamma * e).collect(), dim)?,
                    None => e,
                });
            }
            prototypes.push(text);
        }
        let background: Vec<_> = BACKGROUND.iter().map(|p| tagger.embed_text(p)).collect::<Result<_, _>>()?;
        prototypes.push(Some(mean(&background, dim)?));
        Ok(Self {
            names: preset.categories.iter().map(|c| c.name.clone()).collect(),
            prototypes,
            threshold: preset.sensitivity.threshold(),
            multi: preset.multi,
        })
    }

    pub fn classify(&self, key: &str, image: Option<&[f32]>, manual: Option<&[String]>) -> Result<Classification, String> {
        let mut scores = BTreeMap::new();
        let mut assigned = Vec::new();
        if let Some(image) = image {
            let dim = self.prototypes.iter().flatten().next().map_or(0, Vec::len);
            let image = normalized(image.to_vec(), dim)?;
            let logits: Vec<_> = self
                .prototypes
                .iter()
                .map(|t| t.as_ref().map_or(f32::NEG_INFINITY, |t| 100.0 * image.iter().zip(t).map(|(a, b)| a * b).sum::<f32>()))
                .collect();
            let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let weights: Vec<_> = logits.iter().map(|l| (l - max).exp()).collect();
            let total: f32 = weights.iter().sum();
            let probs: Vec<_> = weights.iter().map(|p| p / total).collect();
            for (name, p) in self.names.iter().zip(&probs) {
                scores.insert(name.clone(), *p);
            }
            if self.multi {
                for (name, p) in self.names.iter().zip(&probs) {
                    if *p >= self.threshold * 0.75 {
                        assigned.push(name.clone());
                    }
                }
            } else {
                // Strict > preserves the first category on ties, including ties with background.
                let mut best = 0;
                for i in 1..probs.len() {
                    if probs[i] > probs[best] {
                        best = i;
                    }
                }
                if best < self.names.len() && probs[best] >= self.threshold {
                    assigned.push(self.names[best].clone());
                }
            }
        } else {
            for name in &self.names {
                scores.insert(name.clone(), 0.0);
            }
        }
        if let Some(manual) = manual {
            for name in manual {
                if !self.names.contains(name) {
                    return Err(format!("unknown override category: {name}"));
                }
            }
            assigned = manual.to_vec();
            assigned.sort_by_key(|n| self.names.iter().position(|name| name == n));
            assigned.dedup();
        }
        Ok(Classification { key: key.into(), scores, assigned, manual: manual.is_some() })
    }
}
