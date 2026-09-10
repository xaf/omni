const BASE: usize = 62;
const BASE62_CHARS: [u8; BASE] = *b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Encode arbitrary bytes as base62.
///
/// A leading `1` byte is prepended before the value is interpreted, so that
/// leading zero bytes in the input still affect the result. Digits are emitted
/// least significant first, which is what callers have always received; do not
/// "fix" the ordering, as the output is used to derive on-disk paths.
///
/// This is long division on a byte buffer rather than a bignum library. The
/// previous implementation used `num-bigint`, `num-integer` and `num-traits`
/// for this one function; the equivalence tests below pin the output.
pub fn encode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    // Big-endian value: a `1` sentinel followed by the input.
    let mut buf = Vec::with_capacity(bytes.len() + 1);
    buf.push(1u8);
    buf.extend_from_slice(bytes);

    // Each pass divides the whole buffer by 62 and yields one digit. The
    // remainder is always < 62, so `rem << 8 | byte` stays well inside u32.
    let mut encoded = Vec::new();
    let mut first_nonzero = 0;
    while first_nonzero < buf.len() {
        let mut rem: u32 = 0;
        for byte in buf[first_nonzero..].iter_mut() {
            let cur = (rem << 8) | u32::from(*byte);
            *byte = (cur / BASE as u32) as u8;
            rem = cur % BASE as u32;
        }
        encoded.push(BASE62_CHARS[rem as usize]);

        // Leading zero bytes can never contribute again, so skip them on the
        // next pass instead of rescanning the whole buffer each time.
        while first_nonzero < buf.len() && buf[first_nonzero] == 0 {
            first_nonzero += 1;
        }
    }

    // Every byte is ASCII by construction.
    String::from_utf8(encoded).expect("base62 alphabet is ASCII")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("valid hex"))
            .collect()
    }

    /// Output is used to derive on-disk cache and data paths, so it must never
    /// change. These vectors were captured from the previous `num-bigint`
    /// implementation before it was replaced.
    #[test]
    fn matches_the_bigint_implementation() {
        let vectors: &[(&str, &str)] = &[
        ("", ""),
        ("00", "84"),
        ("01", "94"),
        ("ff", "F8"),
        ("00000000", "4CFfg4"),
        ("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff", "3e5HsniBS5KOobTZEWggGY393ZY73gK28nCKGtVlTPx1"),
        ("0000000000000000000000000000000000000000000000000000000000000000", "2pX8wOr5j2ACunjH7GLL8mWZWHmY1LA1ZO6Adwksjhy"),
        ("c503dd6734e29d7dbe78284b22232c4dbc1a7ca66c459d39ac72caf2896ea60f", "HnSIG87Xh43gTxwpMd43Ru8AZLh9LvJ01wp7G55DHQj1"),
        ("836bcf08400fcd302ee0b9d53a0c9f4630bfd6430280d4806a8b54083ae2248d", "JrFBaIlMlJ3hatxB2uAtyH5qqIzO5Qrka6hmxWrHurT1"),
        ("00137c194e7f38b40167b2aa9a6597742a2efdcda3df4d598b937ad688e068ae", "Eie3todlTgVPikHYZOFxfYklljJ28paYIHtXArEGriy"),
        ("ad34dac68ec749ed5294872b360d2ae690a4cc05f19a05b2c03cda48257b9ab2", "upMpYIaLUikxsimhoFbt8Frbm5uNGoiPaj4nwiIuEmd1"),
        ("24494ed6e501e4e710c74fce3d4b0d2cbc9f243fb8776addb3a69f654abfcca9", "t1b2RgTMWVAhb8mJdibzSO90G246TiiAvlHNnQ31EJ71"),
        ("632a77b97efaf96fee7f31434e18d212053cf5a7c63a3a6534dd2cb22a477f67", "ZIA4Sf7lmbKDnoiajHJrjRvfrYMmpvlK2Cm6GPzdgDM1"),
        ("9ab17d0ddfe7aa29be15571f2bf3a6d6c319d10e4092df6ed5d8481fe54d9902", "kY2W9KYOPi2bjYe0woe3CD0BQ0SeaJgKxNosBNNb3OZ1"),
        ("69d9dba78093e32d5d005406f2358be1d1b7f13074904e8a1400cf6f0338e9c7", "ZQMY1S0JZx3OhJcTxfAGB9Zo2pTn1wU1PkfFqErKynN1"),
        ("aa5fae1ddc0f781b13971553d1db17049858c980329562100029db1e64b31bb6", "C3iMvXOnonjnar4bxtoTHBx2f4HcPuzNbdKzegnna6d1"),
        ("b5e682cf0be6c2bb7e504d4237ddb02d8f64106e9ec3bbfc5a1b72d0b5383e7a", "UCnujBmgHm4NMQXEgaJuqiLbWRsfgdQeOgO3xtEj3qf1"),
        ("21929f71d6cdbe0fef8360968dfd499f4148523189ec193e408eb4356824bce0", "2vaWob4ARvFc31jm3bjZKSHxXH8sYIzeqXGHdrPTKf61"),
        ("0e2fd61247d3a7f056a8d6c66c18a3b2d4d7f8133f09389d4dac87742847d6d4", "qaKXMEQjAZnsKjRZhwTgIbxwvHN7JNacC4jjXDsdJ421"),
        ("0b", "J4"),
        ("0b30", "EnH"),
        ("0b3055", "BHTB1"),
        ("0b30557a9fc4e9", "Nb34RkoRY5"),
        ("0b30557a9fc4e90e33587da2c7ec11", "PEt1k9C6kCuuNCDM1Y6y1"),
        ("0b30557a9fc4e90e33587da2c7ec1136", "6U3o3ECaLc4yk8VibT2B88"),
        ("0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c61", "7t6YEFiKOC1iB1bjIxyjHLzrmRgTYQZJbEdo6pxMLF"),
        ("0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186ab", "H5xX0Y9zuFJspfSaxJygV9sX047dBfscrdCXQrGeCpbD4"),
        ("0b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc0126", "CpOkRt4mwd6piavX1rmBzUHOTLSgb1Q5tzUnphZAz8AjPK7qdNHBvkd8Fb5iKbv5xIQ5ExNNLlKnReG9UkeD201"),
        ];

        for (input_hex, expected) in vectors {
            let input = from_hex(input_hex);
            assert_eq!(
                encode(&input),
                *expected,
                "base62 output changed for input {input_hex}"
            );
        }
    }

    #[test]
    fn empty_input_encodes_to_empty_string() {
        assert_eq!(encode(&[]), "");
    }

    #[test]
    fn leading_zero_bytes_are_significant() {
        // The `1` sentinel exists so these do not collapse together.
        assert_ne!(encode(&[0x00, 0x01]), encode(&[0x01]));
        assert_ne!(encode(&[0x00, 0x00, 0x01]), encode(&[0x00, 0x01]));
    }

    #[test]
    fn output_is_only_base62_characters() {
        let input: Vec<u8> = (0u8..=255).collect();
        assert!(encode(&input)
            .bytes()
            .all(|b| BASE62_CHARS.contains(&b)));
    }

    #[test]
    fn is_long_enough_for_the_20_character_truncation_callers_use() {
        // directory.rs and env.rs both slice [..20] of a 32-byte digest hash.
        let digest = [0xABu8; 32];
        assert!(
            encode(&digest).len() >= 20,
            "callers slice [..20] and would panic"
        );
    }
}
