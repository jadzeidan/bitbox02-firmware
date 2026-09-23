// SPDX-License-Identifier: Apache-2.0

use alloc::string::String;
use alloc::vec::Vec;

use bitcoin::hashes::{Hash, hash160, sha256d};
use util::bip32::HARDENED;

use super::Error;

pub const MAX_ACCOUNT: u32 = 99;
pub const MAX_ADDRESS: u32 = 9999;

pub fn validate_bip44_keypath(keypath: &[u32], coin: u32) -> Result<(), Error> {
    match keypath {
        [purpose, keypath_coin, account, change, address]
            if *purpose == 44 + HARDENED
                && *keypath_coin == coin + HARDENED
                && (HARDENED..=HARDENED + MAX_ACCOUNT).contains(account)
                && *change <= 1
                && *address <= MAX_ADDRESS =>
        {
            Ok(())
        }
        _ => Err(Error::InvalidInput),
    }
}

pub async fn secp256k1_pubkey(
    hal: &mut impl crate::hal::Hal,
    keypath: &[u32],
) -> Result<crate::bip32::Xpub, Error> {
    crate::keystore::get_xpub(hal, keypath, crate::keystore::Compute::Twice)
        .await
        .map_err(|_| Error::Generic)
}

pub fn hash160(data: &[u8]) -> [u8; 20] {
    hash160::Hash::hash(data).to_byte_array()
}

pub fn base58check_with_alphabet(payload: &[u8], alphabet: &[u8; 58]) -> String {
    let checksum = sha256d::Hash::hash(payload).to_byte_array();
    let mut data = Vec::with_capacity(payload.len() + 4);
    data.extend_from_slice(payload);
    data.extend_from_slice(&checksum[..4]);
    base58_encode(&data, alphabet)
}

pub fn base58check_decode_with_alphabet(
    encoded: &str,
    alphabet: &[u8; 58],
) -> Result<Vec<u8>, Error> {
    let decoded = base58_decode(encoded, alphabet)?;
    if decoded.len() < 5 {
        return Err(Error::InvalidInput);
    }
    let payload_len = decoded.len() - 4;
    let checksum = sha256d::Hash::hash(&decoded[..payload_len]).to_byte_array();
    if decoded[payload_len..] != checksum[..4] {
        return Err(Error::InvalidInput);
    }
    Ok(decoded[..payload_len].to_vec())
}

fn base58_encode(data: &[u8], alphabet: &[u8; 58]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let zeroes = data.iter().take_while(|&&b| b == 0).count();
    let mut digits = vec![0u8; data.len() * 138 / 100 + 1];
    let mut length = 0usize;
    for &byte in data {
        let mut carry = byte as u32;
        for digit in digits.iter_mut().take(length) {
            carry += (*digit as u32) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits[length] = (carry % 58) as u8;
            length += 1;
            carry /= 58;
        }
    }
    let mut out = String::with_capacity(zeroes + length);
    for _ in 0..zeroes {
        out.push(alphabet[0] as char);
    }
    for &digit in digits.iter().take(length).rev() {
        out.push(alphabet[digit as usize] as char);
    }
    out
}

fn base58_decode(encoded: &str, alphabet: &[u8; 58]) -> Result<Vec<u8>, Error> {
    if encoded.is_empty() || !encoded.is_ascii() {
        return Err(Error::InvalidInput);
    }
    let zeroes = encoded
        .as_bytes()
        .iter()
        .take_while(|&&b| b == alphabet[0])
        .count();
    let mut bytes = vec![0u8; encoded.len()];
    let mut length = 0usize;
    for &ch in encoded.as_bytes() {
        let value = alphabet
            .iter()
            .position(|&candidate| candidate == ch)
            .ok_or(Error::InvalidInput)? as u32;
        let mut carry = value;
        for byte in bytes.iter_mut().take(length) {
            carry += (*byte as u32) * 58;
            *byte = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes[length] = (carry & 0xff) as u8;
            length += 1;
            carry >>= 8;
        }
    }
    let mut out = Vec::with_capacity(zeroes + length);
    out.resize(zeroes, 0);
    out.extend(bytes[..length].iter().rev());
    Ok(out)
}

pub fn format_amount(value: u64, decimals: usize, unit: &str) -> String {
    format!("{} {}", util::decimal::format(value, decimals), unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BITCOIN_ALPHABET: &[u8; 58] =
        b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

    #[test]
    fn test_base58_roundtrip() {
        for data in [b"\0\0hello".as_slice(), b"hello", b"\0", b"\xff\0\x01"] {
            let encoded = base58_encode(data, BITCOIN_ALPHABET);
            assert_eq!(base58_decode(&encoded, BITCOIN_ALPHABET).unwrap(), data);
        }
    }

    #[test]
    fn test_base58check() {
        let encoded = base58check_with_alphabet(&[0; 21], BITCOIN_ALPHABET);
        assert_eq!(encoded, "1111111111111111111114oLvT2");
        assert_eq!(
            base58check_decode_with_alphabet(&encoded, BITCOIN_ALPHABET).unwrap(),
            vec![0; 21]
        );
    }
}
