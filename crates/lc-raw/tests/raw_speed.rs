//! Complete output fingerprints from the serial kernels before the speed changes.
//! Original fingerprints were recorded on x86_64 Linux; worker comparisons run everywhere.
use lightcraft_raw::highlight::{Recovery, SegmentationOptions, segmentation};
use lightcraft_raw::{Cfa, Method, Normalized};
use rayon::{ThreadPool, ThreadPoolBuilder};

fn pool(threads: usize) -> ThreadPool {
    ThreadPoolBuilder::new().num_threads(threads).build().unwrap()
}
fn fingerprint(values: impl IntoIterator<Item = f32>) -> u64 {
    values.into_iter().fold(0xcbf29ce484222325, |h, v| (h ^ u64::from(v.to_bits())).wrapping_mul(0x100000001b3))
}
fn assert_original_fingerprint(actual: u64, expected: u64, label: &str) {
    // Transcendental library functions can round differently on other platforms.
    // Preserve the recorded host's strict baseline and compare worker bits on all hosts.
    if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        assert_eq!(actual, expected, "original scalar: {label}");
    }
}
fn sensor(w: usize, h: usize, cfa: Cfa, highlights: bool) -> Normalized {
    let data = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let noise = ((i as u32).wrapping_mul(1664525).wrapping_add(1013904223) >> 8 & 65535) as f32 / 65536.0;
            if highlights {
                let (dx, dy) = (x as isize - w as isize / 2, y as isize - h as isize / 2);
                if dx * dx + dy * dy < (w.min(h) as isize / 4).pow(2) || (x * 13 + y * 7) % 331 == 0 {
                    1.06
                } else {
                    ((0.22 + 0.005 * x as f32 + 0.001 * y as f32) * [1.4, 1.0, 0.8][cfa.color_at(x, y) as usize]).min(1.0)
                }
            } else {
                // Sharp noise, finite negatives/HDR and signed zero, including the tile seams.
                match i % 97 {
                    0 => -0.0,
                    1 => -0.125,
                    2 => 1.25,
                    _ => noise,
                }
            }
        })
        .collect();
    Normalized { width: w, height: h, cpp: 1, data, cfa: Some(cfa) }
}
fn same_bits(a: impl IntoIterator<Item = f32>, b: impl IntoIterator<Item = f32>, label: &str) {
    let (mut a, mut b) = (a.into_iter(), b.into_iter());
    let mut i = 0;
    loop {
        match (a.next(), b.next()) {
            (None, None) => break,
            (Some(a), Some(b)) => assert_eq!(a.to_bits(), b.to_bits(), "{label}: sample {i}"),
            _ => panic!("{label}: lengths differ at sample {i}"),
        }
        i += 1;
    }
}

#[test]
fn demosaic_serial_bits_and_parallel_workers() {
    let (serial, parallel) = (pool(1), pool(8));
    let mut expected = DEMOSAIC_BASELINE.iter();
    for (w, h) in [(7, 9), (33, 35), (127, 129), (128, 128), (129, 131), (255, 251), (257, 259), (385, 257)] {
        for pat in ["RGGB", "BGGR", "GRBG", "GBRG"] {
            let n = sensor(w, h, Cfa::bayer(pat).unwrap(), false);
            for method in [Method::Amaze, Method::Vng4, Method::DualRcdVng, Method::DualAmazeVng] {
                let a = serial.install(|| lightcraft_raw::demosaic(&n, method));
                let b = parallel.install(|| lightcraft_raw::demosaic(&n, method));
                let label = format!("{w}x{h} {pat} {method:?}");
                same_bits(a.data.iter().flatten().copied(), b.data.iter().flatten().copied(), &label);
                assert_original_fingerprint(fingerprint(a.data.iter().flatten().copied()), *expected.next().unwrap(), &label);
            }
        }
    }
}

