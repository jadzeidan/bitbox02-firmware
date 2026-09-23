# SPDX-License-Identifier: Apache-2.0

"""Tests for the Solana, XRP, Tron, and transparent Zcash Python APIs."""

import unittest
from unittest import mock

from bitbox02 import BitBox02, common, hww, solana, tron, xrp, zcash

# Protobuf fields, message classes, and enum constants are generated dynamically.
# pylint: disable=no-member


class TestAltcoins(unittest.TestCase):
    """Verify that public methods construct and unwrap the expected protobufs."""

    def setUp(self) -> None:
        self.device = object.__new__(BitBox02)
        self.device._msg_query = mock.Mock()  # type: ignore[attr-defined]  # pylint: disable=protected-access

    def test_solana(self) -> None:
        self.device._msg_query.return_value = hww.Response(  # pylint: disable=protected-access
            solana=solana.SolanaResponse(pub=common.PubResponse(pub="solana-address"))
        )
        self.assertEqual(self.device.solana_address([1, 2, 3], display=False), "solana-address")
        request = self.device._msg_query.call_args.args[0]  # pylint: disable=protected-access
        self.assertEqual(list(request.solana.pub.keypath), [1, 2, 3])
        self.assertFalse(request.solana.pub.display)

        expected = solana.SolanaSignTransactionResponse(public_key=b"p", signature=b"s")
        self.device._msg_query.return_value = hww.Response(  # pylint: disable=protected-access
            solana=solana.SolanaResponse(sign_transaction=expected)
        )
        response = self.device.solana_sign_transaction(solana.SOLANA_DEVNET, [4], b"message")
        self.assertEqual(response, expected)
        request = self.device._msg_query.call_args.args[0]  # pylint: disable=protected-access
        self.assertEqual(request.solana.sign_transaction.message, b"message")

    def test_xrp(self) -> None:
        self.device._msg_query.return_value = hww.Response(  # pylint: disable=protected-access
            xrp=xrp.XrpResponse(pub=common.PubResponse(pub="xrp-address"))
        )
        self.assertEqual(self.device.xrp_address([1], display=True), "xrp-address")

        payment = xrp.XrpSignPaymentRequest(destination="destination", amount=1, fee=1, sequence=1)
        expected = xrp.XrpSignPaymentResponse(signature=b"s", serialized_transaction=b"tx")
        self.device._msg_query.return_value = hww.Response(  # pylint: disable=protected-access
            xrp=xrp.XrpResponse(sign_payment=expected)
        )
        self.assertEqual(self.device.xrp_sign_payment(payment), expected)
        request = self.device._msg_query.call_args.args[0]  # pylint: disable=protected-access
        self.assertEqual(request.xrp.sign_payment, payment)

    def test_tron(self) -> None:
        self.device._msg_query.return_value = hww.Response(  # pylint: disable=protected-access
            tron=tron.TronResponse(pub=common.PubResponse(pub="tron-address"))
        )
        self.assertEqual(self.device.tron_address([1], display=False), "tron-address")

        expected = tron.TronSignTransactionResponse(signature=b"signature")
        self.device._msg_query.return_value = hww.Response(  # pylint: disable=protected-access
            tron=tron.TronResponse(sign_transaction=expected)
        )
        self.assertEqual(
            self.device.tron_sign_transaction(tron.TRON_TESTNET, [2], b"raw-data"), expected
        )
        request = self.device._msg_query.call_args.args[0]  # pylint: disable=protected-access
        self.assertEqual(request.tron.sign_transaction.raw_data, b"raw-data")

    def test_zcash(self) -> None:
        self.device._msg_query.return_value = hww.Response(  # pylint: disable=protected-access
            zcash=zcash.ZcashResponse(pub=common.PubResponse(pub="zcash-address"))
        )
        self.assertEqual(
            self.device.zcash_address(zcash.ZCASH_TESTNET, [1], display=False), "zcash-address"
        )

        transaction = zcash.ZcashSignTransactionRequest(expiry_height=1)
        expected = zcash.ZcashSignTransactionResponse(
            signatures=[b"signature"], serialized_transaction=b"tx"
        )
        self.device._msg_query.return_value = hww.Response(  # pylint: disable=protected-access
            zcash=zcash.ZcashResponse(sign_transaction=expected)
        )
        self.assertEqual(self.device.zcash_sign_transaction(transaction), expected)
        request = self.device._msg_query.call_args.args[0]  # pylint: disable=protected-access
        self.assertEqual(request.zcash.sign_transaction, transaction)


if __name__ == "__main__":
    unittest.main()
