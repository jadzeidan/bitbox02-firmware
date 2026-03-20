// SPDX-License-Identifier: Apache-2.0

use super::Error;
use super::pb;

use alloc::string::String;
use alloc::vec::Vec;
use pb::solana_response::Response;

use crate::hal::Ui;
use crate::workflow::transaction;

const LAMPORT_DECIMALS: usize = 9;
const LAMPORTS_PER_SIGNATURE: u64 = 5000;

struct MessageHeader {
    num_required_signatures: u8,
    _num_readonly_signed_accounts: u8,
    _num_readonly_unsigned_accounts: u8,
}

struct CompiledInstruction {
    program_id_index: u8,
    account_indices: Vec<u8>,
    data: Vec<u8>,
}

struct ParsedMessage {
    header: MessageHeader,
    account_keys: Vec<[u8; 32]>,
    instructions: Vec<CompiledInstruction>,
}

struct ParsedTransfer {
    recipient: [u8; 32],
    amount: ParsedTransferAmount,
}

enum ParsedTransferAmount {
    SolLamports(u64),
    SplToken { amount: u64, decimals: Option<u8> },
}

fn format_lamports(lamports: u64) -> alloc::string::String {
    format!("{} SOL", util::decimal::format(lamports, LAMPORT_DECIMALS))
}

fn format_token_amount(amount: u64, decimals: Option<u8>) -> String {
    match decimals {
        Some(decimals) => format!("{} SPL", util::decimal::format(amount, decimals as usize)),
        None => format!("{} SPL units", amount),
    }
}

impl ParsedTransfer {
    fn amount_display(&self) -> String {
        match self.amount {
            ParsedTransferAmount::SolLamports(lamports) => format_lamports(lamports),
            ParsedTransferAmount::SplToken { amount, decimals } => {
                format_token_amount(amount, decimals)
            }
        }
    }

    fn sol_lamports(&self) -> Option<u64> {
        match self.amount {
            ParsedTransferAmount::SolLamports(lamports) => Some(lamports),
            ParsedTransferAmount::SplToken { .. } => None,
        }
    }
}

fn read_u8(input: &[u8], cursor: &mut usize) -> Result<u8, Error> {
    let result = *input.get(*cursor).ok_or(Error::InvalidInput)?;
    *cursor += 1;
    Ok(result)
}

fn read_bytes<'a>(input: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], Error> {
    let bytes = input
        .get(*cursor..(*cursor).checked_add(len).ok_or(Error::InvalidInput)?)
        .ok_or(Error::InvalidInput)?;
    *cursor += len;
    Ok(bytes)
}

fn read_shortvec_len(input: &[u8], cursor: &mut usize) -> Result<usize, Error> {
    let mut len = 0usize;
    let mut shift = 0usize;
    loop {
        let byte = read_u8(input, cursor)?;
        len |= ((byte & 0x7f) as usize)
            .checked_shl(shift as _)
            .ok_or(Error::InvalidInput)?;
        if byte & 0x80 == 0 {
            return Ok(len);
        }
        shift += 7;
        if shift > 28 {
            return Err(Error::InvalidInput);
        }
    }
}

