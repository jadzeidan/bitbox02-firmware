// SPDX-License-Identifier: Apache-2.0

#[cfg(not(feature = "app-tron"))]
compile_error!("Tron code is being compiled even though the app-tron feature is not enabled");

use alloc::string::String;
use sha2::Digest as _;
use sha3::Keccak256;

use crate::hal::Ui;
use crate::hal::ui::ConfirmParams;
use crate::pb;

use super::Error;
use pb::tron_request::Request;
use pb::tron_response::Response;

const MAX_RAW_DATA_SIZE: usize = 4096;
const TRC20_TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

fn validate_keypath(keypath: &[u32]) -> Result<(), Error> {
    super::altcoin::validate_bip44_keypath(keypath, 195)
}

fn encode_address(address: &[u8; 21]) -> String {
    bitcoin::base58::encode_check(address)
}

fn validate_address(address: &[u8]) -> Result<[u8; 21], Error> {
    let address: [u8; 21] = address.try_into().map_err(|_| Error::InvalidInput)?;
    if address[0] != 0x41 {
        return Err(Error::InvalidInput);
    }
    Ok(address)
}

async fn derive_address(
    hal: &mut impl crate::hal::Hal,
    keypath: &[u32],
) -> Result<([u8; 21], String), Error> {
    let xpub = super::altcoin::secp256k1_pubkey(hal, keypath).await?;
    let public_key = xpub.pubkey_uncompressed()?;
    let hash = Keccak256::digest(&public_key[1..]);
    let mut address = [0u8; 21];
    address[0] = 0x41;
    address[1..].copy_from_slice(&hash[12..]);
    Ok((address, encode_address(&address)))
}

