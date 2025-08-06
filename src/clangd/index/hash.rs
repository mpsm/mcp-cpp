//! Hash functions for clangd index file naming
//!
//! Clangd uses different hash functions based on version:
//! - Versions 12-18: xxHash64
//! - Versions 19-20: xxh3_64bits

/// Compute hash for file path based on clangd version
pub fn compute_file_hash(path: &str, format_version: u32) -> u64 {
    let bytes = path.as_bytes();
    if format_version <= 18 {
        xxhash64(bytes, 0)
    } else {
        xxh3_64bits(bytes)
    }
}

/// xxHash64 implementation for clangd versions 12-18
pub fn xxhash64(mut data: &[u8], seed: u64) -> u64 {
    const PRIME64_1: u64 = 0x9E3779B185EBCA87;
    const PRIME64_2: u64 = 0xC2B2AE3D27D4EB4F;
    const PRIME64_3: u64 = 0x165667B19E3779F9;
    const PRIME64_4: u64 = 0x85EBCA77C2B2AE63;
    const PRIME64_5: u64 = 0x27D4EB2F165667C5;

    let mut h64: u64;
    let len = data.len();

    if len >= 32 {
        let mut v1 = seed.wrapping_add(PRIME64_1).wrapping_add(PRIME64_2);
        let mut v2 = seed.wrapping_add(PRIME64_2);
        let mut v3 = seed;
        let mut v4 = seed.wrapping_sub(PRIME64_1);

        let chunks = data.chunks_exact(32);
        let remainder = chunks.remainder();

        for chunk in chunks {
            v1 = xxh64_round(v1, read_u64_le(&chunk[0..8]));
            v2 = xxh64_round(v2, read_u64_le(&chunk[8..16]));
            v3 = xxh64_round(v3, read_u64_le(&chunk[16..24]));
            v4 = xxh64_round(v4, read_u64_le(&chunk[24..32]));
        }

        h64 = v1
            .rotate_left(1)
            .wrapping_add(v2.rotate_left(7))
            .wrapping_add(v3.rotate_left(12))
            .wrapping_add(v4.rotate_left(18));

        h64 = xxh64_merge_round(h64, v1);
        h64 = xxh64_merge_round(h64, v2);
        h64 = xxh64_merge_round(h64, v3);
        h64 = xxh64_merge_round(h64, v4);

        data = remainder;
    } else {
        h64 = seed.wrapping_add(PRIME64_5);
    }

    h64 = h64.wrapping_add(len as u64);

    // Process remaining bytes
    let mut offset = 0;
    while offset + 8 <= data.len() {
        let k1 = xxh64_round(0, read_u64_le(&data[offset..offset + 8]));
        h64 ^= k1;
        h64 = h64
            .rotate_left(27)
            .wrapping_mul(PRIME64_1)
            .wrapping_add(PRIME64_4);
        offset += 8;
    }

    if offset + 4 <= data.len() {
        h64 ^= (read_u32_le(&data[offset..offset + 4]) as u64).wrapping_mul(PRIME64_1);
        h64 = h64
            .rotate_left(23)
            .wrapping_mul(PRIME64_2)
            .wrapping_add(PRIME64_3);
        offset += 4;
    }

    while offset < data.len() {
        h64 ^= (data[offset] as u64).wrapping_mul(PRIME64_5);
        h64 = h64.rotate_left(11).wrapping_mul(PRIME64_1);
        offset += 1;
    }

    // Final avalanche
    h64 ^= h64 >> 33;
    h64 = h64.wrapping_mul(PRIME64_2);
    h64 ^= h64 >> 29;
    h64 = h64.wrapping_mul(PRIME64_3);
    h64 ^= h64 >> 32;

    h64
}

fn xxh64_round(acc: u64, input: u64) -> u64 {
    const PRIME64_1: u64 = 0x9E3779B185EBCA87;
    const PRIME64_2: u64 = 0xC2B2AE3D27D4EB4F;

    acc.wrapping_add(input.wrapping_mul(PRIME64_2))
        .rotate_left(31)
        .wrapping_mul(PRIME64_1)
}

fn xxh64_merge_round(acc: u64, val: u64) -> u64 {
    const PRIME64_1: u64 = 0x9E3779B185EBCA87;
    const PRIME64_4: u64 = 0x85EBCA77C2B2AE63;

    let val = xxh64_round(0, val);
    (acc ^ val).wrapping_mul(PRIME64_1).wrapping_add(PRIME64_4)
}

