use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use curve25519_dalek::edwards::EdwardsPoint;
use curve25519_dalek::scalar::Scalar as CurveScalar;
use ed25519_dalek::{PublicKey, SecretKey};

use blake2::Blake2b;
use digest::{Input, VariableOutput};

use num_bigint::BigInt;
use num_traits::ToPrimitive;

pub const ADDRESS_ALPHABET: &[u8] = b"13456789abcdefghijkmnopqrstuwxyz";

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum GenerateKeyType {
    PrivateKey,
    Seed,
    /// Parameter is public offset
    ExtendedPrivateKey(EdwardsPoint),
}

fn ed25519_privkey_to_pubkey(sec: &[u8; 32]) -> [u8; 32] {
    let secret_key = SecretKey::from_bytes(sec).unwrap();
    let public_key = PublicKey::from_secret::<Blake2b>(&secret_key);
    public_key.to_bytes()
}

pub fn secret_to_pubkey(key_material: [u8; 32], generate_key_type: GenerateKeyType) -> [u8; 32] {
    match generate_key_type {
        GenerateKeyType::PrivateKey => ed25519_privkey_to_pubkey(&key_material),
        GenerateKeyType::Seed => {
            let mut private_key = [0u8; 32];
            let mut hasher = Blake2b::new(32).unwrap();
            hasher.process(&key_material);
            hasher.process(&[0, 0, 0, 0]);
            hasher.variable_result(&mut private_key).unwrap();
            ed25519_privkey_to_pubkey(&private_key)
        }
        GenerateKeyType::ExtendedPrivateKey(offset) => {
            let scalar = CurveScalar::from_bytes_mod_order(key_material);
            let curvepoint = &scalar * &ED25519_BASEPOINT_TABLE;
            (&curvepoint + &offset).compress().to_bytes()
        }
    }
}

