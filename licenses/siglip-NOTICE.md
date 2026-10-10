# SigLIP preprocessing and tokenizer references

The Rust implementation in `crates/li-seg/src/{siglip,unigram}.rs` follows:

- Hugging Face Transformers SigLIP tokenizer and image processor. Copyright 2024
  The HuggingFace Inc. team. Apache License 2.0; see Apache-2.0 text in this directory.
- Hugging Face transformers.js Unigram lattice and spm_precompiled's SentencePiece
  charsmap representation. Apache License 2.0. No dependency on either runtime is included.
- Pillow's bicubic coefficient and integer rounding algorithm, implemented in Rust.

The Python Imaging Library (PIL) is Copyright © 1997-2011 by Secret Labs AB;
Copyright © 1995-2011 by Fredrik Lundh and contributors.
Pillow is Copyright © 2010 by Jeffrey 'Alex' Clark and contributors.

PIL/Pillow permission notice (MIT-CMU):

By obtaining, using, and/or copying this software and/or its associated documentation,
you agree that you have read, understood, and will comply with the following terms and
conditions: Permission to use, copy, modify and distribute this software and its
documentation for any purpose and without fee is hereby granted, provided that the
above copyright notice appears in all copies, and that both that copyright notice and
this permission notice appear in supporting documentation, and that the name of Secret
Labs AB or the author not be used in advertising or publicity pertaining to distribution
of the software without specific, written prior permission.

SECRET LABS AB AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE,
INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL
SECRET LABS AB OR THE AUTHOR BE LIABLE FOR ANY SPECIAL, INDIRECT OR CONSEQUENTIAL
DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER
IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

SigLIP weights/tokenizer: Google SigLIP base-patch16-224, Apache-2.0;
Xenova ONNX export revision 4649052661e53c7000355844105f8a1792088239.
The `siglip-unigram.json` test fixture retains an excerpt of that tokenizer and its
SentencePiece normalization map under Apache-2.0.