/// xxh3_64bits implementation for clangd versions 19-20
pub fn xxh3_64bits(data: &[u8]) -> u64 {
    // xxh3 secret key (first 192 bytes)
    const SECRET: &[u8] = &[
        0xb8, 0xfe, 0x6c, 0x39, 0x23, 0xa4, 0x4b, 0xbe, 0x7c, 0x01, 0x81, 0x2c, 0xf7, 0x21, 0xad,
        0x1c, 0xde, 0xd4, 0x6d, 0xe9, 0x83, 0x90, 0x97, 0xdb, 0x72, 0x40, 0xa4, 0xa4, 0xb7, 0xb3,
        0x67, 0x1f, 0xcb, 0x79, 0xe6, 0x4e, 0xcc, 0xc0, 0xe5, 0x78, 0x82, 0x5a, 0xd0, 0x7d, 0xcc,
        0xff, 0x72, 0x21, 0xb8, 0x08, 0x46, 0x74, 0xf7, 0x43, 0x24, 0x8e, 0xe0, 0x35, 0x90, 0xe6,
        0x81, 0x3a, 0x26, 0x4c, 0x3c, 0x28, 0x52, 0xbb, 0x91, 0xc3, 0x00, 0xcb, 0x88, 0xd0, 0x65,
        0x8b, 0x1b, 0x53, 0x2e, 0xa3, 0x71, 0x64, 0x48, 0x97, 0xa2, 0x0d, 0xf9, 0x4e, 0x38, 0x19,
        0xef, 0x46, 0xa9, 0xde, 0xac, 0xd8, 0xa8, 0xfa, 0x76, 0x3f, 0xe3, 0x9c, 0x34, 0x3f, 0xf9,
        0xdc, 0xbb, 0xc7, 0xc7, 0x0b, 0x4f, 0x1d, 0x8a, 0x51, 0xe0, 0x4b, 0xcd, 0xb4, 0x59, 0x31,
        0xc8, 0x9f, 0x7e, 0xc9, 0xd9, 0x78, 0x73, 0x64, 0xea, 0xc5, 0xac, 0x83, 0x34, 0xd3, 0xeb,
        0xc3, 0xc5, 0x81, 0xa0, 0xff, 0xfa, 0x13, 0x63, 0xeb, 0x17, 0x0d, 0xdd, 0x51, 0xb7, 0xf0,
        0xda, 0x49, 0xd3, 0x16, 0x55, 0x26, 0x29, 0xd4, 0x68, 0x9e, 0x2b, 0x16, 0xbe, 0x58, 0x7d,
        0x47, 0xa1, 0xfc, 0x8f, 0xf8, 0xb8, 0xd1, 0x7a, 0xd0, 0x31, 0xce, 0x45, 0xcb, 0x3a, 0x8f,
        0x95, 0x16, 0x04, 0x28, 0xaf, 0xd7, 0xfb, 0xca, 0xbb, 0x4b, 0x40, 0x7e,
    ];

    let len = data.len();

    if len <= 16 {
        xxh3_len_0to16_64b(data, SECRET)
    } else if len <= 128 {
        xxh3_len_17to128_64b(data, SECRET)
    } else if len <= 240 {
        xxh3_len_129to240_64b(data, SECRET)
    } else {
        xxh3_hash_long_64b(data, SECRET)
    }
}

fn xxh3_len_0to16_64b(data: &[u8], secret: &[u8]) -> u64 {
    let len = data.len();

    if len > 8 {
        xxh3_len_9to16_64b(data, secret)
    } else if len >= 4 {
        xxh3_len_4to8_64b(data, secret)
    } else if len > 0 {
        xxh3_len_1to3_64b(data, secret)
    } else {
        xxh64_avalanche(read_u64_le(&secret[56..64]) ^ read_u64_le(&secret[64..72]))
    }
}

fn xxh3_len_1to3_64b(data: &[u8], secret: &[u8]) -> u64 {
    let c1 = data[0];
    let c2 = data[data.len() / 2];
    let c3 = data[data.len() - 1];
    let combined =
        ((c1 as u32) << 16) | ((c2 as u32) << 24) | c3 as u32 | ((data.len() as u32) << 8);
    let bitflip = (read_u32_le(&secret[0..4]) ^ read_u32_le(&secret[4..8])) as u64;
    xxh64_avalanche(combined as u64 ^ bitflip)
}

