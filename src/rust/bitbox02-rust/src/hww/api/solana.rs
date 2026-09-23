// SPDX-License-Identifier: Apache-2.0

#[cfg(not(feature = "app-solana"))]
compile_error!("Solana code is being compiled even though the app-solana feature is not enabled");

use alloc::string::String;
use alloc::vec::Vec;

use crate::hal::Ui;
use crate::hal::ui::ConfirmParams;
use crate::pb;
use util::bip32::HARDENED;

use super::Error;
use pb::solana_request::Request;
use pb::solana_response::Response;

const MAX_MESSAGE_SIZE: usize = 4096;
const SYSTEM_PROGRAM: [u8; 32] = [0; 32];
const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

fn validate_keypath(keypath: &[u32]) -> Result<(), Error> {
    match keypath {
        [purpose, coin, account]
            if *purpose == 44 + HARDENED
                && *coin == 501 + HARDENED
                && (HARDENED..=HARDENED + super::altcoin::MAX_ACCOUNT).contains(account) =>
        {
            Ok(())
        }
        [purpose, coin, account, change]
            if *purpose == 44 + HARDENED
                && *coin == 501 + HARDENED
                && (HARDENED..=HARDENED + super::altcoin::MAX_ACCOUNT).contains(account)
                && (HARDENED..=HARDENED + super::altcoin::MAX_ACCOUNT).contains(change) =>
        {
            Ok(())
        }
        _ => Err(Error::InvalidInput),
    }
}

fn address(public_key: &[u8; 32]) -> String {
    bitcoin::base58::encode(public_key)
}

