// SPDX-License-Identifier: Apache-2.0

#[cfg(not(feature = "app-xrp"))]
compile_error!("XRP code is being compiled even though the app-xrp feature is not enabled");

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use bitcoin::hashes::{Hash, sha512};

use crate::hal::Ui;
use crate::hal::ui::ConfirmParams;
use crate::pb;

use super::Error;
use pb::xrp_request::Request;
use pb::xrp_response::Response;

const RIPPLE_ALPHABET: &[u8; 58] = b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz";
const MAX_XRP_DROPS: u64 = 100_000_000_000_000_000;
const MAX_MEMO_SIZE: usize = 256;
const TF_FULLY_CANONICAL_SIG: u32 = 0x8000_0000;

fn validate_keypath(keypath: &[u32]) -> Result<(), Error> {
    super::altcoin::validate_bip44_keypath(keypath, 144)
}

fn encode_address(account_id: &[u8; 20]) -> String {
    let mut payload = [0u8; 21];
    payload[1..].copy_from_slice(account_id);
    super::altcoin::base58check_with_alphabet(&payload, RIPPLE_ALPHABET)
}

fn decode_address(address: &str) -> Result<[u8; 20], Error> {
    let decoded = super::altcoin::base58check_decode_with_alphabet(address, RIPPLE_ALPHABET)?;
    if decoded.len() != 21 || decoded[0] != 0 {
        return Err(Error::InvalidInput);
    }
    decoded[1..].try_into().map_err(|_| Error::InvalidInput)
}

async fn derive_address(
    hal: &mut impl crate::hal::Hal,
    keypath: &[u32],
) -> Result<(String, [u8; 20], [u8; 33]), Error> {
    let xpub = super::altcoin::secp256k1_pubkey(hal, keypath).await?;
    let public_key: [u8; 33] = xpub.public_key().try_into().map_err(|_| Error::Generic)?;
    let account_id = super::altcoin::hash160(&public_key);
    Ok((encode_address(&account_id), account_id, public_key))
}

async fn process_pub(
    hal: &mut impl crate::hal::Hal,
    request: &pb::XrpPubRequest,
) -> Result<Response, Error> {
    validate_keypath(&request.keypath)?;
    let (address, _, _) = derive_address(hal, &request.keypath).await?;
    if request.display {
        hal.ui()
            .confirm(&ConfirmParams {
                title: "XRP",
                body: &address,
                scrollable: true,
                ..Default::default()
            })
            .await?;
    }
    Ok(Response::Pub(pb::PubResponse { r#pub: address }))
}

fn serialize_variable_length(len: usize, out: &mut Vec<u8>) -> Result<(), Error> {
    match len {
        0..=192 => out.push(len as u8),
        193..=12_480 => {
            let adjusted = len - 193;
            out.push((193 + (adjusted >> 8)) as u8);
            out.push((adjusted & 0xff) as u8);
        }
        12_481..=918_744 => {
            let adjusted = len - 12_481;
            out.push((241 + (adjusted >> 16)) as u8);
            out.push(((adjusted >> 8) & 0xff) as u8);
            out.push((adjusted & 0xff) as u8);
        }
        _ => return Err(Error::InvalidInput),
    }
    Ok(())
}

fn serialize_blob(field_id: u8, value: &[u8], out: &mut Vec<u8>) -> Result<(), Error> {
    out.push(field_id);
    serialize_variable_length(value.len(), out)?;
    out.extend_from_slice(value);
    Ok(())
}

fn serialize_account(field_id: u8, account: &[u8; 20], out: &mut Vec<u8>) {
    out.push(field_id);
    out.push(20);
    out.extend_from_slice(account);
}

fn serialize_payment(
    request: &pb::XrpSignPaymentRequest,
    source: &[u8; 20],
    destination: &[u8; 20],
    public_key: &[u8; 33],
    signature: Option<&[u8]>,
) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x12, 0x00, 0x00]); // TransactionType: Payment
    out.push(0x22); // Flags
    out.extend_from_slice(&TF_FULLY_CANONICAL_SIG.to_be_bytes());
    out.push(0x24); // Sequence
    out.extend_from_slice(&request.sequence.to_be_bytes());
    if let Some(destination_tag) = request.destination_tag {
        out.push(0x2e);
        out.extend_from_slice(&destination_tag.to_be_bytes());
    }
    if let Some(last_ledger_sequence) = request.last_ledger_sequence {
        out.extend_from_slice(&[0x20, 0x1b]);
        out.extend_from_slice(&last_ledger_sequence.to_be_bytes());
    }

    out.push(0x61); // Amount
    out.extend_from_slice(&(request.amount | 0x4000_0000_0000_0000).to_be_bytes());
    out.push(0x68); // Fee
    out.extend_from_slice(&(request.fee | 0x4000_0000_0000_0000).to_be_bytes());
    serialize_blob(0x73, public_key, &mut out)?; // SigningPubKey
    if let Some(signature) = signature {
        serialize_blob(0x74, signature, &mut out)?; // TxnSignature
    }
    serialize_account(0x81, source, &mut out);
    serialize_account(0x83, destination, &mut out);
    if !request.memo.is_empty() {
        out.extend_from_slice(&[0xf9, 0xea, 0x7d]); // Memos, Memo, MemoData
        serialize_variable_length(request.memo.len(), &mut out)?;
        out.extend_from_slice(&request.memo);
        out.extend_from_slice(&[0xe1, 0xf1]); // ObjectEnd, ArrayEnd
    }
    Ok(out)
}