fn xxh3_len_4to8_64b(data: &[u8], secret: &[u8]) -> u64 {
    let input1 = read_u32_le(&data[0..4]);
    let input2 = read_u32_le(&data[data.len() - 4..]);
    let bitflip = read_u64_le(&secret[8..16]) ^ read_u64_le(&secret[16..24]);
    let input64 = input1 as u64 + ((input2 as u64) << 32);
    let keyed = input64 ^ bitflip;
    xxh3_rrmxmx(keyed, data.len() as u64)
}

fn xxh3_len_9to16_64b(data: &[u8], secret: &[u8]) -> u64 {
    let bitflip1 = read_u64_le(&secret[24..32]) ^ read_u64_le(&secret[32..40]);
    let bitflip2 = read_u64_le(&secret[40..48]) ^ read_u64_le(&secret[48..56]);
    let input_lo = read_u64_le(&data[0..8]) ^ bitflip1;
    let input_hi = read_u64_le(&data[data.len() - 8..]) ^ bitflip2;
    let acc = (data.len() as u64)
        .wrapping_add(input_lo.swap_bytes())
        .wrapping_add(input_hi)
        .wrapping_add(xxh3_mul128_fold64(input_lo, input_hi));
    xxh3_avalanche(acc)
}

fn xxh3_len_17to128_64b(data: &[u8], secret: &[u8]) -> u64 {
    let len = data.len();
    let mut acc = (len as u64).wrapping_mul(0xC2B2AE3D27D4EB4F);

    if len > 32 {
        if len > 64 {
            if len > 96 {
                acc = acc.wrapping_add(xxh3_mix16b(&data[48..], &secret[96..], 0));
                acc = acc.wrapping_add(xxh3_mix16b(&data[len - 64..], &secret[112..], 0));
            }
            acc = acc.wrapping_add(xxh3_mix16b(&data[32..], &secret[64..], 0));
            acc = acc.wrapping_add(xxh3_mix16b(&data[len - 48..], &secret[80..], 0));
        }
        acc = acc.wrapping_add(xxh3_mix16b(&data[16..], &secret[32..], 0));
        acc = acc.wrapping_add(xxh3_mix16b(&data[len - 32..], &secret[48..], 0));
    }

    acc = acc.wrapping_add(xxh3_mix16b(&data[0..], &secret[0..], 0));
    acc = acc.wrapping_add(xxh3_mix16b(&data[len - 16..], &secret[16..], 0));

    xxh3_avalanche(acc)
}

fn xxh3_len_129to240_64b(data: &[u8], secret: &[u8]) -> u64 {
    let len = data.len();
    let nb_rounds = len / 16;
    let mut acc = (len as u64).wrapping_mul(0xC2B2AE3D27D4EB4F);

    for i in 0..8 {
        acc = acc.wrapping_add(xxh3_mix16b(&data[16 * i..], &secret[16 * i..], 0));
    }

    acc = xxh3_avalanche(acc);

    for i in 8..nb_rounds {
        acc = acc.wrapping_add(xxh3_mix16b(&data[16 * i..], &secret[16 * (i - 8) + 3..], 0));
    }

    acc = acc.wrapping_add(xxh3_mix16b(&data[len - 16..], &secret[119..], 0));

    xxh3_avalanche(acc)
}

fn xxh3_hash_long_64b(data: &[u8], secret: &[u8]) -> u64 {
    let mut acc = [
        0x9E3779B185EBCA87u64,
        0xC2B2AE3D27D4EB4F,
        0x165667B19E3779F9,
        0x85EBCA77C2B2AE63,
        0x27D4EB2F165667C5,
        0x94D049BB133111EB,
        0x2293EA3D2D4E4424,
        0xDF92B4C8A0F4119C,
    ];

    let nb_blocks = (secret.len() - 64) / 8;
    let block_len = 64 * nb_blocks;

    let mut offset = 0;
    while offset + block_len <= data.len() {
        xxh3_accumulate(
            &mut acc,
            &data[offset..offset + block_len],
            secret,
            nb_blocks,
        );
        xxh3_scramble_acc(&mut acc, &secret[secret.len() - 64..]);
        offset += block_len;
    }

    let nb_stripes = ((data.len() - offset) - 1) / 64;
    for i in 0..nb_stripes {
        xxh3_accumulate_512(&mut acc, &data[offset + i * 64..], &secret[i * 8..]);
    }

    // Last partial block
    xxh3_accumulate_512(
        &mut acc,
        &data[data.len() - 64..],
        &secret[secret.len() - 71..],
    );

    xxh3_merge_accs(
        &acc,
        &secret[11..],
        (data.len() as u64).wrapping_mul(0xC2B2AE3D27D4EB4F),
    )
}

