// Copyright 2024 The HuggingFace Inc. team. Apache-2.0 reference algorithms;
// see licenses/siglip-NOTICE.md and docs/PORTS.md.
//! SigLIP's SentencePiece Unigram: maximum-score segmentation, not greedy longest pieces.
//! Normalization reads the pinned tokenizer's SentencePiece Darts map (including Unicode
//! rewrites). Canonicalization follows Hugging Face SiglipTokenizer; lattice scoring follows
//! transformers.js / SentencePiece. See docs/PORTS.md for upstream revisions and licences.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Default)]
struct Node {
    children: BTreeMap<u8, usize>,
    piece: Option<(u32, f64)>,
}

pub struct Unigram {
    nodes: Vec<Node>,
    unknown_score: f64,
    normalizer: CharsMap,
}

impl Unigram {
    pub fn load(path: &Path) -> Result<Self> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let value: Value = serde_json::from_str(json)?;
        let model = &value["model"];
        if model["type"] != "Unigram" || model["unk_id"] != 2 || model["byte_fallback"] == true {
            bail!("unsupported SigLIP tokenizer model");
        }
        let vocab = model["vocab"].as_array().context("missing Unigram vocabulary")?;
        let mut nodes = vec![Node::default()];
        let mut min = f64::INFINITY;
        for (id, entry) in vocab.iter().enumerate() {
            let piece = entry[0].as_str().context("invalid Unigram piece")?;
            let score = entry[1].as_f64().filter(|s| s.is_finite()).context("invalid Unigram score")?;
            min = min.min(score);
            if id <= 2 || piece.is_empty() {
                continue;
            }
            let mut node = 0;
            for b in piece.bytes() {
                node = if let Some(&child) = nodes[node].children.get(&b) {
                    child
                } else {
                    let child = nodes.len();
                    nodes.push(Node::default());
                    nodes[node].children.insert(b, child);
                    child
                };
            }
            nodes[node].piece = Some((u32::try_from(id)?, score));
        }
        if !min.is_finite() {
            bail!("empty Unigram vocabulary");
        }
        let map = value["normalizer"]["normalizers"].as_array().and_then(|v| v.iter().find_map(|n| n["precompiled_charsmap"].as_str()));
        let normalizer = map.map(CharsMap::from_base64).transpose()?.unwrap_or_default();
        Ok(Self { nodes, unknown_score: min - 10.0, normalizer })
    }

    /// EOS doubles as padding; no BOS. Reserve one position for EOS when truncating.
    pub fn encode(&self, text: &str) -> Vec<i64> {
        let text: String = text.replace('▁', " ").to_lowercase().chars().filter(|c| !c.is_ascii_punctuation()).collect();
        let text = self.normalizer.normalize(&text);
        let words: Vec<_> = text.split_whitespace().collect();
        let text = if words.is_empty() { String::new() } else { format!("▁{}", words.join("▁")) };
        let bytes = text.as_bytes();
        let mut scores = vec![f64::NEG_INFINITY; bytes.len() + 1];
        let mut previous = vec![(0, 2); bytes.len() + 1];
        scores[0] = 0.0;
        for (start, ch) in text.char_indices() {
            let mut node = 0;
            let mut single = false;
            for end in start..bytes.len() {
                let Some(&child) = self.nodes[node].children.get(&bytes[end]) else { break };
                node = child;
                if let Some((id, score)) = self.nodes[node].piece {
                    let end = end + 1;
                    single |= end == start + ch.len_utf8();
                    if scores[start] + score > scores[end] {
                        scores[end] = scores[start] + score;
                        previous[end] = (start, id);
                    }
                }
            }
            if !single {
                let end = start + ch.len_utf8();
                if scores[start] + self.unknown_score > scores[end] {
                    scores[end] = scores[start] + self.unknown_score;
                    previous[end] = (start, 2);
                }
            }
        }
        let mut ids = Vec::new();
        let mut end = bytes.len();
        while end > 0 {
            let (start, id) = previous[end];
            // SentencePiece fuses consecutive unknown characters into one token.
            if id != 2 || ids.last() != Some(&2) {
                ids.push(i64::from(id));
            }
            end = start;
        }
        ids.reverse();
        ids.truncate(63);
        ids.resize(64, 1);
        ids
    }
}