fn parse_message(input: &[u8]) -> Result<ParsedMessage, Error> {
    let mut cursor = 0usize;

    let first = read_u8(input, &mut cursor)?;
    let (header, is_v0): (MessageHeader, bool) = if first & 0x80 == 0 {
        (
            MessageHeader {
                num_required_signatures: first,
                _num_readonly_signed_accounts: read_u8(input, &mut cursor)?,
                _num_readonly_unsigned_accounts: read_u8(input, &mut cursor)?,
            },
            false,
        )
    } else {
        let version = first & 0x7f;
        if version != 0 {
            return Err(Error::InvalidInput);
        }
        (
            MessageHeader {
                num_required_signatures: read_u8(input, &mut cursor)?,
                _num_readonly_signed_accounts: read_u8(input, &mut cursor)?,
                _num_readonly_unsigned_accounts: read_u8(input, &mut cursor)?,
            },
            true,
        )
    };

    let account_keys_len = read_shortvec_len(input, &mut cursor)?;
    if account_keys_len == 0 {
        return Err(Error::InvalidInput);
    }
    let mut account_keys = Vec::with_capacity(account_keys_len);
    for _ in 0..account_keys_len {
        let key_bytes = read_bytes(input, &mut cursor, 32)?;
        let mut key = [0u8; 32];
        key.copy_from_slice(key_bytes);
        account_keys.push(key);
    }
    if header.num_required_signatures == 0
        || header.num_required_signatures as usize > account_keys.len()
    {
        return Err(Error::InvalidInput);
    }

    let _recent_blockhash = read_bytes(input, &mut cursor, 32)?;

    let instruction_len = read_shortvec_len(input, &mut cursor)?;
    let mut instructions = Vec::with_capacity(instruction_len);
    for _ in 0..instruction_len {
        let program_id_index = read_u8(input, &mut cursor)?;
        let account_indices_len = read_shortvec_len(input, &mut cursor)?;
        let mut account_indices = Vec::with_capacity(account_indices_len);
        for _ in 0..account_indices_len {
            account_indices.push(read_u8(input, &mut cursor)?);
        }
        let data_len = read_shortvec_len(input, &mut cursor)?;
        let data = read_bytes(input, &mut cursor, data_len)?.to_vec();
        instructions.push(CompiledInstruction {
            program_id_index,
            account_indices,
            data,
        });
    }

    if is_v0 {
        let address_table_lookups_len = read_shortvec_len(input, &mut cursor)?;
        if address_table_lookups_len != 0 {
            return Err(Error::InvalidInput);
        }
    }

    if cursor != input.len() {
        return Err(Error::InvalidInput);
    }

    Ok(ParsedMessage {
        header,
        account_keys,
        instructions,
    })
}

fn u32_from_le(data: &[u8]) -> Result<u32, Error> {
    let bytes: [u8; 4] = data.try_into().map_err(|_| Error::InvalidInput)?;
    Ok(u32::from_le_bytes(bytes))
}

fn u64_from_le(data: &[u8]) -> Result<u64, Error> {
    let bytes: [u8; 8] = data.try_into().map_err(|_| Error::InvalidInput)?;
    Ok(u64::from_le_bytes(bytes))
}

