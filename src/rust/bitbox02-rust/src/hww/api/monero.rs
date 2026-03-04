// SPDX-License-Identifier: Apache-2.0

#[cfg(not(feature = "app-monero"))]
compile_error!("Monero code is being compiled even though the app-monero feature is not enabled");

use super::Error;
use super::pb;
use crate::hal::Ui;
use crate::hal::ui::ConfirmParams;
use alloc::string::String;
use alloc::vec::Vec;
use pb::xmr_request::Request;
use pb::xmr_response::Response;
use sha3::digest::Digest;
use util::bip32::HARDENED;

const BIP44_PURPOSE: u32 = 44 + HARDENED;
const BIP44_COIN_MONERO: u32 = 128 + HARDENED;
const BIP44_ACCOUNT_MIN: u32 = HARDENED;
const BIP44_ACCOUNT_MAX: u32 = HARDENED + 99;
const BIP44_CHANGE_RECEIVE: u32 = 0;
const BIP44_ADDRESS_MAX: u32 = 9999;

const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const ENCODED_BLOCK_SIZES: [usize; 9] = [0, 2, 3, 5, 6, 7, 9, 10, 11];

struct Params {
    name: &'static str,
    address_prefix: u8,
}

fn params(network: pb::XmrNetwork) -> Params {
    match network {
        pb::XmrNetwork::XmrMainnet => Params {
            name: "Monero",
            address_prefix: 18,
        },
        pb::XmrNetwork::XmrTestnet => Params {
            name: "Monero Testnet",
            address_prefix: 53,
        },
        pb::XmrNetwork::XmrStagenet => Params {
            name: "Monero Stagenet",
            address_prefix: 24,
        },
    }
}

fn validate_address_keypath(keypath: &[u32]) -> Result<(), Error> {
    if let &[
        BIP44_PURPOSE,
        BIP44_COIN_MONERO,
        account,
        BIP44_CHANGE_RECEIVE,
        address,
    ] = keypath
    {
        if !(BIP44_ACCOUNT_MIN..=BIP44_ACCOUNT_MAX).contains(&account) {
            return Err(Error::InvalidInput);
        }
        if address > BIP44_ADDRESS_MAX {
            return Err(Error::InvalidInput);
        }
        return Ok(());
    }
    Err(Error::InvalidInput)
}

fn validate_keypaths(spend_keypath: &[u32], view_keypath: &[u32]) -> Result<(), Error> {
    validate_address_keypath(spend_keypath)?;
    validate_address_keypath(view_keypath)?;
    if spend_keypath[..4] != view_keypath[..4] {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

fn encode_block(block: &[u8]) -> String {
    let mut n = 0u64;
    for byte in block {
        n = (n << 8) | *byte as u64;
    }

    let expected_len = ENCODED_BLOCK_SIZES[block.len()];
    let mut tmp = [0u8; ENCODED_BLOCK_SIZES[8]];
    let mut i = tmp.len();
    while n > 0 {
        i -= 1;
        tmp[i] = BASE58_ALPHABET[(n % 58) as usize];
        n /= 58;
    }
    let encoded_len = tmp.len() - i;

    let mut result = String::with_capacity(expected_len);
    for _ in encoded_len..expected_len {
        result.push('1');
    }
    for ch in &tmp[i..] {
        result.push(*ch as char);
    }
    result
}

fn encode_monero_base58(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(8) {
        out.push_str(&encode_block(chunk));
    }
    out
}

fn make_address(params: &Params, spend_pubkey: &[u8], view_pubkey: &[u8]) -> Result<String, Error> {
    if spend_pubkey.len() != 32 || view_pubkey.len() != 32 {
        return Err(Error::InvalidInput);
    }
    let mut payload = Vec::with_capacity(1 + 32 + 32);
    payload.push(params.address_prefix);
    payload.extend_from_slice(spend_pubkey);
    payload.extend_from_slice(view_pubkey);
    let checksum = sha3::Keccak256::digest(&payload);

    let mut address_data = Vec::with_capacity(payload.len() + 4);
    address_data.extend_from_slice(&payload);
    address_data.extend_from_slice(&checksum[..4]);
    Ok(encode_monero_base58(&address_data))
}

async fn process_address(
    hal: &mut impl crate::hal::Hal,
    request: &pb::XmrAddressRequest,
) -> Result<Response, Error> {
    let network = pb::XmrNetwork::try_from(request.network)?;
    let params = params(network);
    validate_keypaths(&request.spend_keypath, &request.view_keypath)?;

    let spend_xpub = crate::keystore::ed25519::get_xpub(hal, &request.spend_keypath)?;
    let view_xpub = crate::keystore::ed25519::get_xpub(hal, &request.view_keypath)?;
    let address = make_address(&params, spend_xpub.pubkey_bytes(), view_xpub.pubkey_bytes())?;

    if request.display {
        let displayed_address = util::strings::format_address(&address);
        hal.ui()
            .confirm(&ConfirmParams {
                title: params.name,
                body: &displayed_address,
                scrollable: true,
                ..Default::default()
            })
            .await?;
    }

    Ok(Response::Pub(pb::PubResponse { r#pub: address }))
}

/// Handle a Monero protobuf api call.
pub async fn process_api(
    hal: &mut impl crate::hal::Hal,
    request: &Request,
) -> Result<Response, Error> {
    match request {
        Request::Address(request) => process_address(hal, request).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::testing::TestingHal;
    use crate::keystore::testing::mock_unlocked;

    #[test]
    fn test_encode_block() {
        assert_eq!(encode_block(&[0]), "11");
        assert_eq!(encode_block(&[255]), "5Q");
        assert_eq!(encode_block(&[1, 2, 3, 4, 5, 6, 7, 8]), "1An6UebxCZd");
    }

    #[test]
    fn test_validate_keypaths() {
        assert!(
            validate_keypaths(
                &[44 + HARDENED, 128 + HARDENED, HARDENED, 0, 0],
                &[44 + HARDENED, 128 + HARDENED, HARDENED, 0, 1]
            )
            .is_ok()
        );
        assert!(
            validate_keypaths(
                &[44 + HARDENED, 128 + HARDENED, HARDENED, 1, 0],
                &[44 + HARDENED, 128 + HARDENED, HARDENED, 0, 1]
            )
            .is_err()
        );
    }

    #[test]
    fn test_process_address() {
        mock_unlocked();
        let response = util::bb02_async::block_on(process_api(
            &mut TestingHal::new(),
            &Request::Address(pb::XmrAddressRequest {
                network: pb::XmrNetwork::XmrMainnet as i32,
                display: false,
                spend_keypath: vec![44 + HARDENED, 128 + HARDENED, HARDENED, 0, 0],
                view_keypath: vec![44 + HARDENED, 128 + HARDENED, HARDENED, 0, 1],
            }),
        ))
        .unwrap();
        match response {
            Response::Pub(pb::PubResponse { r#pub }) => {
                assert!(r#pub.starts_with('4'));
                assert_eq!(r#pub.len(), 95);
            }
        }
    }
}