#[test]
fn segmentation_serial_bits_and_parallel_workers() {
    let (serial, parallel) = (pool(1), pool(8));
    let mut expected = SEGMENTATION_BASELINE.iter();
    for (w, h) in [(3, 3), (63, 61), (192, 180), (257, 259), (385, 257)] {
        for pat in ["RGGB", "BGGR", "GRBG", "GBRG", "XTRANS"] {
            let cfa = if pat == "XTRANS" { Cfa::xtrans().shifted(1, 3) } else { Cfa::bayer(pat).unwrap() };
            for (mode, recovery) in [
                Recovery::Off,
                Recovery::Small,
                Recovery::Large,
                Recovery::SmallFlat,
                Recovery::LargeFlat,
                Recovery::Adaptive,
                Recovery::AdaptiveFlat,
            ]
            .into_iter()
            .enumerate()
            {
                let mut a = sensor(w, h, cfa.clone(), true);
                let mut b = a.clone();
                let opts = SegmentationOptions {
                    recovery,
                    strength: 0.2,
                    noise: if mode >= 5 { 0.01 } else { 0.0 },
                    combine: if mode == 4 { 8 } else { 2 },
                    ..Default::default()
                };
                let ac = serial.install(|| segmentation(&mut a, [1.7, 0.85, 2.3], 0.99, &opts));
                let bc = parallel.install(|| segmentation(&mut b, [1.7, 0.85, 2.3], 0.99, &opts));
                let label = format!("{w}x{h} {pat} {recovery:?}");
                assert_eq!(ac, bc, "{label}");
                same_bits(a.data.iter().copied(), b.data.iter().copied(), &label);
                assert_original_fingerprint(fingerprint(a.data.iter().copied()), *expected.next().unwrap(), &label);
            }
        }
    }
}

const DEMOSAIC_BASELINE: &[u64] = &[
    0xe5dd6946f27b9213,
    0xbd16c452fb7d20cf,
    0x91a1dcd2e2c2cfdc,
    0xe5dd6946f27b9213,
    0xdfa842d33dd88113,
    0x5fb7d3e060a128e7,
    0x79989b4784cff0dc,
    0xdfa842d33dd88113,
    0x6c3729f1d71782df,
    0x5b0398040d3b7f0f,
    0x3a1bd5d1f3be09a2,
    0x6c3729f1d71782df,
    0x7decca6b5c7ae8cf,
    0xe5b350187211ecbb,
    0x7e8d4cc07e258312,
    0x7decca6b5c7ae8cf,
    0x2bf195ec24be4822,
    0xa6ad5a227788a54f,
    0x3e6af0e6806ac16d,
    0xe4d0eea24a997c05,
    0x8b7215066e526433,
    0x03ecf921929c4dcf,
    0x7f0493bdc555e905,
    0x77ed9878709f01a7,
    0x32bdb8e13bf7360d,
    0xa90eb3f390d0ed40,
    0xae0d55b6d6c8831c,
    0x8bbc481db41c554d,
    0x44d9f8e551341a75,
    0x3e046ad118ea96d8,
    0x8a8978f7f8b3a578,
    0x29b11b4149424d5c,
    0x7c13bd09fcd36bc5,
    0xa780dfba5071ca99,
    0x5b39110947395e13,
    0x3f63e263d6d89ee2,
    0x23e5e111e11a22cd,
    0x9e53f1feebf09f41,
    0xc5999e87d8e293ca,
    0x483d70d9736e4f70,
    0x630bf394c1a600ec,
    0x613d762d39aad217,
    0x2fdf5e5e5b0f9898,
    0xff6afd2a406ee023,
    0x79500ebd9b42b211,
    0x7c7c2348747938eb,
    0xac6db562a4cc5ebb,
    0x686b6d57d1664d43,
    0xb5f5796e0dd82fcb,
    0x9d0863964e3fd11c,
    0xd6c10b591c03c147,
    0x1a1b11956cf76512,
    0xf3d63af976b8841f,
    0xc0c824dc28ad3480,
    0xb1f6df58e197231a,
    0x23d4766a956a73ce,
    0x74a3f3fcd1dd2802,
    0x33c858f7168471a6,
    0x257f7cdee195fd5b,
    0xbb8f60e8ea218d47,
    0x5e52c4f6bbf4def6,
    0x16a0ae5dffddd782,
    0x607718930c79046d,
    0x4d32b026dbe26fa3,
    0x6fa284c6a27928d5,
    0x11cf39f0b3211155,
    0x76813e7a27a35fc9,
    0x2ea1660eec9dd531,
    0x51548e3a6280fb16,
    0xf81d9833d67d91c5,
    0x54f06ccd8b92fdd3,
    0x6a96f89cd75b28e3,
    0x4fa3bf569d210e5a,
    0x12531f5521ab8c5e,
    0x1112fcaf5faaf3bd,
    0xb59e9f17d0424dcc,
    0x4725baffa85a7d46,
    0x824eb4b65a0db34a,
    0x8d143d986b0e031c,
    0x907db5ba71aa59eb,
    0x069bb97350964701,
    0x33b7478a8b0cecf0,
    0xe9d4d65995287bf0,
    0x44e5d1bf98b2afa8,
    0xad1796a142803a54,
    0xd5051785389a0484,
    0xad04705917d2bf74,
    0xc8e3b1e8238d5055,
    0x931837272c6cd16e,
    0x9a19c3001e76e083,
    0xe0be0d616f35791c,
    0x3dd2b383a3688a28,
    0x8f7e1bd78ce59260,
    0x152debb73a1bb1d7,
    0xa8546ff79513e682,
    0x5a24deb540b1dc11,
    0xf74ff70e5779ab7c,
    0xb4ef85aa38b4aef0,
    0xe41c1a0c65dee0e8,
    0x03fd1b1abd8275df,
    0x16473eb85ca38572,
    0x8ed84727528e7ac8,
    0x3149cb9751d443d6,
    0x65616c4564168ec8,
    0x76bb08572f0280c4,
    0x4860f64d65640dc3,
    0xbc89ba649f038aac,
    0x574c9ad1d4a7973c,
    0x99b59693149390bf,
    0xf589ebd32e30f7fb,
    0xc9ec40d3ecedf0fb,
    0x2b7d40da027e2f4f,
    0x9191559473392383,
    0xc01e4c9d8adaa591,
    0xc6b568430d8a7ad8,
    0x806cade067341096,
    0xa0c551a65004b81b,
    0x82745f353451cd7d,
    0x8e33c030f4e7eb45,
    0xc95ad06c8e6b17c0,
    0x4be86f1dab4e68a7,
    0x836c5f81d2880c6d,
    0x4efe14636ac88b53,
    0xb9b7e534c56196dd,
    0x8d4a5e6b3da5e006,
    0xa212b15cf6bc2221,
    0x00ba4ce2e73cd3c1,
    0x4e6e54c9e8e67cf6,
];