/// Only used when outputting addresses to user. Not for speed.
pub fn pubkey_to_address(pubkey: [u8; 32]) -> String {
    let mut reverse_chars = Vec::<u8>::new();
    let mut check_hash = Blake2b::new(5).unwrap();
    check_hash.process(&pubkey as &[u8]);
    let mut check = [0u8; 5];
    check_hash.variable_result(&mut check).unwrap();
    let mut ext_pubkey = pubkey.to_vec();
    ext_pubkey.extend(check.iter().rev());
    let mut ext_pubkey_int = BigInt::from_bytes_be(num_bigint::Sign::Plus, &ext_pubkey);
    for _ in 0..60 {
        let n: BigInt = (&ext_pubkey_int) % 32; // lower 5 bits
        reverse_chars.push(ADDRESS_ALPHABET[n.to_usize().unwrap()]);
        ext_pubkey_int = ext_pubkey_int >> 5;
    }
    reverse_chars.extend(b"_brx"); // xrb_ reversed
    reverse_chars
        .iter()
        .rev()
        .map(|&c| c as char)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    // importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_ed25519_secret_to_pubkey() {
        // generated by nanocurrency-js
        // Seed: fb15ac405d762002202c66bd249589ad450d55631f7b1cd44fef19fcccbc6372
        // Secret: 847B0EC950A7F5B6AD6C3A1AA5A5B940608435B59F201662D13A6D11F65F7DA6
        // Pubkey: D741569435DC9698AAE5212A437F5DEDA76EFC4288CA3FCDE9604190A861FE07
        // Address: xrb_3ot3ctc5dq6pm4ogcabcafzouuf9fuy6748c9z8ykr43k4n85zi9zec5bxnz
        let mut privkey = [0u8; 32];
        privkey.copy_from_slice(
            &hex::decode("847B0EC950A7F5B6AD6C3A1AA5A5B940608435B59F201662D13A6D11F65F7DA6")
                .unwrap(),
        );
        let mut expected_pubkey = [0u8; 32];
        expected_pubkey.copy_from_slice(
            &hex::decode("D741569435DC9698AAE5212A437F5DEDA76EFC4288CA3FCDE9604190A861FE07")
                .unwrap(),
        );
        assert_eq!(ed25519_privkey_to_pubkey(&privkey), expected_pubkey);
    }

    #[test]
    fn test_secret_to_pubkey_from_privkey() {
        // generated by nanocurrency-js
        // Seed: fb15ac405d762002202c66bd249589ad450d55631f7b1cd44fef19fcccbc6372
        // Secret: 847B0EC950A7F5B6AD6C3A1AA5A5B940608435B59F201662D13A6D11F65F7DA6
        // Pubkey: D741569435DC9698AAE5212A437F5DEDA76EFC4288CA3FCDE9604190A861FE07
        // Address: xrb_3ot3ctc5dq6pm4ogcabcafzouuf9fuy6748c9z8ykr43k4n85zi9zec5bxnz
        let mut privkey = [0u8; 32];
        privkey.copy_from_slice(
            &hex::decode("847B0EC950A7F5B6AD6C3A1AA5A5B940608435B59F201662D13A6D11F65F7DA6")
                .unwrap(),
        );
        let mut expected_pubkey = [0u8; 32];
        expected_pubkey.copy_from_slice(
            &hex::decode("D741569435DC9698AAE5212A437F5DEDA76EFC4288CA3FCDE9604190A861FE07")
                .unwrap(),
        );
        assert_eq!(
            secret_to_pubkey(privkey, GenerateKeyType::PrivateKey),
            expected_pubkey
        );
    }

    #[test]
    fn test_secret_to_pubkey_from_seed() {
        // generated by nanocurrency-js
        // Seed: fb15ac405d762002202c66bd249589ad450d55631f7b1cd44fef19fcccbc6372
        // Secret: 847B0EC950A7F5B6AD6C3A1AA5A5B940608435B59F201662D13A6D11F65F7DA6
        // Pubkey: D741569435DC9698AAE5212A437F5DEDA76EFC4288CA3FCDE9604190A861FE07
        // Address: xrb_3ot3ctc5dq6pm4ogcabcafzouuf9fuy6748c9z8ykr43k4n85zi9zec5bxnz
        let mut seed = [0u8; 32];
        seed.copy_from_slice(
            &hex::decode("fb15ac405d762002202c66bd249589ad450d55631f7b1cd44fef19fcccbc6372")
                .unwrap(),
        );
        let mut expected_pubkey = [0u8; 32];
        expected_pubkey.copy_from_slice(
            &hex::decode("D741569435DC9698AAE5212A437F5DEDA76EFC4288CA3FCDE9604190A861FE07")
                .unwrap(),
        );
        assert_eq!(
            secret_to_pubkey(seed, GenerateKeyType::Seed),
            expected_pubkey
        );
    }

    #[test]
    fn test_pubkey_to_address() {
        // generated by nanocurrency-js
        // Seed: fb15ac405d762002202c66bd249589ad450d55631f7b1cd44fef19fcccbc6372
        // Secret: 847B0EC950A7F5B6AD6C3A1AA5A5B940608435B59F201662D13A6D11F65F7DA6
        // Pubkey: D741569435DC9698AAE5212A437F5DEDA76EFC4288CA3FCDE9604190A861FE07
        // Address: xrb_3ot3ctc5dq6pm4ogcabcafzouuf9fuy6748c9z8ykr43k4n85zi9zec5bxnz
        let mut pubkey = [0u8; 32];
        pubkey.copy_from_slice(
            &hex::decode("D741569435DC9698AAE5212A437F5DEDA76EFC4288CA3FCDE9604190A861FE07")
                .unwrap(),
        );
        assert_eq!(
            pubkey_to_address(pubkey),
            "xrb_3ot3ctc5dq6pm4ogcabcafzouuf9fuy6748c9z8ykr43k4n85zi9zec5bxnz"
        );
    }
}