fn parse_transfers_and_fee(
    parsed: &ParsedMessage,
    signer_pubkey: &[u8; 32],
) -> Result<(Vec<ParsedTransfer>, u64), Error> {
    // 11111111111111111111111111111111
    const SYSTEM_PROGRAM_ID: [u8; 32] = [0u8; 32];
    // ComputeBudget111111111111111111111111111111
    const COMPUTE_BUDGET_PROGRAM_ID_B58: &str = "ComputeBudget111111111111111111111111111111";
    // TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA
    const TOKEN_PROGRAM_ID_B58: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
    // TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb
    const TOKEN_2022_PROGRAM_ID_B58: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

    let signer_index = parsed.account_keys[..parsed.header.num_required_signatures as usize]
        .iter()
        .position(|k| k == signer_pubkey)
        .ok_or(Error::InvalidInput)?;
    // To keep user verification simple and correct, we support only txs where the signing account
    // is the fee payer (first account key).
    if signer_index != 0 {
        return Err(Error::InvalidInput);
    }

    let mut transfers = Vec::new();
    let mut compute_unit_limit: Option<u32> = None;
    let mut compute_unit_price_micro_lamports: Option<u64> = None;

    for instruction in parsed.instructions.iter() {
        let program_key = parsed
            .account_keys
            .get(instruction.program_id_index as usize)
            .ok_or(Error::InvalidInput)?;
        let program_id_b58 = bitcoin::base58::encode(program_key);
        if *program_key == SYSTEM_PROGRAM_ID {
            // SystemProgram::Transfer
            if instruction.data.len() != 12 {
                return Err(Error::InvalidInput);
            }
            let ix_type = u32_from_le(&instruction.data[..4])?;
            if ix_type != 2 {
                return Err(Error::InvalidInput);
            }
            if instruction.account_indices.len() < 2 {
                return Err(Error::InvalidInput);
            }
            let sender_index = *instruction
                .account_indices
                .first()
                .ok_or(Error::InvalidInput)? as usize;
            if sender_index != signer_index {
                return Err(Error::InvalidInput);
            }
            let recipient_index = instruction.account_indices[1] as usize;
            let recipient = *parsed
                .account_keys
                .get(recipient_index)
                .ok_or(Error::InvalidInput)?;
            let lamports = u64_from_le(&instruction.data[4..12])?;
            transfers.push(ParsedTransfer {
                recipient,
                amount: ParsedTransferAmount::SolLamports(lamports),
            });
        } else if program_id_b58 == COMPUTE_BUDGET_PROGRAM_ID_B58 {
            let discriminator = *instruction.data.first().ok_or(Error::InvalidInput)?;
            match discriminator {
                // SetComputeUnitLimit
                2 => {
                    if instruction.data.len() != 5 {
                        return Err(Error::InvalidInput);
                    }
                    compute_unit_limit = Some(u32_from_le(&instruction.data[1..5])?);
                }
                // SetComputeUnitPrice
                3 => {
                    if instruction.data.len() != 9 {
                        return Err(Error::InvalidInput);
                    }
                    compute_unit_price_micro_lamports = Some(u64_from_le(&instruction.data[1..9])?);
                }
                _ => return Err(Error::InvalidInput),
            }
        } else if program_id_b58 == TOKEN_PROGRAM_ID_B58
            || program_id_b58 == TOKEN_2022_PROGRAM_ID_B58
        {
            let discriminator = *instruction.data.first().ok_or(Error::InvalidInput)?;
            match discriminator {
                // TokenInstruction::Transfer { amount: u64 }
                3 => {
                    if instruction.data.len() != 9 || instruction.account_indices.len() < 3 {
                        return Err(Error::InvalidInput);
                    }
                    let authority_index = instruction.account_indices[2] as usize;
                    if authority_index != signer_index {
                        return Err(Error::InvalidInput);
                    }
                    let recipient_index = instruction.account_indices[1] as usize;
                    let recipient = *parsed
                        .account_keys
                        .get(recipient_index)
                        .ok_or(Error::InvalidInput)?;
                    let amount = u64_from_le(&instruction.data[1..9])?;
                    transfers.push(ParsedTransfer {
                        recipient,
                        amount: ParsedTransferAmount::SplToken {
                            amount,
                            decimals: None,
                        },
                    });
                }
                // TokenInstruction::TransferChecked { amount: u64, decimals: u8 }
                12 => {
                    if instruction.data.len() != 10 || instruction.account_indices.len() < 4 {
                        return Err(Error::InvalidInput);
                    }
                    let authority_index = instruction.account_indices[3] as usize;
                    if authority_index != signer_index {
                        return Err(Error::InvalidInput);
                    }
                    let recipient_index = instruction.account_indices[2] as usize;
                    let recipient = *parsed
                        .account_keys
                        .get(recipient_index)
                        .ok_or(Error::InvalidInput)?;
                    let amount = u64_from_le(&instruction.data[1..9])?;
                    let decimals = *instruction.data.get(9).ok_or(Error::InvalidInput)?;
                    transfers.push(ParsedTransfer {
                        recipient,
                        amount: ParsedTransferAmount::SplToken {
                            amount,
                            decimals: Some(decimals),
                        },
                    });
                }
                _ => return Err(Error::InvalidInput),
            }
        } else {
            return Err(Error::InvalidInput);
        }
    }

    if transfers.is_empty() {
        return Err(Error::InvalidInput);
    }

    let base_fee = (parsed.header.num_required_signatures as u64)
        .checked_mul(LAMPORTS_PER_SIGNATURE)
        .ok_or(Error::InvalidInput)?;
    let priority_fee = match (compute_unit_limit, compute_unit_price_micro_lamports) {
        (Some(limit), Some(price_micro_lamports)) => {
            (limit as u64)
                .checked_mul(price_micro_lamports)
                .ok_or(Error::InvalidInput)?
                / 1_000_000
        }
        _ => 0,
    };
    let fee = base_fee
        .checked_add(priority_fee)
        .ok_or(Error::InvalidInput)?;

    Ok((transfers, fee))
}

