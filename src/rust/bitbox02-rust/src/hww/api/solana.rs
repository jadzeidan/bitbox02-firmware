// SPDX-License-Identifier: Apache-2.0

#[cfg(not(feature = "app-solana"))]
compile_error!("Solana code is being compiled even though the app-solana feature is not enabled");

pub mod keypath;
mod pubrequest;
mod sign_transaction;

use super::Error;
use super::pb;

use pb::solana_request::Request;
use pb::solana_response::Response;

/// Handle a Solana protobuf api call.
pub async fn process_api(
    hal: &mut impl crate::hal::Hal,
    request: &Request,
) -> Result<Response, Error> {
    match request {
        Request::Pub(request) => pubrequest::process(hal, request).await,
        Request::SignTransaction(request) => sign_transaction::process(hal, request).await,
    }
}
