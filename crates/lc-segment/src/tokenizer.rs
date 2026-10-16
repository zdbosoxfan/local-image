//! SAM 3's text prompt tokens: the shared CLIP byte-pair-encoding tokenizer ([`li_seg::bpe`],
//! the one port Smart Sort also uses) at the 32-token context SAM 3's text encoder takes, padded
//! with the end token as SAM 3 expects, with the mask of real tokens.
//!
//! This file used to hold its own copy of the CLIP BPE port (Apache License 2.0, ported from
//! Hugging Face Transformers' `models/sam3`; see NOTICE). `li-seg::bpe` was generalised from it,
//! and the two produce the same tokens (li-seg's `ds_finding_bpe_preserves_prefix_…` test checks
//! that its early stop matches the old truncate-after-encode); only the padding differs, which is
//! applied here.

use std::collections::HashMap;
use std::path::Path;

use li_seg::bpe::Bpe;

use crate::{Error, Result};

pub const CONTEXT: usize = 32;
const EOS: u32 = 49407;

pub struct Tokenizer {
    bpe: Bpe,
}

impl Tokenizer {
    /// From `vocab.json` and `merges.txt` in `dir`.
    pub fn load(dir: &Path) -> Result<Self> {
        let bpe = Bpe::load(&dir.join("vocab.json"), &dir.join("merges.txt")).map_err(|e| Error::Model(format!("tokenizer: {e:#}")))?;
        Ok(Self { bpe })
    }

    pub fn new(vocab: HashMap<String, u32>, merges: &str) -> Self {
        Self { bpe: Bpe::new(vocab, merges) }
    }

    /// Token ids (start, words, end, padding) and which positions are real tokens.
    pub fn encode(&self, text: &str) -> (Vec<u32>, Vec<bool>) {
        let mut ids = self.bpe.encode(text, CONTEXT);
        // li-seg pads with 0 after the end token (never 0 itself); SAM 3 pads with the end token
        let n = ids.iter().rposition(|&v| v != 0).map_or(0, |i| i + 1);
        if let Some(pad) = ids.get_mut(n..) {
            pad.fill(EOS);
        }
        let valid = (0..CONTEXT).map(|i| i < n).collect();
        (ids, valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOS: u32 = 49406;

    #[test]
    fn merges_by_rank() {
        let vocab: HashMap<String, u32> =
            [("c", 1), ("a", 2), ("t</w>", 3), ("ca", 4), ("cat</w>", 5)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        let t = Tokenizer::new(vocab, "#version: 0.2\nc a\nca t</w>\n");
        let (ids, valid) = t.encode("  CAT ");
        assert_eq!(&ids[..3], &[BOS, 5, EOS]);
        assert_eq!(ids.len(), CONTEXT);
        assert_eq!(valid.iter().filter(|v| **v).count(), 3);
    }

    /// Golden: the shared tokenizer gives SAM 3 exactly what its own copy did — real tokens, then
    /// the end token, then end-token padding; long prompts keep the end token at the last place;
    /// a token id 0 inside the prompt is a real token, not padding.
    #[test]
    fn j08_shared_bpe_matches_sam3_padding_and_mask() {
        let vocab: HashMap<String, u32> =
            [("!</w>", 0), ("c", 1), ("a", 2), ("t</w>", 3), ("ca", 4), ("cat</w>", 5)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        let t = Tokenizer::new(vocab, "#version: 0.2\nc a\nca t</w>\n");
        let (ids, valid) = t.encode("cat !");
        let mut want = vec![BOS, 5, 0, EOS];
        want.resize(CONTEXT, EOS);
        assert_eq!(ids, want);
        assert_eq!(valid, (0..CONTEXT).map(|i| i < 4).collect::<Vec<_>>());
        // unknown pieces map to the end token (as before) and stay real tokens
        let (ids, valid) = t.encode("x");
        assert_eq!(&ids[..3], &[BOS, EOS, EOS]);
        assert_eq!(valid.iter().filter(|v| **v).count(), 3);
        // truncation: 30 words + start + end fill the context exactly
        let (ids, valid) = t.encode(&"cat ".repeat(100));
        assert_eq!(ids.len(), CONTEXT);
        assert_eq!(ids[0], BOS);
        assert!(ids[1..CONTEXT - 1].iter().all(|v| *v == 5));
        assert_eq!(ids[CONTEXT - 1], EOS);
        assert!(valid.iter().all(|v| *v));
        // empty prompt: start + end
        let (ids, valid) = t.encode("   ");
        assert_eq!(&ids[..2], &[BOS, EOS]);
        assert!(ids[2..].iter().all(|v| *v == EOS));
        assert_eq!(valid.iter().filter(|v| **v).count(), 2);
    }
}