async fn process_pub(
    hal: &mut impl crate::hal::Hal,
    request: &pb::TronPubRequest,
) -> Result<Response, Error> {
    validate_keypath(&request.keypath)?;
    let (_, address) = derive_address(hal, &request.keypath).await?;
    if request.display {
        hal.ui()
            .confirm(&ConfirmParams {
                title: "Tron",
                body: &address,
                scrollable: true,
                ..Default::default()
            })
            .await?;
    }
    Ok(Response::Pub(pb::PubResponse { r#pub: address }))
}

struct ProtoReader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> ProtoReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn varint(&mut self) -> Result<u64, Error> {
        let start = self.offset;
        let mut result = 0u64;
        for shift in (0..=63).step_by(7) {
            let byte = *self.data.get(self.offset).ok_or(Error::InvalidInput)?;
            self.offset += 1;
            if shift == 63 && byte > 1 {
                return Err(Error::InvalidInput);
            }
            result |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                if self.offset - start > 1 && byte == 0 {
                    return Err(Error::InvalidInput);
                }
                return Ok(result);
            }
        }
        Err(Error::InvalidInput)
    }

    fn bytes(&mut self) -> Result<&'a [u8], Error> {
        let len: usize = self.varint()?.try_into().map_err(|_| Error::InvalidInput)?;
        let end = self.offset.checked_add(len).ok_or(Error::InvalidInput)?;
        let result = self.data.get(self.offset..end).ok_or(Error::InvalidInput)?;
        self.offset = end;
        Ok(result)
    }

    fn field(&mut self) -> Result<(u32, u8), Error> {
        let key = self.varint()?;
        let field: u32 = (key >> 3).try_into().map_err(|_| Error::InvalidInput)?;
        let wire = (key & 7) as u8;
        if field == 0 || !matches!(wire, 0 | 2) {
            return Err(Error::InvalidInput);
        }
        Ok((field, wire))
    }

    fn finished(&self) -> bool {
        self.offset == self.data.len()
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T) -> Result<(), Error> {
    if slot.replace(value).is_some() {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}

enum ParsedTransaction {
    Trx {
        owner: [u8; 21],
        destination: [u8; 21],
        amount: u64,
    },
    Trc20 {
        owner: [u8; 21],
        contract: [u8; 21],
        destination: [u8; 21],
        amount: u64,
        fee_limit: u64,
    },
}

fn parse_transfer_contract(data: &[u8]) -> Result<ParsedTransaction, Error> {
    let mut reader = ProtoReader::new(data);
    let (mut owner, mut destination, mut amount) = (None, None, None);
    while !reader.finished() {
        match reader.field()? {
            (1, 2) => set_once(&mut owner, validate_address(reader.bytes()?)?)?,
            (2, 2) => set_once(&mut destination, validate_address(reader.bytes()?)?)?,
            (3, 0) => set_once(&mut amount, reader.varint()?)?,
            _ => return Err(Error::InvalidInput),
        }
    }
    let amount = amount.ok_or(Error::InvalidInput)?;
    if amount == 0 || amount > i64::MAX as u64 {
        return Err(Error::InvalidInput);
    }
    Ok(ParsedTransaction::Trx {
        owner: owner.ok_or(Error::InvalidInput)?,
        destination: destination.ok_or(Error::InvalidInput)?,
        amount,
    })
}

fn parse_trigger_contract(data: &[u8], fee_limit: u64) -> Result<ParsedTransaction, Error> {
    let mut reader = ProtoReader::new(data);
    let (mut owner, mut contract, mut call_data) = (None, None, None);
    while !reader.finished() {
        match reader.field()? {
            (1, 2) => set_once(&mut owner, validate_address(reader.bytes()?)?)?,
            (2, 2) => set_once(&mut contract, validate_address(reader.bytes()?)?)?,
            (3 | 5 | 6, 0) if reader.varint()? == 0 => {}
            (4, 2) => set_once(&mut call_data, reader.bytes()?)?,
            _ => return Err(Error::InvalidInput),
        }
    }
    let call_data = call_data.ok_or(Error::InvalidInput)?;
    if call_data.len() != 68
        || call_data[..4] != TRC20_TRANSFER_SELECTOR
        || call_data[4..16] != [0; 12]
        || call_data[36..60] != [0; 24]
        || fee_limit == 0
        || fee_limit > i64::MAX as u64
    {
        return Err(Error::InvalidInput);
    }
    let mut destination = [0u8; 21];
    destination[0] = 0x41;
    destination[1..].copy_from_slice(&call_data[16..36]);
    let amount = u64::from_be_bytes(call_data[60..68].try_into().unwrap());
    if amount == 0 {
        return Err(Error::InvalidInput);
    }
    Ok(ParsedTransaction::Trc20 {
        owner: owner.ok_or(Error::InvalidInput)?,
        contract: contract.ok_or(Error::InvalidInput)?,
        destination,
        amount,
        fee_limit,
    })
}

fn parse_any(data: &[u8]) -> Result<(&str, &[u8]), Error> {
    let mut reader = ProtoReader::new(data);
    let (mut type_url, mut value) = (None, None);
    while !reader.finished() {
        match reader.field()? {
            (1, 2) => set_once(
                &mut type_url,
                core::str::from_utf8(reader.bytes()?).map_err(|_| Error::InvalidInput)?,
            )?,
            (2, 2) => set_once(&mut value, reader.bytes()?)?,
            _ => return Err(Error::InvalidInput),
        }
    }
    Ok((
        type_url.ok_or(Error::InvalidInput)?,
        value.ok_or(Error::InvalidInput)?,
    ))
}

fn parse_contract(data: &[u8], fee_limit: u64) -> Result<ParsedTransaction, Error> {
    let mut reader = ProtoReader::new(data);
    let (mut contract_type, mut parameter, mut permission_id) = (None, None, None);
    while !reader.finished() {
        match reader.field()? {
            (1, 0) => set_once(&mut contract_type, reader.varint()?)?,
            (2, 2) => set_once(&mut parameter, reader.bytes()?)?,
            (5, 0) => set_once(&mut permission_id, reader.varint()?)?,
            _ => return Err(Error::InvalidInput),
        }
    }
    if permission_id.unwrap_or(0) != 0 {
        return Err(Error::InvalidInput);
    }
    let (type_url, value) = parse_any(parameter.ok_or(Error::InvalidInput)?)?;
    match contract_type.ok_or(Error::InvalidInput)? {
        1 if type_url == "type.googleapis.com/protocol.TransferContract" && fee_limit == 0 => {
            parse_transfer_contract(value)
        }
        31 if type_url == "type.googleapis.com/protocol.TriggerSmartContract" => {
            parse_trigger_contract(value, fee_limit)
        }
        _ => Err(Error::InvalidInput),
    }
}

fn parse_raw_data(raw_data: &[u8]) -> Result<ParsedTransaction, Error> {
    if raw_data.is_empty() || raw_data.len() > MAX_RAW_DATA_SIZE {
        return Err(Error::InvalidInput);
    }
    let mut reader = ProtoReader::new(raw_data);
    let (mut contract, mut expiration, mut timestamp, mut fee_limit) = (None, None, None, None);
    let mut seen = [false; 19];
    while !reader.finished() {
        let (field, wire) = reader.field()?;
        if field != 11 && (field as usize >= seen.len() || seen[field as usize]) {
            return Err(Error::InvalidInput);
        }
        if field != 11 {
            seen[field as usize] = true;
        }
        match (field, wire) {
            (1, 2) if reader.bytes()?.len() == 2 => {}
            (4, 2) if reader.bytes()?.len() == 8 => {}
            (8, 0) => expiration = Some(reader.varint()?),
            (11, 2) => set_once(&mut contract, reader.bytes()?)?,
            (14, 0) => timestamp = Some(reader.varint()?),
            (18, 0) => fee_limit = Some(reader.varint()?),
            (3, 0) if reader.varint()? <= i64::MAX as u64 => {}
            _ => return Err(Error::InvalidInput),
        }
    }
    let expiration = expiration.ok_or(Error::InvalidInput)?;
    let timestamp = timestamp.ok_or(Error::InvalidInput)?;
    if timestamp == 0
        || timestamp > i64::MAX as u64
        || expiration > i64::MAX as u64
        || expiration <= timestamp
    {
        return Err(Error::InvalidInput);
    }
    parse_contract(contract.ok_or(Error::InvalidInput)?, fee_limit.unwrap_or(0))
}

async fn process_sign(
    hal: &mut impl crate::hal::Hal,
    request: &pb::TronSignTransactionRequest,
) -> Result<Response, Error> {
    validate_keypath(&request.keypath)?;
    let network = pb::TronNetwork::try_from(request.network)?;
    let transaction = parse_raw_data(&request.raw_data)?;
    let (derived_address, _) = derive_address(hal, &request.keypath).await?;
    let owner = match &transaction {
        ParsedTransaction::Trx { owner, .. } | ParsedTransaction::Trc20 { owner, .. } => owner,
    };
    if owner != &derived_address {
        return Err(Error::InvalidInput);
    }
    if network == pb::TronNetwork::TronTestnet {
        hal.ui()
            .confirm(&ConfirmParams {
                title: "Warning",
                body: "Tron testnet",
                accept_is_nextarrow: true,
                ..Default::default()
            })
            .await?;
    }
    match transaction {
        ParsedTransaction::Trx {
            destination,
            amount,
            ..
        } => {
            hal.ui()
                .verify_recipient(
                    &encode_address(&destination),
                    &super::altcoin::format_amount(amount, 6, "TRX"),
                )
                .await?;
        }
        ParsedTransaction::Trc20 {
            contract,
            destination,
            amount,
            fee_limit,
            ..
        } => {
            hal.ui()
                .verify_recipient(
                    &encode_address(&destination),
                    &format!("{} TRC-20 units", amount),
                )
                .await?;
            hal.ui()
                .confirm(&ConfirmParams {
                    title: "Token contract",
                    body: &encode_address(&contract),
                    scrollable: true,
                    accept_is_nextarrow: true,
                    ..Default::default()
                })
                .await?;
            hal.ui()
                .confirm(&ConfirmParams {
                    title: "Maximum fee",
                    body: &super::altcoin::format_amount(fee_limit, 6, "TRX"),
                    longtouch: true,
                    ..Default::default()
                })
                .await?;
        }
    }
    hal.ui().status("Transaction\nconfirmed", true).await;

    let digest: [u8; 32] = sha2::Sha256::digest(&request.raw_data).into();
    let private_key = crate::keystore::secp256k1_get_private_key(hal, &request.keypath).await?;
    let result = crate::secp256k1::secp256k1_sign(
        private_key
            .as_slice()
            .try_into()
            .map_err(|_| Error::Generic)?,
        &digest,
        None,
    )?;
    let mut signature = result.signature.to_vec();
    signature.push(result.recid);
    Ok(Response::SignTransaction(pb::TronSignTransactionResponse {
        signature,
    }))
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
    use alloc::vec::Vec;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[async_test::test]
    async fn test_address() {
        crate::keystore::testing::mock_unlocked_using_mnemonic(MNEMONIC, "");
        let (_, address) = derive_address(
            &mut crate::hal::testing::TestingHal::new(),
            &[
                44 + util::bip32::HARDENED,
                195 + util::bip32::HARDENED,
                util::bip32::HARDENED,
                0,
                0,
            ],
        )
        .await
        .unwrap();
        assert_eq!(address, "TUEZSdKsoDHQMeZwihtdoBiN46zxhGWYdH");
    }

    fn varint(mut value: u64) -> Vec<u8> {
        let mut result = Vec::new();
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            result.push(byte);
            if value == 0 {
                return result;
            }
        }
    }

    fn field_varint(field: u32, value: u64, out: &mut Vec<u8>) {
        out.extend_from_slice(&varint((field as u64) << 3));
        out.extend_from_slice(&varint(value));
    }

    fn field_bytes(field: u32, value: &[u8], out: &mut Vec<u8>) {
        out.extend_from_slice(&varint(((field as u64) << 3) | 2));
        out.extend_from_slice(&varint(value.len() as u64));
        out.extend_from_slice(value);
    }

    fn raw_data(contract_type: u64, type_url: &str, value: &[u8], fee_limit: u64) -> Vec<u8> {
        let mut any = Vec::new();
        field_bytes(1, type_url.as_bytes(), &mut any);
        field_bytes(2, value, &mut any);
        let mut contract = Vec::new();
        field_varint(1, contract_type, &mut contract);
        field_bytes(2, &any, &mut contract);
        let mut raw_data = Vec::new();
        field_bytes(1, &[0, 1], &mut raw_data);
        field_bytes(4, &[0; 8], &mut raw_data);
        field_varint(8, 2_000, &mut raw_data);
        field_bytes(11, &contract, &mut raw_data);
        field_varint(14, 1_000, &mut raw_data);
        if fee_limit != 0 {
            field_varint(18, fee_limit, &mut raw_data);
        }
        raw_data
    }

    #[test]
    fn test_address_encoding() {
        let address = [0x41; 21];
        let encoded = encode_address(&address);
        assert_eq!(bitcoin::base58::decode_check(&encoded).unwrap(), address);
    }

    #[test]
    fn test_reject_noncanonical_varint() {
        let mut reader = ProtoReader::new(&[0x80, 0]);
        assert_eq!(reader.varint(), Err(Error::InvalidInput));
    }

    #[test]
    fn test_parse_trx_transfer() {
        let owner = [0x41; 21];
        let mut destination = [0x22; 21];
        destination[0] = 0x41;
        let mut transfer = Vec::new();
        field_bytes(1, &owner, &mut transfer);
        field_bytes(2, &destination, &mut transfer);
        field_varint(3, 123, &mut transfer);
        let raw_data = raw_data(
            1,
            "type.googleapis.com/protocol.TransferContract",
            &transfer,
            0,
        );
        match parse_raw_data(&raw_data).unwrap() {
            ParsedTransaction::Trx {
                owner: parsed_owner,
                destination: parsed_destination,
                amount,
            } => {
                assert_eq!(parsed_owner, owner);
                assert_eq!(parsed_destination, destination);
                assert_eq!(amount, 123);
            }
            _ => panic!("unexpected transaction"),
        }
    }

    #[test]
    fn test_parse_trc20_transfer() {
        let owner = [0x41; 21];
        let contract_address = [0x41; 21];
        let destination = [0x22; 20];
        let mut call_data = [0u8; 68];
        call_data[..4].copy_from_slice(&TRC20_TRANSFER_SELECTOR);
        call_data[16..36].copy_from_slice(&destination);
        call_data[60..].copy_from_slice(&123u64.to_be_bytes());
        let mut trigger = Vec::new();
        field_bytes(1, &owner, &mut trigger);
        field_bytes(2, &contract_address, &mut trigger);
        field_bytes(4, &call_data, &mut trigger);
        let raw_data = raw_data(
            31,
            "type.googleapis.com/protocol.TriggerSmartContract",
            &trigger,
            10_000_000,
        );
        match parse_raw_data(&raw_data).unwrap() {
            ParsedTransaction::Trc20 {
                owner: parsed_owner,
                contract,
                destination: parsed_destination,
                amount,
                fee_limit,
            } => {
                assert_eq!(parsed_owner, owner);
                assert_eq!(contract, contract_address);
                assert_eq!(parsed_destination[1..], destination);
                assert_eq!(amount, 123);
                assert_eq!(fee_limit, 10_000_000);
            }
            _ => panic!("unexpected transaction"),
        }
    }
}
