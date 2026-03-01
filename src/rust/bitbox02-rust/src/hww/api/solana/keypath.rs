// SPDX-License-Identifier: Apache-2.0

use util::bip32::HARDENED;

const BIP44_PURPOSE: u32 = 44 + HARDENED;
const BIP44_COIN: u32 = 501 + HARDENED;
const BIP44_ACCOUNT_MIN: u32 = HARDENED;
const BIP44_ACCOUNT_MAX: u32 = HARDENED + 99; // 100 accounts
const BIP44_ADDRESS_MIN: u32 = HARDENED;
const BIP44_ADDRESS_MAX: u32 = HARDENED + 9999; // 10k addresses

pub struct Error;

/// Validates a Solana keypath:
/// m/44'/501'/account'/address', with account in [0', 99'] and address in [0', 9999'].
pub fn validate_address(keypath: &[u32]) -> Result<(), Error> {
    if let &[BIP44_PURPOSE, BIP44_COIN, account, address] = keypath
        && (BIP44_ACCOUNT_MIN..=BIP44_ACCOUNT_MAX).contains(&account)
        && (BIP44_ADDRESS_MIN..=BIP44_ADDRESS_MAX).contains(&address)
    {
        return Ok(());
    }
    Err(Error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_address() {
        for account in 0..100 {
            assert!(
                validate_address(&[44 + HARDENED, 501 + HARDENED, account + HARDENED, HARDENED])
                    .is_ok()
            );
        }
        assert!(
            validate_address(&[44 + HARDENED, 501 + HARDENED, 100 + HARDENED, HARDENED]).is_err()
        );
        assert!(
            validate_address(&[44 + HARDENED, 501 + HARDENED, HARDENED, 10000 + HARDENED]).is_err()
        );

        assert!(validate_address(&[45 + HARDENED, 501 + HARDENED, HARDENED, HARDENED]).is_err());
        assert!(validate_address(&[44 + HARDENED, 500 + HARDENED, HARDENED, HARDENED]).is_err());

        assert!(validate_address(&[44 + HARDENED, 501 + HARDENED, HARDENED]).is_err());
        assert!(
            validate_address(&[44 + HARDENED, 501 + HARDENED, HARDENED, HARDENED, HARDENED])
                .is_err()
        );
        assert!(
            validate_address(&[44 + HARDENED, 501 + HARDENED, HARDENED - 1, HARDENED]).is_err()
        );
        assert!(
            validate_address(&[44 + HARDENED, 501 + HARDENED, HARDENED, HARDENED - 1]).is_err()
        );
    }
}