pub async fn process(
    hal: &mut impl crate::hal::Hal,
    request: &pb::SolanaSignTransactionRequest,
) -> Result<Response, Error> {
    if request.message.is_empty() {
        return Err(Error::InvalidInput);
    }
    super::keypath::validate_address(&request.keypath)?;
    let signer_pubkey = {
        let xpub = crate::keystore::ed25519::get_xpub(hal, &request.keypath)
            .or(Err(Error::InvalidInput))?;
        let mut pubkey = [0u8; 32];
        pubkey.copy_from_slice(xpub.pubkey_bytes());
        pubkey
    };
    let parsed = parse_message(&request.message)?;
    let (transfers, fee) = parse_transfers_and_fee(&parsed, &signer_pubkey)?;

    let mut total = 0u64;
    for transfer in transfers.iter() {
        hal.ui()
            .verify_recipient(
                &bitcoin::base58::encode(&transfer.recipient),
                &transfer.amount_display(),
            )
            .await?;
        if let Some(lamports) = transfer.sol_lamports() {
            total = total.checked_add(lamports).ok_or(Error::InvalidInput)?;
        }
    }

    let total_with_fee = total.checked_add(fee).ok_or(Error::InvalidInput)?;
    let fee_percentage = if total == 0 {
        None
    } else {
        Some(100. * fee as f64 / total as f64)
    };
    transaction::verify_total_fee_maybe_warn(
        hal,
        &format_lamports(total_with_fee),
        &format_lamports(fee),
        fee_percentage,
    )
    .await?;

    let signature_result =
        crate::keystore::ed25519::sign_message(hal, &request.keypath, &request.message)
            .or(Err(Error::InvalidInput))?;
    Ok(Response::SignTransaction(
        pb::SolanaSignTransactionResponse {
            signature: signature_result.signature.to_vec(),
            public_key: signature_result.public_key.as_ref().to_vec(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::hal::testing::TestingHal;
    use crate::hal::testing::ui::Screen;
    use crate::keystore;
    use crate::keystore::testing::mock_unlocked;
    use util::bb02_async::block_on;
    use util::bip32::HARDENED;

    fn push_shortvec(out: &mut Vec<u8>, len: usize) {
        let mut rem = len;
        loop {
            let mut elem = (rem & 0x7f) as u8;
            rem >>= 7;
            if rem != 0 {
                elem |= 0x80;
            }
            out.push(elem);
            if rem == 0 {
                break;
            }
        }
    }

    fn make_simple_transfer_message(
        signer_pubkey: &[u8; 32],
        recipient_pubkey: &[u8; 32],
        lamports: u64,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        // Legacy message header
        out.push(1); // num_required_signatures
        out.push(0); // num_readonly_signed_accounts
        out.push(1); // num_readonly_unsigned_accounts (system program)

        // account_keys: signer, recipient, system program (all-zero pubkey)
        push_shortvec(&mut out, 3);
        out.extend_from_slice(signer_pubkey);
        out.extend_from_slice(recipient_pubkey);
        out.extend_from_slice(&[0u8; 32]);

        // recent_blockhash
        out.extend_from_slice(&[0u8; 32]);

        // instructions: 1x SystemProgram::Transfer
        push_shortvec(&mut out, 1);
        out.push(2); // program_id_index (system program)
        push_shortvec(&mut out, 2); // accounts
        out.push(0); // from signer
        out.push(1); // to recipient
        push_shortvec(&mut out, 12); // data len
        out.extend_from_slice(&2u32.to_le_bytes()); // Transfer ix discriminator
        out.extend_from_slice(&lamports.to_le_bytes());

        out
    }

    fn make_spl_token_transfer_checked_message(
        signer_pubkey: &[u8; 32],
        recipient_token_account_pubkey: &[u8; 32],
        token_program_pubkey: &[u8; 32],
        amount: u64,
        decimals: u8,
    ) -> Vec<u8> {
        let source_token_account = [0x11u8; 32];
        let mint = [0x22u8; 32];
        let mut out = Vec::new();
        // Legacy message header
        out.push(1); // num_required_signatures
        out.push(0); // num_readonly_signed_accounts
        out.push(1); // num_readonly_unsigned_accounts (token program)

        // account_keys: signer, source token account, mint, destination token account, token program
        push_shortvec(&mut out, 5);
        out.extend_from_slice(signer_pubkey);
        out.extend_from_slice(&source_token_account);
        out.extend_from_slice(&mint);
        out.extend_from_slice(recipient_token_account_pubkey);
        out.extend_from_slice(token_program_pubkey);

        // recent_blockhash
        out.extend_from_slice(&[0u8; 32]);

        // instructions: 1x TokenInstruction::TransferChecked
        push_shortvec(&mut out, 1);
        out.push(4); // program_id_index (token program)
        push_shortvec(&mut out, 4); // accounts
        out.push(1); // source token account
        out.push(2); // mint
        out.push(3); // destination token account
        out.push(0); // authority (signer)
        push_shortvec(&mut out, 10); // data len
        out.push(12); // TransferChecked discriminator
        out.extend_from_slice(&amount.to_le_bytes());
        out.push(decimals);

        out
    }

    #[test]
    fn test_process() {
        mock_unlocked();
        let keypath = [44 + HARDENED, 501 + HARDENED, HARDENED, HARDENED].to_vec();
        let signer_pubkey = {
            let xpub = keystore::ed25519::get_xpub(&mut TestingHal::new(), &keypath).unwrap();
            let mut pubkey = [0u8; 32];
            pubkey.copy_from_slice(xpub.pubkey_bytes());
            pubkey
        };
        let recipient_pubkey = [0x42u8; 32];
        let message =
            make_simple_transfer_message(&signer_pubkey, &recipient_pubkey, 1_000_000_000);
        let request = pb::SolanaSignTransactionRequest {
            keypath: keypath.clone(),
            message: message.clone(),
        };

        let expected = {
            let sig = keystore::ed25519::sign_message(&mut TestingHal::new(), &keypath, &message)
                .unwrap();
            pb::SolanaSignTransactionResponse {
                signature: sig.signature.to_vec(),
                public_key: sig.public_key.as_ref().to_vec(),
            }
        };
        assert_eq!(
            block_on(process(&mut TestingHal::new(), &request)),
            Ok(Response::SignTransaction(expected)),
        );

        let mut hal = TestingHal::new();
        assert!(block_on(process(&mut hal, &request)).is_ok());
        assert_eq!(
            hal.ui.screens,
            vec![
                Screen::Recipient {
                    recipient: bitcoin::base58::encode(&recipient_pubkey),
                    amount: "1 SOL".into(),
                },
                Screen::TotalFee {
                    total: "1.000005 SOL".into(),
                    fee: "0.000005 SOL".into(),
                    longtouch: true,
                },
            ]
        );

        keystore::lock();
        assert_eq!(
            block_on(process(&mut TestingHal::new(), &request)),
            Err(Error::InvalidInput)
        );

        mock_unlocked();
        assert_eq!(
            block_on(process(
                &mut TestingHal::new(),
                &pb::SolanaSignTransactionRequest {
                    keypath: [44 + HARDENED, 60 + HARDENED, HARDENED, HARDENED].to_vec(),
                    message: message.clone(),
                }
            )),
            Err(Error::InvalidInput)
        );

        mock_unlocked();
        assert_eq!(
            block_on(process(
                &mut TestingHal::new(),
                &pb::SolanaSignTransactionRequest {
                    keypath: [44 + HARDENED, 501 + HARDENED, HARDENED, HARDENED].to_vec(),
                    message: vec![],
                }
            )),
            Err(Error::InvalidInput)
        );

        mock_unlocked();
        assert_eq!(
            block_on(process(
                &mut TestingHal::new(),
                &pb::SolanaSignTransactionRequest {
                    keypath,
                    message: b"invalid message".to_vec(),
                }
            )),
            Err(Error::InvalidInput)
        );
    }

    #[test]
    fn test_process_spl_token_transfer_checked() {
        mock_unlocked();
        let keypath = [44 + HARDENED, 501 + HARDENED, HARDENED, HARDENED].to_vec();
        let signer_pubkey = {
            let xpub = keystore::ed25519::get_xpub(&mut TestingHal::new(), &keypath).unwrap();
            let mut pubkey = [0u8; 32];
            pubkey.copy_from_slice(xpub.pubkey_bytes());
            pubkey
        };
        let recipient_token_account_pubkey = [0x43u8; 32];
        let token_program_pubkey = {
            let decoded =
                bitcoin::base58::decode("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
            let mut pubkey = [0u8; 32];
            pubkey.copy_from_slice(&decoded);
            pubkey
        };
        let message = make_spl_token_transfer_checked_message(
            &signer_pubkey,
            &recipient_token_account_pubkey,
            &token_program_pubkey,
            1_234_500,
            4,
        );
        let request = pb::SolanaSignTransactionRequest {
            keypath: keypath.clone(),
            message: message.clone(),
        };

        let expected = {
            let sig = keystore::ed25519::sign_message(&mut TestingHal::new(), &keypath, &message)
                .unwrap();
            pb::SolanaSignTransactionResponse {
                signature: sig.signature.to_vec(),
                public_key: sig.public_key.as_ref().to_vec(),
            }
        };
        assert_eq!(
            block_on(process(&mut TestingHal::new(), &request)),
            Ok(Response::SignTransaction(expected)),
        );

        let mut hal = TestingHal::new();
        assert!(block_on(process(&mut hal, &request)).is_ok());
        assert_eq!(
            hal.ui.screens,
            vec![
                Screen::Recipient {
                    recipient: bitcoin::base58::encode(&recipient_token_account_pubkey),
                    amount: "123.45 SPL".into(),
                },
                Screen::TotalFee {
                    total: "0.000005 SOL".into(),
                    fee: "0.000005 SOL".into(),
                    longtouch: true,
                },
            ]
        );
    }
}
