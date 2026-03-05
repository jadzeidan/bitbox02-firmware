// SPDX-License-Identifier: Apache-2.0

#[cfg(not(feature = "app-monero"))]
compile_error!("Monero code is being compiled even though the app-monero feature is not enabled");

use super::Error;
use super::pb;
use crate::hal::Ui;
use crate::hal::ui::ConfirmParams;
use crate::workflow::transaction;
use crate::workflow::verify_message;
use alloc::string::String;
use alloc::vec::Vec;
use core::convert::TryInto;
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
const MAX_MESSAGE_SIZE: usize = 1024;

const BASE58_ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
const ENCODED_BLOCK_SIZES: [usize; 9] = [0, 2, 3, 5, 6, 7, 9, 10, 11];

struct Params {
    name: &'static str,
    address_prefix_standard: u8,
    address_prefix_integrated: u8,
    address_prefix_subaddress: u8,
    unit: &'static str,
}

fn params(network: pb::XmrNetwork) -> Params {
    match network {
        pb::XmrNetwork::XmrMainnet => Params {
            name: "Monero",
            address_prefix_standard: 18,
            address_prefix_integrated: 19,
            address_prefix_subaddress: 42,
            unit: "XMR",
        },
        pb::XmrNetwork::XmrTestnet => Params {
            name: "Monero Testnet",
            address_prefix_standard: 53,
            address_prefix_integrated: 54,
            address_prefix_subaddress: 63,
            unit: "TXMR",
        },
        pb::XmrNetwork::XmrStagenet => Params {
            name: "Monero Stagenet",
            address_prefix_standard: 24,
            address_prefix_integrated: 25,
            address_prefix_subaddress: 36,
            unit: "SXMR",
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

fn format_amount(unit: &str, piconero: u64) -> String {
    let mut out = util::decimal::format_no_trim(piconero, 12);
    out.push(' ');
    out.push_str(unit);
    out
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
    payload.push(params.address_prefix_standard);
    payload.extend_from_slice(spend_pubkey);
    payload.extend_from_slice(view_pubkey);
    let checksum = sha3::Keccak256::digest(&payload);

    let mut address_data = Vec::with_capacity(payload.len() + 4);
    address_data.extend_from_slice(&payload);
    address_data.extend_from_slice(&checksum[..4]);
    Ok(encode_monero_base58(&address_data))
}

fn decoded_block_size(encoded_len: usize) -> Option<usize> {
    match encoded_len {
        2 => Some(1),
        3 => Some(2),
        5 => Some(3),
        6 => Some(4),
        7 => Some(5),
        9 => Some(6),
        10 => Some(7),
        11 => Some(8),
        _ => None,
    }
}

fn decode_base58_char(c: u8) -> Result<u8, Error> {
    for (index, a) in BASE58_ALPHABET.iter().enumerate() {
        if *a == c {
            return Ok(index as u8);
        }
    }
    Err(Error::InvalidInput)
}

fn decode_block(block: &[u8]) -> Result<Vec<u8>, Error> {
    let decoded_len = decoded_block_size(block.len()).ok_or(Error::InvalidInput)?;
    let mut value: u128 = 0;
    for c in block {
        let digit = decode_base58_char(*c)? as u128;
        value = value
            .checked_mul(58)
            .and_then(|v| v.checked_add(digit))
            .ok_or(Error::InvalidInput)?;
    }

    let max_value = 1u128 << (8 * decoded_len);
    if value >= max_value {
        return Err(Error::InvalidInput);
    }

    let mut out = vec![0u8; decoded_len];
    for i in (0..decoded_len).rev() {
        out[i] = (value & 0xff) as u8;
        value >>= 8;
    }
    Ok(out)
}

fn decode_monero_base58(input: &str) -> Result<Vec<u8>, Error> {
    let input = input.as_bytes();
    if input.is_empty() {
        return Err(Error::InvalidInput);
    }
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < input.len() {
        let remaining = input.len() - offset;
        let block_len = if remaining > 11 { 11 } else { remaining };
        out.extend_from_slice(&decode_block(&input[offset..offset + block_len])?);
        offset += block_len;
    }
    Ok(out)
}

fn validate_destination_address(address: &str, params: &Params) -> Result<(), Error> {
    if address.is_empty() || address.len() > 200 {
        return Err(Error::InvalidInput);
    }
    let decoded = decode_monero_base58(address)?;
    if decoded.len() != 69 && decoded.len() != 77 {
        return Err(Error::InvalidInput);
    }
    if decoded.len() < 5 {
        return Err(Error::InvalidInput);
    }
    let prefix = decoded[0];
    if decoded.len() == 69
        && prefix != params.address_prefix_standard
        && prefix != params.address_prefix_subaddress
    {
        return Err(Error::InvalidInput);
    }
    if decoded.len() == 77 && prefix != params.address_prefix_integrated {
        return Err(Error::InvalidInput);
    }

    let checksum_index = decoded.len() - 4;
    let checksum = &decoded[checksum_index..];
    let expected = sha3::Keccak256::digest(&decoded[..checksum_index]);
    if checksum != &expected[..4] {
        return Err(Error::InvalidInput);
    }
    Ok(())
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

async fn process_sign_transaction(
    hal: &mut impl crate::hal::Hal,
    request: &pb::XmrSignTransactionRequest,
) -> Result<Response, Error> {
    let network = pb::XmrNetwork::try_from(request.network)?;
    let params = params(network);
    validate_address_keypath(&request.spend_keypath)?;
    let sighash: [u8; 32] = request
        .sighash
        .as_slice()
        .try_into()
        .or(Err(Error::InvalidInput))?;

    if request.outputs.is_empty() {
        return Err(Error::InvalidInput);
    }

    let mut outputs_sum: u64 = 0;
    for output in &request.outputs {
        validate_destination_address(&output.destination_address, &params)?;
        outputs_sum = outputs_sum
            .checked_add(output.amount)
            .ok_or(Error::InvalidInput)?;
        hal.ui()
            .verify_recipient(
                &util::strings::format_address(&output.destination_address),
                &format_amount(params.unit, output.amount),
            )
            .await?;
    }

    let total = outputs_sum
        .checked_add(request.fee)
        .ok_or(Error::InvalidInput)?;

    let fee_percentage = if outputs_sum == 0 {
        None
    } else {
        Some(100.0 * (request.fee as f64) / (outputs_sum as f64))
    };
    transaction::verify_total_fee_maybe_warn(
        hal,
        &format_amount(params.unit, total),
        &format_amount(params.unit, request.fee),
        fee_percentage,
    )
    .await?;

    hal.ui()
        .confirm(&ConfirmParams {
            title: "Sign Monero tx",
            body: "Proceed with signing?",
            longtouch: true,
            ..Default::default()
        })
        .await?;

    let sign_result = crate::keystore::ed25519::sign(hal, &request.spend_keypath, &sighash)?;
    Ok(Response::SignTransaction(pb::XmrSignTransactionResponse {
        signature: sign_result.signature.to_vec(),
        public_key: sign_result.public_key.to_bytes().to_vec(),
    }))
}

async fn process_sign_message(
    hal: &mut impl crate::hal::Hal,
    request: &pb::XmrSignMessageRequest,
) -> Result<Response, Error> {
    let network = pb::XmrNetwork::try_from(request.network)?;
    let params = params(network);
    validate_keypaths(&request.spend_keypath, &request.view_keypath)?;
    if request.msg.is_empty() || request.msg.len() > MAX_MESSAGE_SIZE {
        return Err(Error::InvalidInput);
    }

    let spend_xpub = crate::keystore::ed25519::get_xpub(hal, &request.spend_keypath)?;
    let view_xpub = crate::keystore::ed25519::get_xpub(hal, &request.view_keypath)?;
    let address = make_address(&params, spend_xpub.pubkey_bytes(), view_xpub.pubkey_bytes())?;

    hal.ui()
        .confirm(&ConfirmParams {
            title: params.name,
            body: &util::strings::format_address(&address),
            scrollable: true,
            accept_is_nextarrow: true,
            ..Default::default()
        })
        .await?;

    verify_message::verify(hal, "Sign message", "Sign", &request.msg, true).await?;

    // Prefix and hash to obtain a 32-byte digest compatible with our ed25519 signer.
    let mut msg: Vec<u8> = Vec::new();
    msg.extend(b"\x18Monero Signed Message:\n");
    msg.extend(format!("{}", request.msg.len()).as_bytes());
    msg.extend(&request.msg);
    let sighash: [u8; 32] = sha3::Keccak256::digest(&msg).into();

    let sign_result = crate::keystore::ed25519::sign(hal, &request.spend_keypath, &sighash)?;
    Ok(Response::SignMessage(pb::XmrSignMessageResponse {
        signature: sign_result.signature.to_vec(),
        public_key: sign_result.public_key.to_bytes().to_vec(),
    }))
}

/// Handle a Monero protobuf api call.
pub async fn process_api(
    hal: &mut impl crate::hal::Hal,
    request: &Request,
) -> Result<Response, Error> {
    match request {
        Request::Address(request) => process_address(hal, request).await,
        Request::SignTransaction(request) => process_sign_transaction(hal, request).await,
        Request::SignMessage(request) => process_sign_message(hal, request).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hal::testing::TestingHal;
    use crate::hal::testing::ui::Screen;
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
    fn test_format_amount() {
        assert_eq!(format_amount("XMR", 0), "0.000000000000 XMR");
        assert_eq!(
            format_amount("XMR", 1_250_000_000_000),
            "1.250000000000 XMR"
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
            _ => panic!("wrong response type"),
        }
    }

    fn get_valid_mainnet_address() -> String {
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
            Response::Pub(pb::PubResponse { r#pub }) => r#pub,
            _ => panic!("wrong response type"),
        }
    }

    #[test]
    fn test_process_sign_transaction() {
        mock_unlocked();
        let destination = get_valid_mainnet_address();
        let response = util::bb02_async::block_on(process_api(
            &mut TestingHal::new(),
            &Request::SignTransaction(pb::XmrSignTransactionRequest {
                network: pb::XmrNetwork::XmrMainnet as i32,
                spend_keypath: vec![44 + HARDENED, 128 + HARDENED, HARDENED, 0, 0],
                sighash: [7u8; 32].to_vec(),
                outputs: vec![pb::xmr_sign_transaction_request::Output {
                    destination_address: destination,
                    amount: 1_200_000_000_000,
                }],
                fee: 30_000_000_000,
            }),
        ))
        .unwrap();
        match response {
            Response::SignTransaction(pb::XmrSignTransactionResponse {
                signature,
                public_key,
            }) => {
                assert_eq!(signature.len(), 64);
                assert_eq!(public_key.len(), 32);
            }
            _ => panic!("wrong response type"),
        }
    }

    #[test]
    fn test_validate_destination_address() {
        let mainnet = params(pb::XmrNetwork::XmrMainnet);
        let valid_standard = get_valid_mainnet_address();
        assert!(validate_destination_address(&valid_standard, &mainnet).is_ok());

        let mut invalid_checksum = valid_standard.as_bytes().to_vec();
        let last = invalid_checksum.len() - 1;
        invalid_checksum[last] = b'1';
        let invalid_checksum = core::str::from_utf8(&invalid_checksum).unwrap();
        assert!(validate_destination_address(invalid_checksum, &mainnet).is_err());

        let testnet = params(pb::XmrNetwork::XmrTestnet);
        assert!(validate_destination_address(&valid_standard, &testnet).is_err());
    }

    #[test]
    fn test_process_sign_message() {
        mock_unlocked();
        let mut mock_hal = TestingHal::new();
        let response = util::bb02_async::block_on(process_api(
            &mut mock_hal,
            &Request::SignMessage(pb::XmrSignMessageRequest {
                spend_keypath: vec![44 + HARDENED, 128 + HARDENED, HARDENED, 0, 0],
                view_keypath: vec![44 + HARDENED, 128 + HARDENED, HARDENED, 0, 1],
                msg: b"hello monero".to_vec(),
                network: pb::XmrNetwork::XmrMainnet as i32,
            }),
        ))
        .unwrap();
        match response {
            Response::SignMessage(pb::XmrSignMessageResponse {
                signature,
                public_key,
            }) => {
                assert_eq!(signature.len(), 64);
                assert_eq!(public_key.len(), 32);
            }
            _ => panic!("wrong response type"),
        }
        assert_eq!(
            mock_hal.ui.screens,
            vec![
                Screen::Confirm {
                    title: "Monero".into(),
                    body: util::strings::format_address(&get_valid_mainnet_address()),
                    longtouch: false,
                },
                Screen::Confirm {
                    title: "Sign message".into(),
                    body: "hello monero".into(),
                    longtouch: true,
                },
            ]
        );
    }

    #[test]
    fn test_process_sign_message_invalid() {
        mock_unlocked();
        let response = util::bb02_async::block_on(process_api(
            &mut TestingHal::new(),
            &Request::SignMessage(pb::XmrSignMessageRequest {
                spend_keypath: vec![44 + HARDENED, 128 + HARDENED, HARDENED, 0, 0],
                view_keypath: vec![44 + HARDENED, 128 + HARDENED, HARDENED, 0, 1],
                msg: Vec::new(),
                network: pb::XmrNetwork::XmrMainnet as i32,
            }),
        ));
        assert_eq!(response, Err(Error::InvalidInput));
    }

    #[test]
    fn test_process_sign_message_invalid_keypaths() {
        mock_unlocked();
        let response = util::bb02_async::block_on(process_api(
            &mut TestingHal::new(),
            &Request::SignMessage(pb::XmrSignMessageRequest {
                spend_keypath: vec![44 + HARDENED, 128 + HARDENED, HARDENED, 0, 0],
                view_keypath: vec![44 + HARDENED, 128 + HARDENED, HARDENED + 1, 0, 1],
                msg: b"hello".to_vec(),
                network: pb::XmrNetwork::XmrMainnet as i32,
            }),
        ));
        assert_eq!(response, Err(Error::InvalidInput));
    }
}