fn xxh3_accumulate(acc: &mut [u64; 8], data: &[u8], secret: &[u8], nb_blocks: usize) {
    for n in 0..nb_blocks {
        xxh3_accumulate_512(acc, &data[n * 64..], &secret[n * 8..]);
    }
}

fn xxh3_accumulate_512(acc: &mut [u64; 8], data: &[u8], secret: &[u8]) {
    for i in 0..8 {
        let data_val = read_u64_le(&data[8 * i..]);
        let secret_val = read_u64_le(&secret[8 * i..]);
        acc[i ^ 1] = acc[i ^ 1].wrapping_add(data_val);
        acc[i] = acc[i].wrapping_add(xxh3_mul128_fold64(
            data_val ^ secret_val,
            data_val ^ secret_val.swap_bytes(),
        ));
    }
}

fn xxh3_scramble_acc(acc: &mut [u64; 8], secret: &[u8]) {
    for i in 0..8 {
        let secret_val = read_u64_le(&secret[8 * i..]);
        acc[i] = xxh3_xorshift64(acc[i], 47) ^ secret_val;
        acc[i] = acc[i].wrapping_mul(0x9E3779B185EBCA87);
    }
}

fn xxh3_merge_accs(acc: &[u64; 8], secret: &[u8], start: u64) -> u64 {
    let mut result = start;

    for i in 0..4 {
        result = result.wrapping_add(xxh3_mix2accs(acc[2 * i], acc[2 * i + 1], &secret[16 * i..]));
    }

    xxh3_avalanche(result)
}

fn xxh3_mix2accs(acc1: u64, acc2: u64, secret: &[u8]) -> u64 {
    xxh3_mul128_fold64(
        acc1 ^ read_u64_le(&secret[0..]),
        acc2 ^ read_u64_le(&secret[8..]),
    )
}

fn xxh3_mix16b(data: &[u8], secret: &[u8], seed: u64) -> u64 {
    let input_lo = read_u64_le(&data[0..8]);
    let input_hi = read_u64_le(&data[8..16]);
    xxh3_mul128_fold64(
        input_lo ^ (read_u64_le(&secret[0..8]).wrapping_add(seed)),
        input_hi ^ (read_u64_le(&secret[8..16]).wrapping_sub(seed)),
    )
}

fn xxh3_mul128_fold64(lhs: u64, rhs: u64) -> u64 {
    let product = (lhs as u128).wrapping_mul(rhs as u128);
    (product as u64) ^ ((product >> 64) as u64)
}

fn xxh3_xorshift64(v: u64, shift: u32) -> u64 {
    v ^ (v >> shift)
}

fn xxh3_rrmxmx(h: u64, len: u64) -> u64 {
    let h = h ^ (h.rotate_left(49) ^ h.rotate_left(24));
    let h = h.wrapping_mul(0x9FB21C651E98DF25);
    let h = h ^ ((h >> 35).wrapping_add(len));
    let h = h.wrapping_mul(0x9FB21C651E98DF25);
    h ^ (h >> 28)
}

fn xxh3_avalanche(h: u64) -> u64 {
    let h = xxh3_xorshift64(h, 37);
    let h = h.wrapping_mul(0x165667919E3779F9);
    xxh3_xorshift64(h, 32)
}

fn xxh64_avalanche(h: u64) -> u64 {
    let h = h ^ (h >> 33);
    let h = h.wrapping_mul(0xC2B2AE3D27D4EB4F);
    let h = h ^ (h >> 29);
    let h = h.wrapping_mul(0x165667B19E3779F9);
    h ^ (h >> 32)
}

fn read_u32_le(data: &[u8]) -> u32 {
    u32::from_le_bytes([data[0], data[1], data[2], data[3]])
}