const SEGMENTATION_BASELINE: &[u64] = &[
    0x32036ef0dca78e91,
    0x32036ef0dca78e91,
    0x32036ef0dca78e91,
    0x32036ef0dca78e91,
    0x32036ef0dca78e91,
    0x32036ef0dca78e91,
    0x32036ef0dca78e91,
    0x653480a5528011fe,
    0x653480a5528011fe,
    0x653480a5528011fe,
    0x653480a5528011fe,
    0x653480a5528011fe,
    0x653480a5528011fe,
    0x653480a5528011fe,
    0x77320ba47941c6e0,
    0x77320ba47941c6e0,
    0x77320ba47941c6e0,
    0x77320ba47941c6e0,
    0x77320ba47941c6e0,
    0x77320ba47941c6e0,
    0x77320ba47941c6e0,
    0xb9c838248edaf1bf,
    0xb9c838248edaf1bf,
    0xb9c838248edaf1bf,
    0xb9c838248edaf1bf,
    0xb9c838248edaf1bf,
    0xb9c838248edaf1bf,
    0xb9c838248edaf1bf,
    0x60256ee7189c8893,
    0x60256ee7189c8893,
    0x60256ee7189c8893,
    0x60256ee7189c8893,
    0x60256ee7189c8893,
    0x60256ee7189c8893,
    0x60256ee7189c8893,
    0x9ea87329023e3e16,
    0xe834dcb78ef34bd1,
    0x8241090703f39f15,
    0x9a6e5543ed77cc46,
    0x4b1c38e94f965f91,
    0xac0df947f2109e48,
    0x4b4d3bcd25b4c44b,
    0xc8cd0f2b737419e3,
    0x2638a50067349b79,
    0x63293e61e622548b,
    0x25f4b815648a316c,
    0xada04b8e4b071f12,
    0x278264dc6c815557,
    0xb11f9db6dadf9133,
    0x60bdb7965ca87aa0,
    0x45b2c3d785fadb9a,
    0xef01cf7fae2d73e9,
    0x29657a67a5b8f0a3,
    0xebf0c20d3acfc0e9,
    0xd750f81f5e2500a4,
    0xdd9b34aa87e02c4b,
    0x18c65522b9e9e917,
    0x7140e36102a0c42e,
    0x59222a420605f530,
    0x8f5e817aad381b16,
    0xce5d5c1a80781f3d,
    0x1a31c899feb8ad17,
    0xbda36fa40f8b012d,
    0x68178fe6879c3857,
    0xdea55742cae2be0f,
    0xd12e84366d05b85b,
    0x88058e4cd04323c5,
    0x8533b931ade06bbf,
    0xd72c18a1c57b3441,
    0x57695f4372afe80e,
    0x723b3c142e55e7fe,
    0xf18a664c21234b35,
    0xbf89e606c562ab1c,
    0xe640f78bf37f31c0,
    0x905dc94c5226dc69,
    0x7ffde626ce50d699,
    0xb113256b0f176e42,
    0x0b8cfd5b3f37839a,
    0x7f9da18f96d01f57,
    0x5f5d1eabe10d303d,
    0xfa3d0953d9b29048,
    0x542932d321de013d,
    0xb3d4fe28032dfa6e,
    0xb6d72096010a6e4b,
    0x789a066c3e3fd3fd,
    0x57731bde4b796724,
    0xc16953d38ecd4f02,
    0xeb8453bac8d2deb0,
    0x3fba71cc6c96433c,
    0x66bbccb3716c66fc,
    0xee62789504052980,
    0x4ab679ed07ef3eb1,
    0x01ea05a683296f8c,
    0xdfea5b98167dcd4b,
    0xba7340700065635d,
    0x91ecb89460d321ec,
    0xbe0ddd4d49ba957f,
    0x2bea071cc208387a,
    0xbcf8bb6064ed481d,
    0x9db30f1e1b41eb6d,
    0x2a580c3a46d00bd1,
    0xa9ec69516074f2bf,
    0xa54aa0b68347e8b0,
    0x846a2eed593686bb,
    0xa08e1a6bf7e228e8,
    0x5c04b8e3c78b6af6,
    0xa526d2651b256bd3,
    0x743d06cbb137637a,
    0x4fedb0e4c0960fd9,
    0x379f9a88feb6eaf0,
    0xf918a6cc49257b74,
    0xadaad3137f5b2b3b,
    0xa16e476c19256640,
    0x8bf04164475bcd03,
    0x46ab6e7d7ea9a220,
    0xc73045ad7822f9f1,
    0xfb4fbfb550768efc,
    0x67e9b9d06c4449de,
    0xed0f6165b1458cb6,
    0x9bcb9d6a7748de80,
    0xde570618a60f3171,
    0x3c2cbff78cd036a6,
    0x0c78a2c6613ccaa7,
    0x1cce42546a4a6cad,
    0xa6d406abaa669027,
    0xf1eacee6785dc153,
    0x7fca34a4645ac94c,
    0x418dc14536c4ab25,
    0x87ef1e9ffd614497,
    0x47062a90f20e200b,
    0x9a0ff534d83efe0d,
    0xb88f286e60afa307,
    0xea947032cd38e88a,
    0x3ad21eda145ab384,
    0x88a1cbedfcfff349,
    0x561f9b4796260c7a,
    0xdff018c076dc03e1,
    0xaf28bd7ea81ae274,
    0x050e4bc6fd71bd60,
    0xf119b9e7573af6ef,
    0xf1e1ea73195f0518,
    0xd7b17b51a00abd5c,
    0x9394a4ea26352c51,
    0x22fe28fdfced445e,
    0xfff5de910f6cb6de,
    0x4670ed86aab6f090,
    0x7e6f665ffea2b690,
    0xe593ee171caf18c2,
    0x54f1fcd0b9f5b2f8,
    0x98c831370ac3462e,
    0xda9b349416f5d11d,
    0x768720152343e7d0,
    0x861651eb78637c63,
    0xc6006cab242e0213,
    0x6f586527a987c141,
    0x20c9365eb3c07731,
    0x981ef82b5647a9f2,
    0x46a4d7bdc5aeb9af,
    0x1e82e3ed8dfc5ca1,
    0x58550d3ea82d236d,
    0x37195d56d60f15c9,
    0x08af4d728ae51316,
    0x4690a3ac161b0798,
    0x5de5aa943ceddfff,
    0x3feec1991130bad1,
    0x00da56bb73c550c3,
    0x03fb8e109a6fcd80,
    0x62d6e1063fc47efc,
    0xe298dba22046cac9,
    0xc92166873fad5318,
    0x9726471fe308b8a3,
    0x112cd443f1907fca,
    0xbe382ca84e56ac64,
    0x02a7024bc06a1107,
    0x573a0df87c530e93,
];