#[derive(Default)]
struct CharsMap {
    trie: Vec<u32>,
    replacements: Vec<u8>,
}

impl CharsMap {
    fn from_base64(s: &str) -> Result<Self> {
        let mut bytes = Vec::new();
        let mut bits = 0u32;
        let mut count = 0;
        for b in s.bytes().take_while(|b| *b != b'=') {
            let v = match b {
                b'A'..=b'Z' => b - b'A',
                b'a'..=b'z' => b - b'a' + 26,
                b'0'..=b'9' => b - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => bail!("invalid charsmap base64"),
            };
            bits = (bits << 6) | u32::from(v);
            count += 6;
            if count >= 8 {
                count -= 8;
                bytes.push((bits >> count) as u8);
            }
        }
        let size = u32::from_le_bytes(bytes.get(..4).context("truncated charsmap")?.try_into()?) as usize;
        if size == 0 || !size.is_multiple_of(4) || size + 4 > bytes.len() {
            bail!("invalid charsmap size");
        }
        let trie = bytes[4..4 + size].as_chunks::<4>().0.iter().map(|b| u32::from_le_bytes(*b)).collect();
        let replacements = bytes[4 + size..].to_vec();
        std::str::from_utf8(&replacements).context("invalid charsmap replacements")?;
        Ok(Self { trie, replacements })
    }

    fn prefix(&self, bytes: &[u8]) -> Option<(usize, &str)> {
        let offset = |u: u32| ((u >> 10) as usize) << ((u & (1 << 9)) >> 6);
        let mut pos = offset(*self.trie.first()?);
        let mut best = None;
        for (i, &b) in bytes.iter().enumerate() {
            if b == 0 {
                break;
            }
            pos ^= usize::from(b);
            let unit = *self.trie.get(pos)?;
            if unit & (0x8000_00ff) != u32::from(b) {
                break;
            }
            pos ^= offset(unit);
            if unit & 256 != 0 {
                let index = (*self.trie.get(pos)? & 0x7fff_ffff) as usize;
                let rest = self.replacements.get(index..)?;
                let len = rest.iter().position(|b| *b == 0)?;
                best = Some((i + 1, std::str::from_utf8(&rest[..len]).ok()?));
            }
        }
        best
    }

    fn normalize(&self, text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while !rest.is_empty() {
            if let Some((n, replacement)) = self.prefix(rest.as_bytes()).filter(|(n, _)| rest.is_char_boundary(*n)) {
                out.push_str(replacement);
                rest = &rest[n..];
            } else if let Some(c) = rest.chars().next() {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn golden_unigram_reference_ids_and_padding() {
        let t = Unigram::from_json(include_str!("../tests/fixtures/siglip-unigram.json")).unwrap();
        let golden: Value = serde_json::from_str(include_str!("../tests/fixtures/siglip-golden.json")).unwrap();
        for row in golden.as_array().unwrap() {
            assert_eq!(t.encode(row["text"].as_str().unwrap()), serde_json::from_value::<Vec<i64>>(row["ids"].clone()).unwrap(), "{}", row["text"]);
        }
        assert_eq!(t.encode(&"a ".repeat(200)).len(), 64);
        assert_eq!(t.encode(&"a ".repeat(200))[63], 1);
    }
    #[test]
    fn unigram_viterbi_beats_greedy_and_fuses_unknowns() {
        let t =
            Unigram::from_json(r#"{"model":{"type":"Unigram","unk_id":2,"vocab":[["<pad>",0],["</s>",0],["<unk>",0],["▁",-1],["ab",-9],["a",-1],["b",-1]]}}"#)
                .unwrap();
        assert_eq!(&t.encode("AB!!!")[..4], &[3, 5, 6, 1]);
        assert_eq!(&t.encode("🦄🦄")[..3], &[3, 2, 1]);
        assert!(Unigram::from_json("{}").is_err());
    }
}