async fn process_pub(
    hal: &mut impl crate::hal::Hal,
    request: &pb::SolanaPubRequest,
) -> Result<Response, Error> {
    validate_keypath(&request.keypath)?;
    let public_key = crate::keystore::ed25519::slip10_get_pubkey_twice(hal, &request.keypath)
        .await
        .map_err(|_| Error::Generic)?;
    let address = address(public_key.as_bytes());
    if request.display {
        hal.ui()
            .confirm(&ConfirmParams {
                title: "Solana",
                body: &address,
                scrollable: true,
                ..Default::default()
            })
            .await?;
    }
    Ok(Response::Pub(pb::PubResponse { r#pub: address }))
}

struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_u8(&mut self) -> Result<u8, Error> {
        let value = *self.data.get(self.offset).ok_or(Error::InvalidInput)?;
        self.offset += 1;
        Ok(value)
    }

    fn read(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let end = self.offset.checked_add(len).ok_or(Error::InvalidInput)?;
        let result = self.data.get(self.offset..end).ok_or(Error::InvalidInput)?;
        self.offset = end;
        Ok(result)
    }

    fn read_shortvec(&mut self) -> Result<usize, Error> {
        let mut value = 0usize;
        let mut shift = 0usize;
        let start = self.offset;
        loop {
            let byte = self.read_u8()?;
            if shift >= usize::BITS as usize || ((byte & 0x7f) as usize) > (usize::MAX >> shift) {
                return Err(Error::InvalidInput);
            }
            value |= ((byte & 0x7f) as usize) << shift;
            if byte & 0x80 == 0 {
                let encoded_len = self.offset - start;
                if encoded_len > 3
                    || value > u16::MAX as usize
                    || (encoded_len > 1 && value < (1 << (7 * (encoded_len - 1))))
                {
                    return Err(Error::InvalidInput);
                }
                return Ok(value);
            }
            shift += 7;
        }
    }

    fn finished(&self) -> bool {
        self.offset == self.data.len()
    }
}

enum Transfer {
    Sol {
        destination: [u8; 32],
        amount: u64,
    },
    Spl {
        destination: [u8; 32],
        mint: [u8; 32],
        amount: u64,
        decimals: u8,
    },
}

fn parse_message(message: &[u8], signer: &[u8; 32]) -> Result<Vec<Transfer>, Error> {
    if message.is_empty() || message.len() > MAX_MESSAGE_SIZE {
        return Err(Error::InvalidInput);
    }
    let mut reader = Reader::new(message);
    let first = reader.read_u8()?;
    let versioned = first & 0x80 != 0;
    let required_signatures = if versioned {
        if first != 0x80 {
            return Err(Error::InvalidInput);
        }
        reader.read_u8()? as usize
    } else {
        first as usize
    };
    let readonly_signed = reader.read_u8()? as usize;
    let readonly_unsigned = reader.read_u8()? as usize;
    if required_signatures == 0 || readonly_signed >= required_signatures {
        return Err(Error::InvalidInput);
    }

    let account_count = reader.read_shortvec()?;
    if account_count == 0
        || account_count > 256
        || required_signatures > account_count
        || readonly_unsigned > account_count - required_signatures
    {
        return Err(Error::InvalidInput);
    }
    let mut accounts = Vec::with_capacity(account_count);
    for _ in 0..account_count {
        accounts.push(
            reader
                .read(32)?
                .try_into()
                .map_err(|_| Error::InvalidInput)?,
        );
    }
    let signer_index = accounts
        .iter()
        .position(|account| account == signer)
        .ok_or(Error::InvalidInput)?;
    if signer_index >= required_signatures || signer_index >= required_signatures - readonly_signed
    {
        return Err(Error::InvalidInput);
    }

    let is_writable = |index: usize| {
        if index < required_signatures {
            index < required_signatures - readonly_signed
        } else {
            index < account_count - readonly_unsigned
        }
    };

    reader.read(32)?; // recent blockhash
    let instruction_count = reader.read_shortvec()?;
    if instruction_count == 0 || instruction_count > 16 {
        return Err(Error::InvalidInput);
    }
    let mut transfers = Vec::new();
    for _ in 0..instruction_count {
        let program_index = reader.read_u8()? as usize;
        let program = *accounts.get(program_index).ok_or(Error::InvalidInput)?;
        let instruction_account_count = reader.read_shortvec()?;
        if instruction_account_count > 16 {
            return Err(Error::InvalidInput);
        }
        let instruction_accounts = reader.read(instruction_account_count)?;
        if instruction_accounts
            .iter()
            .any(|&index| index as usize >= account_count)
        {
            return Err(Error::InvalidInput);
        }
        let data_len = reader.read_shortvec()?;
        let data = reader.read(data_len)?;

        if program == SYSTEM_PROGRAM {
            if instruction_accounts.len() != 2
                || data.len() != 12
                || u32::from_le_bytes(data[..4].try_into().unwrap()) != 2
                || instruction_accounts[0] as usize != signer_index
                || !is_writable(instruction_accounts[1] as usize)
            {
                return Err(Error::InvalidInput);
            }
            let amount = u64::from_le_bytes(data[4..].try_into().unwrap());
            if amount == 0 {
                return Err(Error::InvalidInput);
            }
            transfers.push(Transfer::Sol {
                destination: accounts[instruction_accounts[1] as usize],
                amount,
            });
        } else {
            let program_address = address(&program);
            if (program_address != TOKEN_PROGRAM && program_address != TOKEN_2022_PROGRAM)
                || instruction_accounts.len() != 4
                || data.len() != 10
                || data[0] != 12 // TransferChecked
                || instruction_accounts[3] as usize != signer_index
                || !is_writable(instruction_accounts[0] as usize)
                || !is_writable(instruction_accounts[2] as usize)
            {
                return Err(Error::InvalidInput);
            }
            let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
            if amount == 0 {
                return Err(Error::InvalidInput);
            }
            transfers.push(Transfer::Spl {
                mint: accounts[instruction_accounts[1] as usize],
                destination: accounts[instruction_accounts[2] as usize],
                amount,
                decimals: data[9],
            });
        }
    }

    if versioned {
        // Address lookup table contents are not committed to by the message. Fail closed until the
        // firmware API can authenticate the resolved accounts instead of trusting the host.
        if reader.read_shortvec()? != 0 {
            return Err(Error::InvalidInput);
        }
    }
    if !reader.finished() || transfers.is_empty() {
        return Err(Error::InvalidInput);
    }
    Ok(transfers)
}

async fn process_sign(
    hal: &mut impl crate::hal::Hal,
    request: &pb::SolanaSignTransactionRequest,
) -> Result<Response, Error> {
    validate_keypath(&request.keypath)?;
    let network = pb::SolanaNetwork::try_from(request.network)?;
    let public_key = crate::keystore::ed25519::slip10_get_pubkey_twice(hal, &request.keypath)
        .await
        .map_err(|_| Error::Generic)?;
    let transfers = parse_message(&request.message, public_key.as_bytes())?;

    if network != pb::SolanaNetwork::SolanaMainnet {
        hal.ui()
            .confirm(&ConfirmParams {
                title: "Warning",
                body: match network {
                    pb::SolanaNetwork::SolanaTestnet => "Solana testnet",
                    pb::SolanaNetwork::SolanaDevnet => "Solana devnet",
                    pb::SolanaNetwork::SolanaMainnet => unreachable!(),
                },
                accept_is_nextarrow: true,
                ..Default::default()
            })
            .await?;
    }
    for transfer in transfers {
        match transfer {
            Transfer::Sol {
                destination,
                amount,
            } => {
                hal.ui()
                    .verify_recipient(
                        &address(&destination),
                        &super::altcoin::format_amount(amount, 9, "SOL"),
                    )
                    .await?;
            }
            Transfer::Spl {
                destination,
                mint,
                amount,
                decimals,
            } => {
                if decimals > 18 {
                    return Err(Error::InvalidInput);
                }
                hal.ui()
                    .verify_recipient(
                        &address(&destination),
                        &super::altcoin::format_amount(amount, decimals as usize, "SPL"),
                    )
                    .await?;
                hal.ui()
                    .confirm(&ConfirmParams {
                        title: "Token mint",
                        body: &address(&mint),
                        scrollable: true,
                        accept_is_nextarrow: true,
                        ..Default::default()
                    })
                    .await?;
            }
        }
    }
    hal.ui().status("Transaction\nconfirmed", true).await;

    let signature = crate::keystore::ed25519::slip10_sign(hal, &request.keypath, &request.message)
        .await
        .map_err(|_| Error::Generic)?;
    if signature.public_key != public_key {
        return Err(Error::Generic);
    }
    Ok(Response::SignTransaction(
        pb::SolanaSignTransactionResponse {
            public_key: public_key.to_bytes().to_vec(),
            signature: signature.signature.to_vec(),
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

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[async_test::test]
    async fn test_address() {
        crate::keystore::testing::mock_unlocked_using_mnemonic(MNEMONIC, "");
        let public_key = crate::keystore::ed25519::slip10_get_pubkey_twice(
            &mut crate::hal::testing::TestingHal::new(),
            &[44 + HARDENED, 501 + HARDENED, HARDENED],
        )
        .await
        .unwrap();
        assert_eq!(
            address(public_key.as_bytes()),
            "GjJyeC1r2RgkuoCWMyPYkCWSGSGLcz266EaAkLA27AhL"
        );
    }

    #[test]
    fn test_parse_legacy_transfer() {
        let signer = [1u8; 32];
        let recipient = [2u8; 32];
        let mut message = vec![1, 0, 1, 3];
        message.extend_from_slice(&signer);
        message.extend_from_slice(&recipient);
        message.extend_from_slice(&SYSTEM_PROGRAM);
        message.extend_from_slice(&[3u8; 32]);
        message.extend_from_slice(&[1, 2, 2, 0, 1, 12]);
        message.extend_from_slice(&2u32.to_le_bytes());
        message.extend_from_slice(&123u64.to_le_bytes());
        let transfers = parse_message(&message, &signer).unwrap();
        assert_eq!(transfers.len(), 1);
        match &transfers[0] {
            Transfer::Sol {
                destination,
                amount,
            } => {
                assert_eq!(*destination, recipient);
                assert_eq!(*amount, 123);
            }
            _ => panic!("unexpected transfer"),
        }
    }

    #[test]
    fn test_program_ids() {
        assert_eq!(bitcoin::base58::decode(TOKEN_PROGRAM).unwrap().len(), 32);
        assert_eq!(
            bitcoin::base58::decode(TOKEN_2022_PROGRAM).unwrap().len(),
            32
        );
        assert_eq!(address(&SYSTEM_PROGRAM), "11111111111111111111111111111111");
    }

    #[test]
    fn test_parse_v0_spl_transfer() {
        let signer = [1u8; 32];
        let source = [2u8; 32];
        let destination = [3u8; 32];
        let mint = [4u8; 32];
        let token_program: [u8; 32] = bitcoin::base58::decode(TOKEN_PROGRAM)
            .unwrap()
            .try_into()
            .unwrap();
        let mut message = vec![0x80, 1, 0, 2, 5];
        for account in [signer, source, destination, mint, token_program] {
            message.extend_from_slice(&account);
        }
        message.extend_from_slice(&[5u8; 32]);
        message.extend_from_slice(&[1, 4, 4, 1, 3, 2, 0, 10, 12]);
        message.extend_from_slice(&123u64.to_le_bytes());
        message.extend_from_slice(&[6, 0]); // Decimals and no address-table lookups.

        let transfers = parse_message(&message, &signer).unwrap();
        assert_eq!(transfers.len(), 1);
        match &transfers[0] {
            Transfer::Spl {
                destination: parsed_destination,
                mint: parsed_mint,
                amount,
                decimals,
            } => {
                assert_eq!(*parsed_destination, destination);
                assert_eq!(*parsed_mint, mint);
                assert_eq!(*amount, 123);
                assert_eq!(*decimals, 6);
            }
            _ => panic!("unexpected transfer"),
        }
    }
}
