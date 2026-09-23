// SPDX-License-Identifier: Apache-2.0

#[cfg(not(feature = "app-zcash"))]
compile_error!("Zcash code is being compiled even though the app-zcash feature is not enabled");

use alloc::string::String;
use alloc::vec::Vec;

use crate::hal::Ui;
use crate::hal::ui::ConfirmParams;
use crate::pb;

use super::Error;
use pb::zcash_request::Request;
use pb::zcash_response::Response;

const MAX_MONEY: u64 = 21_000_000 * 100_000_000;
const MAX_INPUTS: usize = 64;
const MAX_OUTPUTS: usize = 64;
const SIGHASH_ALL: u8 = 1;
const VERSION: u32 = 0x8000_0005;
const VERSION_GROUP_ID: u32 = 0x26a7_270a;

const KNOWN_BRANCH_IDS: &[u32] = &[
    0xc2d6_d0b4, // NU5
    0xc8e7_1055, // NU6
    0x4dec_4df0, // NU6.1
    0x5437_f330, // NU6.2
    0x37a5_165b, // NU6.3
];

fn validate_keypath(keypath: &[u32]) -> Result<(), Error> {
    super::altcoin::validate_bip44_keypath(keypath, 133)
}

fn address_prefix(network: pb::ZcashNetwork, p2sh: bool) -> [u8; 2] {
    match (network, p2sh) {
        (pb::ZcashNetwork::ZcashMainnet, false) => [0x1c, 0xb8],
        (pb::ZcashNetwork::ZcashMainnet, true) => [0x1c, 0xbd],
        (pb::ZcashNetwork::ZcashTestnet, false) => [0x1d, 0x25],
        (pb::ZcashNetwork::ZcashTestnet, true) => [0x1c, 0xba],
    }
}

fn encode_address(network: pb::ZcashNetwork, hash: &[u8; 20], p2sh: bool) -> String {
    let mut payload = [0u8; 22];
    payload[..2].copy_from_slice(&address_prefix(network, p2sh));
    payload[2..].copy_from_slice(hash);
    bitcoin::base58::encode_check(&payload)
}

fn p2pkh_script(public_key_hash: &[u8; 20]) -> [u8; 25] {
    let mut script = [0u8; 25];
    script[..3].copy_from_slice(&[0x76, 0xa9, 0x14]);
    script[3..23].copy_from_slice(public_key_hash);
    script[23..].copy_from_slice(&[0x88, 0xac]);
    script
}

fn parse_output_address(network: pb::ZcashNetwork, script: &[u8]) -> Result<String, Error> {
    if script.len() == 25 && script[..3] == [0x76, 0xa9, 0x14] && script[23..] == [0x88, 0xac] {
        return Ok(encode_address(
            network,
            script[3..23].try_into().unwrap(),
            false,
        ));
    }
    if script.len() == 23 && script[..2] == [0xa9, 0x14] && script[22] == 0x87 {
        return Ok(encode_address(
            network,
            script[2..22].try_into().unwrap(),
            true,
        ));
    }
    Err(Error::InvalidInput)
}

async fn derive(
    hal: &mut impl crate::hal::Hal,
    network: pb::ZcashNetwork,
    keypath: &[u32],
) -> Result<(String, [u8; 20], [u8; 33]), Error> {
    let xpub = super::altcoin::secp256k1_pubkey(hal, keypath).await?;
    let public_key: [u8; 33] = xpub.public_key().try_into().map_err(|_| Error::Generic)?;
    let public_key_hash = super::altcoin::hash160(&public_key);
    Ok((
        encode_address(network, &public_key_hash, false),
        public_key_hash,
        public_key,
    ))
}