fn read_u64_le(data: &[u8]) -> u64 {
    u64::from_le_bytes([
        data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xxhash64_reference_vectors() {
        // Reference values generated by C implementation
        assert_eq!(xxhash64(b"", 0), 0xEF46DB3751D8E999);
        assert_eq!(xxhash64(b"a", 0), 0xD24EC4F1A98C6E5B);
        assert_eq!(xxhash64(b"abc", 0), 0x44BC2CF5AD770999);
        assert_eq!(xxhash64(b"message digest", 0), 0x066ED728FCEEB3BE);
        assert_eq!(
            xxhash64(b"abcdefghijklmnopqrstuvwxyz", 0),
            0xCFE1F278FA89835C
        );
        assert_eq!(
            xxhash64(b"/home/user/project/main.cpp", 0),
            0x29BD10997380DC29
        );
        assert_eq!(xxhash64(b"/usr/include/stdio.h", 0), 0x11CAA5469517AA39);
        assert_eq!(xxhash64(b"/test/project/utils.cpp", 0), 0x8E2DCB19CC85BD47);
    }

    #[test]
    fn test_xxh3_64bits_reference_vectors() {
        // Reference values generated by C implementation (limited test vectors)
        assert_eq!(xxh3_64bits(b""), 0x2D06800538D394C2);
        assert_eq!(xxh3_64bits(b"a"), 0xE6C632B61E964E1F);
        assert_eq!(xxh3_64bits(b"ab"), 0xA873719C24D5735C);
        assert_eq!(xxh3_64bits(b"abc"), 0x78AF5F94892F3950);
    }

    #[test]
    fn test_compute_file_hash() {
        let path = "/home/user/project/main.cpp";

        // Version 18 should use xxhash64
        let hash_v18 = compute_file_hash(path, 18);
        assert_eq!(hash_v18, xxhash64(path.as_bytes(), 0));
        assert_eq!(hash_v18, 0x29BD10997380DC29); // Reference value from C

        // Version 19 should use xxh3_64bits
        let hash_v19 = compute_file_hash(path, 19);
        assert_eq!(hash_v19, xxh3_64bits(path.as_bytes()));

        // Hashes should be different between versions
        assert_ne!(hash_v18, hash_v19);
    }

    #[test]
    fn test_clangd_index_filename_generation() {
        // Test realistic file paths that would be used in clangd index
        let test_paths = vec![
            "/test/project/main.cpp",
            "/test/project/utils.cpp",
            "/usr/include/stdio.h",
        ];

        for path in test_paths {
            let hash_v18 = compute_file_hash(path, 18);
            let hash_v19 = compute_file_hash(path, 19);

            // Verify that we get consistent hashes for each version
            assert_eq!(hash_v18, xxhash64(path.as_bytes(), 0));
            assert_eq!(hash_v19, xxh3_64bits(path.as_bytes()));

            // Generate index filenames as clangd would
            let basename = path.split('/').next_back().unwrap();
            let filename_v18 = format!("{basename}.{hash_v18:016X}.idx");
            let filename_v19 = format!("{basename}.{hash_v19:016X}.idx");

            // Verify format matches clangd convention
            assert!(filename_v18.ends_with(".idx"));
            assert!(filename_v19.ends_with(".idx"));
            assert!(filename_v18.contains(&format!("{hash_v18:016X}")));
            assert!(filename_v19.contains(&format!("{hash_v19:016X}")));
        }
    }

    #[test]
    fn test_reference_paths_specific_hashes() {
        // Test specific paths with known reference values from C implementation

        // Test utils.cpp from our test project (xxHash64 version 18)
        assert_eq!(
            compute_file_hash("/test/project/utils.cpp", 18),
            0x8E2DCB19CC85BD47
        );

        // Verify index filename generation for test project files
        let paths_and_expected_hashes = vec![
            ("/test/project/utils.cpp", 0x8E2DCB19CC85BD47),
            ("/usr/include/stdio.h", 0x11CAA5469517AA39),
        ];

        for (path, expected_hash) in paths_and_expected_hashes {
            let actual_hash = compute_file_hash(path, 18);
            assert_eq!(actual_hash, expected_hash);

            let basename = path.split('/').next_back().unwrap();
            let expected_filename = format!("{basename}.{expected_hash:016X}.idx");
            let actual_filename = format!("{basename}.{actual_hash:016X}.idx");

            assert_eq!(actual_filename, expected_filename);
        }
    }
}
