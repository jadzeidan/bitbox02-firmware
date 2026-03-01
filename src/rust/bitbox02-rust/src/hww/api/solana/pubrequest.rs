// SPDX-License-Identifier: Apache-2.0

use super::Error;
use super::pb;
use crate::hal::Ui;
use crate::hal::ui::ConfirmParams;

use pb::solana_response::Response;

pub async fn process(
    hal: &mut impl crate::hal::Hal,
    request: &pb::SolanaPubRequest,
) -> Result<Response, Error> {
    super::keypath::validate_address(&request.keypath)?;
    let xpub =
        crate::keystore::ed25519::get_xpub(hal, &request.keypath).or(Err(Error::InvalidInput))?;
    let address = bitcoin::base58::encode(xpub.pubkey_bytes());

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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::hal::testing::TestingHal;
    use crate::hal::testing::ui::Screen;
    use crate::keystore;
    use crate::keystore::testing::mock_unlocked;
    use util::bb02_async::block_on;
    use util::bip32::HARDENED;

    #[test]
    fn test_process() {
        let request = pb::SolanaPubRequest {
            keypath: [44 + HARDENED, 501 + HARDENED, HARDENED, HARDENED].to_vec(),
            display: false,
        };

        mock_unlocked();
        let expected = {
            let xpub =
                keystore::ed25519::get_xpub(&mut TestingHal::new(), &request.keypath).unwrap();
            bitcoin::base58::encode(xpub.pubkey_bytes())
        };
        assert_eq!(
            block_on(process(&mut TestingHal::new(), &request)),
            Ok(Response::Pub(pb::PubResponse { r#pub: expected }))
        );

        mock_unlocked();
        let mut hal = TestingHal::new();
        let expected_display = {
            let xpub = keystore::ed25519::get_xpub(
                &mut TestingHal::new(),
                &[44 + HARDENED, 501 + HARDENED, HARDENED, HARDENED],
            )
            .unwrap();
            bitcoin::base58::encode(xpub.pubkey_bytes())
        };
        assert!(
            block_on(process(
                &mut hal,
                &pb::SolanaPubRequest {
                    keypath: [44 + HARDENED, 501 + HARDENED, HARDENED, HARDENED].to_vec(),
                    display: true,
                }
            ))
            .is_ok()
        );
        assert_eq!(
            hal.ui.screens,
            vec![Screen::Confirm {
                title: "Solana".into(),
                body: expected_display,
                longtouch: false,
            }]
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
                &pb::SolanaPubRequest {
                    keypath: [44 + HARDENED, 60 + HARDENED, HARDENED, HARDENED].to_vec(),
                    display: false,
                }
            )),
            Err(Error::InvalidInput)
        );
    }
}