async fn process_pub(
    hal: &mut impl crate::hal::Hal,
    request: &pb::ZcashPubRequest,
) -> Result<Response, Error> {
    validate_keypath(&request.keypath)?;
    let network = pb::ZcashNetwork::try_from(request.network)?;
    let (address, _, _) = derive(hal, network, &request.keypath).await?;
    if request.display {
        hal.ui()
            .confirm(&ConfirmParams {
                title: "Zcash",
                body: &address,
                scrollable: true,
                ..Default::default()
            })
            .await?;
    }
    Ok(Response::Pub(pb::PubResponse { r#pub: address }))
}

fn compact_size(value: usize, out: &mut Vec<u8>) {
    match value {
        0..=0xfc => out.push(value as u8),
        0xfd..=0xffff => {
            out.push(0xfd);
            out.extend_from_slice(&(value as u16).to_le_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            out.push(0xfe);
            out.extend_from_slice(&(value as u32).to_le_bytes());
        }
        _ => {
            out.push(0xff);
            out.extend_from_slice(&(value as u64).to_le_bytes());
        }
    }
}

fn field_bytes(value: &[u8], out: &mut Vec<u8>) {
    compact_size(value.len(), out);
    out.extend_from_slice(value);
}

fn blake2b256(personalization: &[u8; 16], data: &[u8]) -> [u8; 32] {
    const IV: [u64; 8] = [
        0x6a09e667f3bcc908,
        0xbb67ae8584caa73b,
        0x3c6ef372fe94f82b,
        0xa54ff53a5f1d36f1,
        0x510e527fade682d1,
        0x9b05688c2b3e6c1f,
        0x1f83d9abfb41bd6b,
        0x5be0cd19137e2179,
    ];
    const SIGMA: [[usize; 16]; 12] = [
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
        [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
        [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
        [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
        [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
        [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
        [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
        [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
        [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    ];

    fn mix(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
        v[d] = (v[d] ^ v[a]).rotate_right(32);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(24);
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
        v[d] = (v[d] ^ v[a]).rotate_right(16);
        v[c] = v[c].wrapping_add(v[d]);
        v[b] = (v[b] ^ v[c]).rotate_right(63);
    }

    fn compress(
        h: &mut [u64; 8],
        block: &[u8; 128],
        count: u128,
        last: bool,
        iv: &[u64; 8],
        sigma: &[[usize; 16]; 12],
    ) {
        let mut message = [0u64; 16];
        for (word, bytes) in message.iter_mut().zip(block.chunks_exact(8)) {
            *word = u64::from_le_bytes(bytes.try_into().unwrap());
        }
        let mut v = [0u64; 16];
        v[..8].copy_from_slice(h);
        v[8..].copy_from_slice(iv);
        v[12] ^= count as u64;
        v[13] ^= (count >> 64) as u64;
        if last {
            v[14] = !v[14];
        }
        for schedule in sigma {
            mix(
                &mut v,
                0,
                4,
                8,
                12,
                message[schedule[0]],
                message[schedule[1]],
            );
            mix(
                &mut v,
                1,
                5,
                9,
                13,
                message[schedule[2]],
                message[schedule[3]],
            );
            mix(
                &mut v,
                2,
                6,
                10,
                14,
                message[schedule[4]],
                message[schedule[5]],
            );
            mix(
                &mut v,
                3,
                7,
                11,
                15,
                message[schedule[6]],
                message[schedule[7]],
            );
            mix(
                &mut v,
                0,
                5,
                10,
                15,
                message[schedule[8]],
                message[schedule[9]],
            );
            mix(
                &mut v,
                1,
                6,
                11,
                12,
                message[schedule[10]],
                message[schedule[11]],
            );
            mix(
                &mut v,
                2,
                7,
                8,
                13,
                message[schedule[12]],
                message[schedule[13]],
            );
            mix(
                &mut v,
                3,
                4,
                9,
                14,
                message[schedule[14]],
                message[schedule[15]],
            );
        }
        for i in 0..8 {
            h[i] ^= v[i] ^ v[i + 8];
        }
    }

    let mut h = IV;
    h[0] ^= 0x0101_0020;
    h[6] ^= u64::from_le_bytes(personalization[..8].try_into().unwrap());
    h[7] ^= u64::from_le_bytes(personalization[8..].try_into().unwrap());
    let mut count = 0u128;
    let mut chunks = data.chunks(128).peekable();
    while let Some(chunk) = chunks.next() {
        let mut block = [0u8; 128];
        block[..chunk.len()].copy_from_slice(chunk);
        count += chunk.len() as u128;
        compress(&mut h, &block, count, chunks.peek().is_none(), &IV, &SIGMA);
    }
    if data.is_empty() {
        compress(&mut h, &[0; 128], 0, true, &IV, &SIGMA);
    }
    let mut out = [0u8; 32];
    for (bytes, word) in out.chunks_exact_mut(8).zip(h) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    out
}

fn serialize_output(output: &pb::zcash_sign_transaction_request::Output, out: &mut Vec<u8>) {
    out.extend_from_slice(&output.value.to_le_bytes());
    field_bytes(&output.script_pubkey, out);
}

fn header_digest(request: &pb::ZcashSignTransactionRequest) -> [u8; 32] {
    let mut data = Vec::with_capacity(20);
    data.extend_from_slice(&VERSION.to_le_bytes());
    data.extend_from_slice(&VERSION_GROUP_ID.to_le_bytes());
    data.extend_from_slice(&request.consensus_branch_id.to_le_bytes());
    data.extend_from_slice(&request.lock_time.to_le_bytes());
    data.extend_from_slice(&request.expiry_height.to_le_bytes());
    blake2b256(b"ZTxIdHeadersHash", &data)
}

fn signature_digest(
    request: &pb::ZcashSignTransactionRequest,
    input_index: usize,
) -> Result<[u8; 32], Error> {
    let mut prevouts = Vec::new();
    let mut amounts = Vec::new();
    let mut scripts = Vec::new();
    let mut sequences = Vec::new();
    for input in &request.inputs {
        if input.prev_out_hash.len() != 32 {
            return Err(Error::InvalidInput);
        }
        prevouts.extend_from_slice(&input.prev_out_hash);
        prevouts.extend_from_slice(&input.prev_out_index.to_le_bytes());
        amounts.extend_from_slice(&input.value.to_le_bytes());
        field_bytes(&input.script_pubkey, &mut scripts);
        sequences.extend_from_slice(&input.sequence.to_le_bytes());
    }
    let mut outputs = Vec::new();
    for output in &request.outputs {
        serialize_output(output, &mut outputs);
    }
    let input = request.inputs.get(input_index).ok_or(Error::InvalidInput)?;
    let mut txin = Vec::new();
    txin.extend_from_slice(&input.prev_out_hash);
    txin.extend_from_slice(&input.prev_out_index.to_le_bytes());
    txin.extend_from_slice(&input.value.to_le_bytes());
    field_bytes(&input.script_pubkey, &mut txin);
    txin.extend_from_slice(&input.sequence.to_le_bytes());

    let mut transparent = Vec::with_capacity(193);
    transparent.push(SIGHASH_ALL);
    transparent.extend_from_slice(&blake2b256(b"ZTxIdPrevoutHash", &prevouts));
    transparent.extend_from_slice(&blake2b256(b"ZTxTrAmountsHash", &amounts));
    transparent.extend_from_slice(&blake2b256(b"ZTxTrScriptsHash", &scripts));
    transparent.extend_from_slice(&blake2b256(b"ZTxIdSequencHash", &sequences));
    transparent.extend_from_slice(&blake2b256(b"ZTxIdOutputsHash", &outputs));
    transparent.extend_from_slice(&blake2b256(b"Zcash___TxInHash", &txin));
    let transparent_digest = blake2b256(b"ZTxIdTranspaHash", &transparent);

    let mut final_data = Vec::with_capacity(128);
    final_data.extend_from_slice(&header_digest(request));
    final_data.extend_from_slice(&transparent_digest);
    final_data.extend_from_slice(&blake2b256(b"ZTxIdSaplingHash", &[]));
    final_data.extend_from_slice(&blake2b256(b"ZTxIdOrchardHash", &[]));
    let mut personalization = [0u8; 16];
    personalization[..12].copy_from_slice(b"ZcashTxHash_");
    personalization[12..].copy_from_slice(&request.consensus_branch_id.to_le_bytes());
    Ok(blake2b256(&personalization, &final_data))
}

fn serialize_transaction(
    request: &pb::ZcashSignTransactionRequest,
    script_sigs: &[Vec<u8>],
) -> Result<Vec<u8>, Error> {
    if script_sigs.len() != request.inputs.len() {
        return Err(Error::InvalidInput);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&VERSION_GROUP_ID.to_le_bytes());
    out.extend_from_slice(&request.consensus_branch_id.to_le_bytes());
    out.extend_from_slice(&request.lock_time.to_le_bytes());
    out.extend_from_slice(&request.expiry_height.to_le_bytes());
    compact_size(request.inputs.len(), &mut out);
    for (input, script_sig) in request.inputs.iter().zip(script_sigs) {
        out.extend_from_slice(&input.prev_out_hash);
        out.extend_from_slice(&input.prev_out_index.to_le_bytes());
        field_bytes(script_sig, &mut out);
        out.extend_from_slice(&input.sequence.to_le_bytes());
    }
    compact_size(request.outputs.len(), &mut out);
    for output in &request.outputs {
        serialize_output(output, &mut out);
    }
    out.extend_from_slice(&[0, 0, 0]); // Sapling spends, Sapling outputs, Orchard actions.
    Ok(out)
}

async fn process_sign(
    hal: &mut impl crate::hal::Hal,
    request: &pb::ZcashSignTransactionRequest,
) -> Result<Response, Error> {
    let network = pb::ZcashNetwork::try_from(request.network)?;
    if request.inputs.is_empty()
        || request.inputs.len() > MAX_INPUTS
        || request.outputs.is_empty()
        || request.outputs.len() > MAX_OUTPUTS
        || request.expiry_height > 499_999_999
        || !KNOWN_BRANCH_IDS.contains(&request.consensus_branch_id)
    {
        return Err(Error::InvalidInput);
    }

    let mut total_in = 0u64;
    let mut input_public_keys = Vec::with_capacity(request.inputs.len());
    let mut outpoints = Vec::with_capacity(request.inputs.len());
    for input in &request.inputs {
        validate_keypath(&input.keypath)?;
        if input.prev_out_hash.len() != 32 || input.value > MAX_MONEY {
            return Err(Error::InvalidInput);
        }
        let outpoint = (&input.prev_out_hash, input.prev_out_index);
        if outpoints.contains(&outpoint) {
            return Err(Error::InvalidInput);
        }
        outpoints.push(outpoint);
        let (_, public_key_hash, public_key) = derive(hal, network, &input.keypath).await?;
        if input.script_pubkey != p2pkh_script(&public_key_hash) {
            return Err(Error::InvalidInput);
        }
        input_public_keys.push(public_key);
        total_in = total_in
            .checked_add(input.value)
            .filter(|value| *value <= MAX_MONEY)
            .ok_or(Error::InvalidInput)?;
    }

    let mut total_out = 0u64;
    let mut external_total = 0u64;
    let mut external_outputs = Vec::new();
    for output in &request.outputs {
        if output.value > MAX_MONEY {
            return Err(Error::InvalidInput);
        }
        let output_address = parse_output_address(network, &output.script_pubkey)?;
        total_out = total_out
            .checked_add(output.value)
            .filter(|value| *value <= MAX_MONEY)
            .ok_or(Error::InvalidInput)?;
        if output.keypath.is_empty() {
            external_total = external_total
                .checked_add(output.value)
                .ok_or(Error::InvalidInput)?;
            external_outputs.push((output_address, output.value));
        } else {
            validate_keypath(&output.keypath)?;
            let (change_address, public_key_hash, _) =
                derive(hal, network, &output.keypath).await?;
            if output.script_pubkey != p2pkh_script(&public_key_hash)
                || output_address != change_address
            {
                return Err(Error::InvalidInput);
            }
        }
    }
    let fee = total_in.checked_sub(total_out).ok_or(Error::InvalidInput)?;
    if network == pb::ZcashNetwork::ZcashTestnet {
        hal.ui()
            .confirm(&ConfirmParams {
                title: "Warning",
                body: "Zcash testnet",
                accept_is_nextarrow: true,
                ..Default::default()
            })
            .await?;
    }
    for (address, value) in external_outputs {
        hal.ui()
            .verify_recipient(&address, &super::altcoin::format_amount(value, 8, "ZEC"))
            .await?;
    }
    let total = external_total.checked_add(fee).ok_or(Error::InvalidInput)?;
    crate::workflow::transaction::verify_total_fee_maybe_warn(
        hal,
        &super::altcoin::format_amount(total, 8, "ZEC"),
        &super::altcoin::format_amount(fee, 8, "ZEC"),
        if external_total == 0 {
            None
        } else {
            Some(100.0 * fee as f64 / external_total as f64)
        },
    )
    .await?;
    hal.ui().status("Transaction\nconfirmed", true).await;

    let mut signatures = Vec::with_capacity(request.inputs.len());
    let mut script_sigs = Vec::with_capacity(request.inputs.len());
    for (index, input) in request.inputs.iter().enumerate() {
        let digest = signature_digest(request, index)?;
        let private_key = crate::keystore::secp256k1_get_private_key(hal, &input.keypath).await?;
        let result = crate::secp256k1::secp256k1_sign(
            private_key
                .as_slice()
                .try_into()
                .map_err(|_| Error::Generic)?,
            &digest,
            None,
        )?;
        let mut signature = bitcoin::secp256k1::ecdsa::Signature::from_compact(&result.signature)
            .map_err(|_| Error::Generic)?
            .serialize_der()
            .to_vec();
        signature.push(SIGHASH_ALL);
        let public_key = input_public_keys[index];
        let mut script_sig = Vec::with_capacity(signature.len() + public_key.len() + 2);
        script_sig.push(signature.len() as u8);
        script_sig.extend_from_slice(&signature);
        script_sig.push(public_key.len() as u8);
        script_sig.extend_from_slice(&public_key);
        signatures.push(signature);
        script_sigs.push(script_sig);
    }
    let serialized_transaction = serialize_transaction(request, &script_sigs)?;
    Ok(Response::SignTransaction(
        pb::ZcashSignTransactionResponse {
            signatures,
            serialized_transaction,
        },
    ))
}

pub async fn process_api(
    hal: &mut impl crate::hal::Hal,
    request: &Request,
) -> Result<Response, Error> {
    match request {
        Request::Pub(request) => process_pub(hal, request).await,
        Request::SignTransaction(request) => process_sign(hal, request).await,
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
        let (address, _, _) = derive(
            &mut crate::hal::testing::TestingHal::new(),
            pb::ZcashNetwork::ZcashMainnet,
            &[
                44 + util::bip32::HARDENED,
                133 + util::bip32::HARDENED,
                util::bip32::HARDENED,
                0,
                0,
            ],
        )
        .await
        .unwrap();
        assert_eq!(address, "t1XVXWCvpMgBvUaed4XDqWtgQgJSu1Ghz7F");
    }

    #[test]
    fn test_addresses() {
        let hash = hex!("8286bf790866805397e3a947640b77a43f0b43a5");
        assert_eq!(
            encode_address(pb::ZcashNetwork::ZcashMainnet, &hash, false),
            "t1VmmGiyjVNeCjxDZzg7vZmd99WyzVby9yC"
        );
        assert!(encode_address(pb::ZcashNetwork::ZcashTestnet, &hash, false).starts_with("tm"));
    }

    #[test]
    fn test_blake2b_personalization() {
        assert_eq!(
            blake2b256(b"ZTxIdSaplingHash", &[]),
            hex!("6f2fc8f98feafd94e74a0df4bed74391ee0b5a69945e4ced8ca8a095206f00ae")
        );
        let data: Vec<u8> = (0..129).map(|value| (value % 251) as u8).collect();
        assert_eq!(
            blake2b256(b"1234567890abcdef", &data),
            hex!("ff2650c7414e6945480538ca0737ca269b4c23a4c6ca82b06afe6e4d89fd47bc")
        );
    }

    #[test]
    fn test_signature_digest() {
        // Independently generated from ZIP-244 using Python's hashlib.blake2b.
        let input_script = hex!("76a914111111111111111111111111111111111111111188ac").to_vec();
        let output_script = hex!("76a914222222222222222222222222222222222222222288ac").to_vec();
        let request = pb::ZcashSignTransactionRequest {
            network: pb::ZcashNetwork::ZcashMainnet as i32,
            inputs: vec![pb::zcash_sign_transaction_request::Input {
                keypath: vec![],
                prev_out_hash: (0..32).collect(),
                prev_out_index: 3,
                value: 100_000,
                script_pubkey: input_script,
                sequence: 0xffff_fffe,
            }],
            outputs: vec![pb::zcash_sign_transaction_request::Output {
                value: 99_000,
                script_pubkey: output_script,
                keypath: vec![],
            }],
            lock_time: 0x1122_3344,
            expiry_height: 123_456,
            consensus_branch_id: 0xc2d6_d0b4,
        };
        assert_eq!(
            signature_digest(&request, 0).unwrap(),
            hex!("f3de7e728969127ab5eb26ab82818a76ba4d9cdde41cdc36e831d00341c63178")
        );
    }
}