async fn process_sign(
    hal: &mut impl crate::hal::Hal,
    request: &pb::XrpSignPaymentRequest,
) -> Result<Response, Error> {
    validate_keypath(&request.keypath)?;
    let network = pb::XrpNetwork::try_from(request.network)?;
    if request.amount == 0
        || request.amount > MAX_XRP_DROPS
        || request.fee == 0
        || request.fee > MAX_XRP_DROPS
        || request.sequence == 0
        || request.memo.len() > MAX_MEMO_SIZE
    {
        return Err(Error::InvalidInput);
    }
    let memo = if request.memo.is_empty() {
        None
    } else {
        Some(core::str::from_utf8(&request.memo).map_err(|_| Error::InvalidInput)?)
    };
    let destination = decode_address(&request.destination)?;
    let (_, source, public_key) = derive_address(hal, &request.keypath).await?;
    if source == destination {
        return Err(Error::InvalidInput);
    }

    if network == pb::XrpNetwork::XrpTestnet {
        hal.ui()
            .confirm(&ConfirmParams {
                title: "Warning",
                body: "XRP testnet",
                accept_is_nextarrow: true,
                ..Default::default()
            })
            .await?;
    }
    hal.ui()
        .verify_recipient(
            &request.destination,
            &super::altcoin::format_amount(request.amount, 6, "XRP"),
        )
        .await?;
    if let Some(destination_tag) = request.destination_tag {
        hal.ui()
            .confirm(&ConfirmParams {
                title: "Destination tag",
                body: &destination_tag.to_string(),
                accept_is_nextarrow: true,
                ..Default::default()
            })
            .await?;
    }
    if let Some(memo) = memo {
        hal.ui()
            .confirm(&ConfirmParams {
                title: "Memo",
                body: memo,
                scrollable: true,
                accept_is_nextarrow: true,
                ..Default::default()
            })
            .await?;
    }
    let total = request
        .amount
        .checked_add(request.fee)
        .ok_or(Error::InvalidInput)?;
    crate::workflow::transaction::verify_total_fee_maybe_warn(
        hal,
        &super::altcoin::format_amount(total, 6, "XRP"),
        &super::altcoin::format_amount(request.fee, 6, "XRP"),
        Some(100.0 * request.fee as f64 / request.amount as f64),
    )
    .await?;
    hal.ui().status("Transaction\nconfirmed", true).await;

    let unsigned = serialize_payment(request, &source, &destination, &public_key, None)?;
    let mut signing_data = Vec::with_capacity(unsigned.len() + 4);
    signing_data.extend_from_slice(b"STX\0");
    signing_data.extend_from_slice(&unsigned);
    let full_hash = sha512::Hash::hash(&signing_data).to_byte_array();
    let digest: [u8; 32] = full_hash[..32].try_into().unwrap();
    let private_key = crate::keystore::secp256k1_get_private_key(hal, &request.keypath).await?;
    let sign_result = crate::secp256k1::secp256k1_sign(
        private_key
            .as_slice()
            .try_into()
            .map_err(|_| Error::Generic)?,
        &digest,
        None,
    )?;
    let signature = bitcoin::secp256k1::ecdsa::Signature::from_compact(&sign_result.signature)
        .map_err(|_| Error::Generic)?
        .serialize_der()
        .to_vec();
    let serialized_transaction = serialize_payment(
        request,
        &source,
        &destination,
        &public_key,
        Some(&signature),
    )?;

    Ok(Response::SignPayment(pb::XrpSignPaymentResponse {
        signature,
        serialized_transaction,
    }))
}