fn check_amaze_cases(cases: &[(usize, usize, &str, u64)]) {
    let (serial, parallel) = (pool(1), pool(8));
    for &(w, h, pat, expected) in cases {
        let n = sensor(w, h, Cfa::bayer(pat).unwrap(), false);
        let a = serial.install(|| lightcraft_raw::demosaic(&n, Method::Amaze));
        let b = parallel.install(|| lightcraft_raw::demosaic(&n, Method::Amaze));
        let label = format!("{w}x{h} {pat} AMaZE");
        same_bits(a.data.iter().flatten().copied(), b.data.iter().flatten().copied(), &label);
        assert_original_fingerprint(fingerprint(a.data.iter().flatten().copied()), expected, &label);
    }
}

#[test]
fn amaze_tall_narrow_and_padding_original_bits() {
    check_amaze_cases(&[
        (4, 17, "RGGB", 0x4a43052235f3db4d),
        (4, 17, "BGGR", 0x68334101c5ae98ed),
        (4, 17, "GRBG", 0x0ca8e99024c48873),
        (4, 17, "GBRG", 0x2438d868531f886f),
        (17, 4, "RGGB", 0xb317ea8f1adfb80b),
        (17, 4, "BGGR", 0xbea1e6aa6fe3875b),
        (17, 4, "GRBG", 0x8cdc396d7114ba55),
        (17, 4, "GBRG", 0x20fa06d276263015),
        (34, 513, "RGGB", 0x2c1b3ed93419a85a),
        (34, 513, "BGGR", 0x4a7c219e0566ec46),
        (34, 513, "GRBG", 0xe016933410598758),
        (34, 513, "GBRG", 0xcc95e882358c6798),
        (126, 1025, "RGGB", 0xd1344188e066cfa5),
        (126, 1025, "BGGR", 0x41e8a2a5f70b62a5),
        (126, 1025, "GRBG", 0xd1de020885d90718),
        (126, 1025, "GBRG", 0x37bb2baa762c0004),
        (128, 1025, "RGGB", 0x6690f6a60febb2fb),
        (128, 1025, "BGGR", 0xb747c4f8422abf37),
        (128, 1025, "GRBG", 0x1ff753e3d067ec06),
        (128, 1025, "GBRG", 0xf7bc1fcc064bd74e),
        (129, 1025, "RGGB", 0x3ba1e8f2f833b924),
        (129, 1025, "BGGR", 0x1a0d387e65e5882f),
        (129, 1025, "GRBG", 0x5b33ce4d22b190d0),
        (129, 1025, "GBRG", 0xafdde473d1f54f50),
        (255, 1025, "RGGB", 0x44881672864141d2),
        (255, 1025, "BGGR", 0x50b268fe24cb595b),
        (255, 1025, "GRBG", 0xac4d5f383a60b972),
        (255, 1025, "GBRG", 0x26bfb064e235a371),
        (257, 1031, "RGGB", 0x61d2d9aff62d38e8),
        (257, 1031, "BGGR", 0x3ddb11b2696050c0),
        (257, 1031, "GRBG", 0x667e5d2406039ada),
        (257, 1031, "GBRG", 0xafe503dbb80eb094),
    ]);
}

#[test]
#[ignore = "24 MP full-output comparison; run explicitly with --ignored"]
fn amaze_24mp_original_bits() {
    check_amaze_cases(&[
        (6000, 4000, "RGGB", 0xceba782ab80ab846),
        (6000, 4000, "BGGR", 0x8a43a2c6650d44be),
        (6000, 4000, "GRBG", 0x058794ee2ce1d526),
        (6000, 4000, "GBRG", 0x4b472aac62199dd2),
    ]);
}
