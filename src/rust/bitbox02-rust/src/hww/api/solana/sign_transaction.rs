// SPDX-License-Identifier: Apache-2.0

use super::Error;
use super::pb;

use pb::solana_response::Response;

pub async fn process(
    hal: &mut impl crate::hal::Hal,
    request: &pb::SolanaSignTransactionRequest,
) -> Result<Response, Error> {
    if request.message.is_empty() {
        return Err(Error::InvalidInput);
    }
    super::keypath::validate_address(&request.keypath)?;
    crate::workflow::verify_message::verify(
        hal,
        "Sign Solana transaction",
        "Transaction",
        &request.message,
        true,
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
    use crate::keystore;
    use crate::keystore::testing::mock_unlocked;
    use util::bb02_async::block_on;
    use util::bip32::HARDENED;

    #[test]
    fn test_process() {
        let request = pb::SolanaSignTransactionRequest {
            keypath: [44 + HARDENED, 501 + HARDENED, HARDENED, HARDENED].to_vec(),
            message: b"solana-transaction".to_vec(),
        };

        mock_unlocked();
        let expected = {
            let sig = keystore::ed25519::sign_message(
                &mut TestingHal::new(),
                &request.keypath,
                &request.message,
            )
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
                    message: b"solana-transaction".to_vec(),
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
    }
}