pub async fn process_api(
    hal: &mut impl crate::hal::Hal,
    request: &Request,
) -> Result<Response, Error> {
    match request {
        Request::Pub(request) => process_pub(hal, request).await,
        Request::SignPayment(request) => process_sign(hal, request).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_lit::hex;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[async_test::test]
    async fn test_address() {
        crate::keystore::testing::mock_unlocked_using_mnemonic(MNEMONIC, "");
        let (address, _, _) = derive_address(
            &mut crate::hal::testing::TestingHal::new(),
            &[
                44 + util::bip32::HARDENED,
                144 + util::bip32::HARDENED,
                util::bip32::HARDENED,
                0,
                0,
            ],
        )
        .await
        .unwrap();
        assert_eq!(address, "rHsMGQEkVNJmpGWs8XUBoTBiAAbwxZN5v3");
    }

    #[test]
    fn test_address_codec() {
        let address = "rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN";
        let account_id = decode_address(address).unwrap();
        assert_eq!(encode_address(&account_id), address);
        assert!(decode_address("rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErM").is_err());
    }

    #[test]
    fn test_variable_length() {
        let mut out = Vec::new();
        serialize_variable_length(192, &mut out).unwrap();
        assert_eq!(out, vec![192]);
        out.clear();
        serialize_variable_length(193, &mut out).unwrap();
        assert_eq!(out, vec![193, 0]);
        out.clear();
        serialize_variable_length(12_481, &mut out).unwrap();
        assert_eq!(out, vec![241, 0, 0]);
    }

    #[test]
    fn test_serialize_payment() {
        let source = decode_address("rJrRMgiRgrU6hDF4pgu5DXQdWyPbY35ErN").unwrap();
        let destination = decode_address("r9cZA1mLK5R5Am25ArfXFmqgNwjZgnfk59").unwrap();
        let request = pb::XrpSignPaymentRequest {
            network: pb::XrpNetwork::XrpMainnet as i32,
            keypath: vec![],
            destination: "r9cZA1mLK5R5Am25ArfXFmqgNwjZgnfk59".into(),
            amount: 1_000_000,
            fee: 12,
            sequence: 1,
            destination_tag: Some(42),
            last_ledger_sequence: Some(100),
            memo: b"hello".to_vec(),
        };
        let public_key = hex!("0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798");
        assert_eq!(
            serialize_payment(&request, &source, &destination, &public_key, None).unwrap(),
            hex!(
                "120000228000000024000000012e0000002a201b000000646140000000000f424068400000000000000c73210279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f817988114ba8e78626ee42c41b46d46c3048df3a1c3c8707283145e7b112523f68d2f5e879db4eac51c6698a69304f9ea7d0568656c6c6fe1f1"
            )
        );
    }
}
