//! Pure, deterministic prototype classification. Prompt changes reuse cached image embeddings;
//! exemplars refine the text direction without training or uploading any model.

use super::store::normalized;
use super::{Scoring, SortPreset, Store, Tagger};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const BACKGROUND: [&str; 4] = ["a photo", "a blurry photo", "a photo of an empty room", "a screenshot"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Classification {
    pub key: String,
    pub scores: BTreeMap<String, f32>,
    pub assigned: Vec<String>,
    pub manual: bool,
    /// Rank by unrounded similarities: sigmoid f32 probabilities can saturate at one.
    #[serde(default)]
    pub ranked: Vec<String>,
}

pub struct Classifier {
    scoring: Scoring,
    names: Vec<String>,
    prototypes: Vec<Vec<Vec<f32>>>,
    match_all: Vec<bool>,
    threshold: f32,
    multi: bool,
    face_gates: Vec<(Option<u32>, Option<u32>)>,
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
            let mut texts = texts.into_iter().map(|t| normalized(t, dim)).collect::<Result<Vec<_>, _>>()?;
            // Duplicate keys must not inflate the few-shot weight.
            let keys: std::collections::BTreeSet<_> = c.exemplars.iter().collect();
            let examples: Vec<_> = keys.into_iter().filter_map(|key| store.get(tagger.model_id(), key).map(|v| v.to_vec())).collect();
            if !examples.is_empty() {
                let e = mean(&examples, dim)?;
                let gamma = examples.len().min(10) as f32 / 5.0;
                if texts.is_empty() {
                    texts.push(e);
                } else {
                    for t in &mut texts {
                        *t = normalized(t.iter().zip(&e).map(|(t, e)| t + gamma * e).collect(), dim)?;
                    }
                }
            }
            prototypes.push(texts);
        }
        let background: Vec<_> = BACKGROUND.iter().map(|p| tagger.embed_text(p)).collect::<Result<_, _>>()?;
        prototypes.push(vec![mean(&background, dim)?]);
        Ok(Self {
            scoring: tagger.scoring(),
            names: preset.categories.iter().map(|c| c.name.clone()).collect(),
            prototypes,
            match_all: preset.categories.iter().map(|c| c.match_all).chain([false]).collect(),
            threshold: preset.sensitivity.threshold(),
            multi: preset.multi,
            face_gates: preset.categories.iter().map(|c| (c.min_faces, c.max_faces)).collect(),
        })
    }

    pub fn classify(&self, key: &str, image: Option<&[f32]>, manual: Option<&[String]>) -> Result<Classification, String> {
        self.classify_with_faces(key, image, manual, None)
    }

    pub fn classify_with_faces(
        &self,
        key: &str,
        image: Option<&[f32]>,
        manual: Option<&[String]>,
        faces: Option<usize>,
    ) -> Result<Classification, String> {
        let mut scores = BTreeMap::new();
        let mut assigned = Vec::new();
        let mut ranked = Vec::new();
        if let Some(image) = image {
            let dim = self.prototypes.iter().flatten().next().map_or(0, Vec::len);
            let image = normalized(image.to_vec(), dim)?;
            let logits: Vec<_> = self
                .prototypes
                .iter()
                .zip(&self.match_all)
                .map(|(tags, all)| {
                    let scores = tags.iter().map(|t| image.iter().zip(t).map(|(a, b)| a * b).sum::<f32>());
                    if tags.is_empty() {
                        f32::NEG_INFINITY
                    } else if *all {
                        scores.fold(f32::INFINITY, f32::min)
                    } else {
                        scores.fold(f32::NEG_INFINITY, f32::max)
                    }
                })
                .collect();
            let mut probs: Vec<f32> = match self.scoring {
                Scoring::Clip => {
                    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                    let weights: Vec<_> = logits.iter().map(|l| ((l - max) * 100.0).exp()).collect();
                    let total: f32 = weights.iter().sum();
                    weights.iter().map(|p| p / total).collect()
                }
                Scoring::Siglip => logits.iter().map(|cos| li_seg::siglip::probability(*cos)).collect(),
            };
            if let Some(count) = faces {
                for (prob, (min, max)) in probs.iter_mut().zip(&self.face_gates) {
                    if min.is_some_and(|n| count < n as usize) || max.is_some_and(|n| count > n as usize) {
                        *prob = 0.;
                    }
                }
            }
            for (name, p) in self.names.iter().zip(&probs) {
                scores.insert(name.clone(), *p);
            }
            let mut order: Vec<usize> = (0..self.names.len()).collect();
            order.sort_by(|a, b| logits[*b].total_cmp(&logits[*a]).then_with(|| a.cmp(b)));
            ranked = order.iter().map(|i| self.names[*i].clone()).collect();
            if self.multi {
                for (name, p) in self.names.iter().zip(&probs) {
                    if *p >= self.threshold * if self.scoring == Scoring::Clip { 0.75 } else { 1.0 } {
                        assigned.push(name.clone());
                    }
                }
            } else {
                // Strict > preserves the first category on ties, including ties with background.
                let mut best = 0;
                let candidates = if self.scoring == Scoring::Siglip { self.names.len() } else { probs.len() };
                for i in 1..candidates {
                    if probs[i] > 0.0 && (probs[best] == 0.0 || logits[i] > logits[best]) {
                        best = i;
                    }
                }
                if !self.names.is_empty() && best < self.names.len() && probs[best] >= self.threshold {
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
        Ok(Classification { key: key.into(), scores, assigned, manual: manual.is_some(), ranked })
    }
}
