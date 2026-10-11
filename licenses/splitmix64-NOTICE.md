# SplitMix64

The five integer mixing steps in `crates/li-ai/src/builders.rs` and the real-model evaluation
example follow Sebastiano Vigna's SplitMix64 reference. Local Image's implementation is Rust;
no C code is compiled or included in the application.

Canonical source: <https://prng.di.unimi.it/splitmix64.c>.
Pinned reference: <https://github.com/svaarala/duktape/blob/84131700dfd32d9d1063f99b5b0a900f5935a613/misc/splitmix64.c>.
The pinned reference carries this notice:

Written in 2015 by Sebastiano Vigna (vigna@acm.org)

To the extent possible under law, the author has dedicated all copyright
and related and neighboring rights to this software to the public domain
worldwide. This software is distributed without any warranty.
See <http://creativecommons.org/publicdomain/zero/1.0/>.
