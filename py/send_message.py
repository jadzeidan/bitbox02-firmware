#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0

"""Script for interacting with bitbox v2"""

# pylint: disable=too-many-lines

import argparse
import hashlib
import socket
import pprint
import sys
from typing import List, Any, Optional, Callable, Union, Tuple, Sequence
import base64
import binascii
import textwrap
import json
from pathlib import Path
import os

import requests
import base58
import hid
import semver
from tzlocal import get_localzone

from bitbox02 import util
from bitbox02 import bitbox02
from bitbox02.bitbox02 import Bootloader
from bitbox02.communication import (
    devices,
    HARDENED,
    Bitbox02Exception,
    UserAbortException,
    FirmwareVersionOutdatedException,
    u2fhid,
    bitbox_api_protocol,
    PhysicalLayer,
)

import u2f
import u2f.bitbox02

try:
    # Optional rlp dependency only needed to sign ethereum transactions.
    # pylint: disable=import-error
    import rlp
except ModuleNotFoundError:
    pass

try:
    # Optional bech32 dependency only needed for Bitcoin signing demos.
    # pylint: disable=import-error
    import bech32
except ModuleNotFoundError:
    pass


def eprint(*args: Any, **kwargs: Any) -> None:
    """
    Like print, but defaults to stderr.
    """
    kwargs.setdefault("file", sys.stderr)
    print(*args, **kwargs)


def ask_user(
    choices: Sequence[Tuple[str, Callable[[], None]]],
) -> Union[Callable[[], None], bool, None]:
    """Ask user to choose one of the choices, q quits"""
    print("What would you like to do?")
    for idx, choice in enumerate(choices):
        print(f"- ({idx+1}) {choice[0]}")
    print("- (q) Quit")
    ans_str = input("")
    if ans_str == "q":
        return False
    try:
        ans = int(ans_str)
        if ans < 1 or ans > len(choices):
            raise ValueError("Out of range")
    except ValueError:
        print("Invalid input")
        return None
    return choices[ans - 1][1]


BITBOXSYNC_CHALLENGE = bytes([0x10]) * 32
BITBOXSYNC_NAMESPACE_ID = bytes(range(0x20, 0x30))
BITBOXSYNC_INVITE_ID = bytes(range(0x30, 0x40))
BITBOXSYNC_INVITE_SERVER_SECRET_HASH = bytes(range(0x40, 0x60))
BITBOXSYNC_NAMESPACE_DEK = bytes(range(0x60, 0x80))
BITBOXSYNC_EXPIRES_AT = 1893456000  # 2030-01-01 00:00:00 UTC
BITBOXSYNC_SERVER_ORIGIN = "https://sync.example.com"
BITBOXSYNC_MAX_ACCEPTED = 5


def parse_keypath(value: str) -> List[int]:
    """Parse a BIP32 keypath such as m/44'/144'/0'/0/0."""
    components = value.strip().split("/")
    if components and components[0].lower() == "m":
        components = components[1:]
    if not components or any(not component for component in components):
        raise argparse.ArgumentTypeError("keypath must contain at least one component")

    result = []
    for component in components:
        hardened = component[-1:] in ("'", "h", "H")
        number = component[:-1] if hardened else component
        try:
            index = int(number, 10)
        except ValueError as exc:
            raise argparse.ArgumentTypeError(f"invalid keypath component: {component}") from exc
        if index < 0 or index >= HARDENED:
            raise argparse.ArgumentTypeError(f"keypath component out of range: {component}")
        result.append(index + (HARDENED if hardened else 0))
    return result


def parse_hex(value: str) -> bytes:
    """Parse an optionally 0x-prefixed hexadecimal byte string."""
    value = value.strip()
    if value.startswith(("0x", "0X")):
        value = value[2:]
    try:
        return bytes.fromhex(value)
    except ValueError as exc:
        raise argparse.ArgumentTypeError("expected an even-length hexadecimal byte string") from exc


def parse_int(value: Union[str, int]) -> int:
    """Parse a decimal integer or a string with a Python-style base prefix."""
    if isinstance(value, int):
        return value
    return int(value, 0)


def _run_cli_command(
    device: bitbox02.BitBox02,
    debug: bool,
    command: Optional[Callable[[bitbox02.BitBox02], None]],
) -> int:
    if command is None:
        return SendMessage(device, debug).run()
    if debug:
        device.debug = True
    try:
        command(device)
    except UserAbortException:
        eprint("Aborted by user")
        return 1
    finally:
        device.close()
    return 0


# Protobuf fields and enum constants are generated dynamically.
# pylint: disable=no-member
def _command_solana_address(device: bitbox02.BitBox02, args: argparse.Namespace) -> None:
    print(device.solana_address(args.keypath, display=args.display))


def _command_solana_sign(device: bitbox02.BitBox02, args: argparse.Namespace) -> None:
    network = {
        "mainnet": bitbox02.solana.SOLANA_MAINNET,
        "testnet": bitbox02.solana.SOLANA_TESTNET,
        "devnet": bitbox02.solana.SOLANA_DEVNET,
    }[args.network]
    response = device.solana_sign_transaction(network, args.keypath, args.message_hex)
    print(f"public_key: {response.public_key.hex()}")
    print(f"signature: {response.signature.hex()}")


def _command_xrp_address(device: bitbox02.BitBox02, args: argparse.Namespace) -> None:
    print(device.xrp_address(args.keypath, display=args.display))


def _command_xrp_sign(device: bitbox02.BitBox02, args: argparse.Namespace) -> None:
    network = {
        "mainnet": bitbox02.xrp.XRP_MAINNET,
        "testnet": bitbox02.xrp.XRP_TESTNET,
    }[args.network]
    memo = args.memo.encode("utf-8") if args.memo is not None else args.memo_hex or b""
    request = bitbox02.xrp.XrpSignPaymentRequest(
        network=network,
        keypath=args.keypath,
        destination=args.destination,
        amount=args.amount,
        fee=args.fee,
        sequence=args.sequence,
        memo=memo,
    )
    if args.destination_tag is not None:
        request.destination_tag = args.destination_tag
    if args.last_ledger_sequence is not None:
        request.last_ledger_sequence = args.last_ledger_sequence
    response = device.xrp_sign_payment(request)
    print(f"signature: {response.signature.hex()}")
    print(f"serialized_transaction: {response.serialized_transaction.hex()}")


def _command_tron_address(device: bitbox02.BitBox02, args: argparse.Namespace) -> None:
    print(device.tron_address(args.keypath, display=args.display))


def _command_tron_sign(device: bitbox02.BitBox02, args: argparse.Namespace) -> None:
    network = {
        "mainnet": bitbox02.tron.TRON_MAINNET,
        "testnet": bitbox02.tron.TRON_TESTNET,
    }[args.network]
    response = device.tron_sign_transaction(network, args.keypath, args.raw_data_hex)
    print(f"signature: {response.signature.hex()}")


def _load_zcash_transaction(
    filename: str, network: "bitbox02.zcash.ZcashNetwork.V"
) -> "bitbox02.zcash.ZcashSignTransactionRequest":
    if filename == "-":
        document = json.load(sys.stdin)
    else:
        with Path(filename).open("r", encoding="utf-8") as file:
            document = json.load(file)
    try:
        inputs = [
            bitbox02.zcash.ZcashSignTransactionRequest.Input(
                keypath=parse_keypath(item["keypath"]),
                prev_out_hash=parse_hex(item["prev_out_hash"]),
                prev_out_index=parse_int(item["prev_out_index"]),
                value=parse_int(item["value"]),
                script_pubkey=parse_hex(item["script_pubkey"]),
                sequence=parse_int(item["sequence"]),
            )
            for item in document["inputs"]
        ]
        outputs = [
            bitbox02.zcash.ZcashSignTransactionRequest.Output(
                value=parse_int(item["value"]),
                script_pubkey=parse_hex(item["script_pubkey"]),
                keypath=parse_keypath(item["keypath"]) if item.get("keypath") else [],
            )
            for item in document["outputs"]
        ]
        return bitbox02.zcash.ZcashSignTransactionRequest(
            network=network,
            inputs=inputs,
            outputs=outputs,
            lock_time=parse_int(document.get("lock_time", 0)),
            expiry_height=parse_int(document["expiry_height"]),
            consensus_branch_id=parse_int(document["consensus_branch_id"]),
        )
    except (KeyError, TypeError, ValueError, argparse.ArgumentTypeError) as exc:
        raise ValueError(f"invalid Zcash transaction JSON: {exc}") from exc


def _command_zcash_address(device: bitbox02.BitBox02, args: argparse.Namespace) -> None:
    network = {
        "mainnet": bitbox02.zcash.ZCASH_MAINNET,
        "testnet": bitbox02.zcash.ZCASH_TESTNET,
    }[args.network]
    print(device.zcash_address(network, args.keypath, display=args.display))


def _command_zcash_sign(device: bitbox02.BitBox02, args: argparse.Namespace) -> None:
    network = {
        "mainnet": bitbox02.zcash.ZCASH_MAINNET,
        "testnet": bitbox02.zcash.ZCASH_TESTNET,
    }[args.network]
    response = device.zcash_sign_transaction(_load_zcash_transaction(args.transaction, network))
    for index, signature in enumerate(response.signatures):
        print(f"signature[{index}]: {signature.hex()}")
    print(f"serialized_transaction: {response.serialized_transaction.hex()}")


def _add_altcoin_commands(parser: argparse.ArgumentParser) -> None:
    subparsers = parser.add_subparsers(dest="command", metavar="COMMAND")

    solana_address = subparsers.add_parser("solana-address", help="derive a Solana address")
    solana_address.add_argument(
        "--keypath", type=parse_keypath, default=parse_keypath("m/44'/501'/0'")
    )
    solana_address.add_argument("--display", action="store_true", help="confirm on the device")
    solana_address.set_defaults(command_handler=_command_solana_address)

    solana_sign = subparsers.add_parser("solana-sign", help="sign a serialized Solana message")
    solana_sign.add_argument(
        "--network", choices=("mainnet", "testnet", "devnet"), default="mainnet"
    )
    solana_sign.add_argument(
        "--keypath", type=parse_keypath, default=parse_keypath("m/44'/501'/0'")
    )
    solana_sign.add_argument("--message-hex", type=parse_hex, required=True)
    solana_sign.set_defaults(command_handler=_command_solana_sign)

    xrp_address = subparsers.add_parser("xrp-address", help="derive an XRP classic address")
    xrp_address.add_argument(
        "--keypath", type=parse_keypath, default=parse_keypath("m/44'/144'/0'/0/0")
    )
    xrp_address.add_argument("--display", action="store_true", help="confirm on the device")
    xrp_address.set_defaults(command_handler=_command_xrp_address)

    xrp_sign = subparsers.add_parser("xrp-sign", help="sign an XRP Payment")
    xrp_sign.add_argument("--network", choices=("mainnet", "testnet"), default="mainnet")
    xrp_sign.add_argument(
        "--keypath", type=parse_keypath, default=parse_keypath("m/44'/144'/0'/0/0")
    )
    xrp_sign.add_argument("--destination", required=True)
    xrp_sign.add_argument("--amount", type=parse_int, required=True, help="amount in drops")
    xrp_sign.add_argument("--fee", type=parse_int, required=True, help="fee in drops")
    xrp_sign.add_argument("--sequence", type=parse_int, required=True)
    xrp_sign.add_argument("--destination-tag", type=parse_int)
    xrp_sign.add_argument("--last-ledger-sequence", type=parse_int)
    memo = xrp_sign.add_mutually_exclusive_group()
    memo.add_argument("--memo", help="UTF-8 memo")
    memo.add_argument("--memo-hex", type=parse_hex, help="raw memo bytes as hex")
    xrp_sign.set_defaults(command_handler=_command_xrp_sign)

    tron_address = subparsers.add_parser("tron-address", help="derive a Tron address")
    tron_address.add_argument(
        "--keypath", type=parse_keypath, default=parse_keypath("m/44'/195'/0'/0/0")
    )
    tron_address.add_argument("--display", action="store_true", help="confirm on the device")
    tron_address.set_defaults(command_handler=_command_tron_address)

    tron_sign = subparsers.add_parser("tron-sign", help="sign serialized Tron raw_data")
    tron_sign.add_argument("--network", choices=("mainnet", "testnet"), default="mainnet")
    tron_sign.add_argument(
        "--keypath", type=parse_keypath, default=parse_keypath("m/44'/195'/0'/0/0")
    )
    tron_sign.add_argument("--raw-data-hex", type=parse_hex, required=True)
    tron_sign.set_defaults(command_handler=_command_tron_sign)

    zcash_address = subparsers.add_parser(
        "zcash-address", help="derive a transparent Zcash address"
    )
    zcash_address.add_argument("--network", choices=("mainnet", "testnet"), default="mainnet")
    zcash_address.add_argument(
        "--keypath", type=parse_keypath, default=parse_keypath("m/44'/133'/0'/0/0")
    )
    zcash_address.add_argument("--display", action="store_true", help="confirm on the device")
    zcash_address.set_defaults(command_handler=_command_zcash_address)

    zcash_sign = subparsers.add_parser("zcash-sign", help="sign a transparent Zcash v5 transaction")
    zcash_sign.add_argument("--network", choices=("mainnet", "testnet"), default="mainnet")
    zcash_sign.add_argument(
        "--transaction", required=True, metavar="JSON", help="JSON file, or - for stdin"
    )
    zcash_sign.set_defaults(command_handler=_command_zcash_sign)


# pylint: enable=no-member


def _solana_demo_sol_message(signer: bytes) -> bytes:
    if len(signer) != 32:
        raise ValueError("invalid Solana signer address")
    recipient = bytes([2]) * 32
    system_program = bytes(32)
    recent_blockhash = bytes([3]) * 32
    message = bytearray([1, 0, 1, 3])
    message.extend(signer)
    message.extend(recipient)
    message.extend(system_program)
    message.extend(recent_blockhash)
    message.extend([1, 2, 2, 0, 1, 12])
    message.extend((2).to_bytes(4, "little"))  # SystemProgram::Transfer.
    message.extend((1_000_000).to_bytes(8, "little"))  # 0.001 SOL.
    return bytes(message)


def _solana_demo_spl_message(signer: bytes) -> bytes:
    if len(signer) != 32:
        raise ValueError("invalid Solana signer address")
    source = bytes([2]) * 32
    destination = bytes([3]) * 32
    mint = bytes([4]) * 32
    token_program = base58.b58decode("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
    recent_blockhash = bytes([5]) * 32
    message = bytearray([0x80, 1, 0, 2, 5])
    for account in (signer, source, destination, mint, token_program):
        message.extend(account)
    message.extend(recent_blockhash)
    message.extend([1, 4, 4, 1, 3, 2, 0, 10, 12])
    message.extend((1_000_000).to_bytes(8, "little"))  # 1 token at 6 decimals.
    message.extend([6, 0])  # Token decimals and no address-table lookups.
    return bytes(message)


def _protobuf_varint(value: int) -> bytes:
    result = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        result.append(byte | (0x80 if value else 0))
        if not value:
            return bytes(result)


def _protobuf_field_varint(field: int, value: int) -> bytes:
    return _protobuf_varint(field << 3) + _protobuf_varint(value)


def _protobuf_field_bytes(field: int, value: bytes) -> bytes:
    return _protobuf_varint((field << 3) | 2) + _protobuf_varint(len(value)) + value


def _tron_raw_data(contract_type: int, type_url: bytes, value: bytes, fee_limit: int = 0) -> bytes:
    parameter = _protobuf_field_bytes(1, type_url) + _protobuf_field_bytes(2, value)
    contract = _protobuf_field_varint(1, contract_type) + _protobuf_field_bytes(2, parameter)
    raw_data = (
        _protobuf_field_bytes(1, b"\x00\x01")
        + _protobuf_field_bytes(4, bytes(8))
        + _protobuf_field_varint(8, 2_000)
        + _protobuf_field_bytes(11, contract)
        + _protobuf_field_varint(14, 1_000)
    )
    if fee_limit:
        raw_data += _protobuf_field_varint(18, fee_limit)
    return raw_data


def _tron_demo_trx_raw_data(owner: bytes) -> bytes:
    if len(owner) != 21 or owner[0] != 0x41:
        raise ValueError("invalid Tron owner address")
    destination = b"\x41" + bytes([0x22]) * 20
    transfer = (
        _protobuf_field_bytes(1, owner)
        + _protobuf_field_bytes(2, destination)
        + _protobuf_field_varint(3, 1_000_000)  # 1 TRX.
    )
    return _tron_raw_data(
        1,
        b"type.googleapis.com/protocol.TransferContract",
        transfer,
    )


def _tron_demo_trc20_raw_data(owner: bytes) -> bytes:
    if len(owner) != 21 or owner[0] != 0x41:
        raise ValueError("invalid Tron owner address")
    contract = b"\x41" + bytes([0x33]) * 20
    destination = bytes([0x22]) * 20
    call_data = bytearray(68)
    call_data[:4] = bytes.fromhex("a9059cbb")  # transfer(address,uint256).
    call_data[16:36] = destination
    call_data[60:] = (1_000_000).to_bytes(8, "big")
    trigger = (
        _protobuf_field_bytes(1, owner)
        + _protobuf_field_bytes(2, contract)
        + _protobuf_field_bytes(4, bytes(call_data))
    )
    return _tron_raw_data(
        31,
        b"type.googleapis.com/protocol.TriggerSmartContract",
        trigger,
        fee_limit=10_000_000,
    )


def _zcash_p2pkh_script(address: str) -> bytes:
    decoded = base58.b58decode_check(address)
    if len(decoded) != 22 or decoded[:2] != b"\x1c\xb8":
        raise ValueError("invalid Zcash mainnet transparent address")
    return b"\x76\xa9\x14" + decoded[2:] + b"\x88\xac"


def _btc_demo_inputs_outputs(
    device: bitbox02.BitBox02,
    bip44_account: int,
    coin: "bitbox02.btc.BTCCoin.V" = bitbox02.btc.BTC,
    script_configs: Optional[Sequence[bitbox02.btc.BTCScriptConfigWithKeypath]] = None,
) -> Tuple[List[bitbox02.BTCInputType], List[bitbox02.BTCOutputType]]:
    """
    Returns a sample btc tx.
    """

    def address_to_pkscript(address: str) -> bytes:
        lowercase_address = address.lower()
        if lowercase_address.startswith(("bc1", "tb1", "bcrt1", "ltc1", "tltc1")):
            separator_pos = lowercase_address.rfind("1")
            assert separator_pos > 0
            witness_version, witness_program = bech32.decode(
                lowercase_address[:separator_pos], address
            )
            assert witness_version == 0
            assert witness_program is not None
            return bytes([0, len(witness_program)]) + bytes(witness_program)

        decoded = base58.b58decode_check(address)
        assert len(decoded) == 21
        return b"\xa9\x14" + decoded[1:] + b"\x87"

    def make_prev_tx(
        pubkey_script: bytes,
    ) -> Tuple[bytes, Any]:
        version = 1
        locktime = 0
        value = int(1e8 * 0.60005)
        prev_out_hash = b"11111111111111111111111111111111"
        prev_out_index = 0
        signature_script = b"some signature script"
        sequence = 0xFFFFFFFF
        prev_tx = {
            "version": version,
            "locktime": locktime,
            "inputs": [
                {
                    "prev_out_hash": prev_out_hash,
                    "prev_out_index": prev_out_index,
                    "signature_script": signature_script,
                    "sequence": sequence,
                }
            ],
            "outputs": [{"value": value, "pubkey_script": pubkey_script}],
        }
        serialized = (
            version.to_bytes(4, "little")
            + b"\x01"
            + prev_out_hash
            + prev_out_index.to_bytes(4, "little")
            + bytes([len(signature_script)])
            + signature_script
            + sequence.to_bytes(4, "little")
            + b"\x01"
            + value.to_bytes(8, "little")
            + bytes([len(pubkey_script)])
            + pubkey_script
            + locktime.to_bytes(4, "little")
        )
        return hashlib.sha256(hashlib.sha256(serialized).digest()).digest(), prev_tx

    if script_configs is None:
        script_configs = [
            bitbox02.btc.BTCScriptConfigWithKeypath(
                script_config=bitbox02.btc.BTCScriptConfig(
                    simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                ),
                keypath=[84 + HARDENED, 0 + HARDENED, bip44_account],
            ),
            bitbox02.btc.BTCScriptConfigWithKeypath(
                script_config=bitbox02.btc.BTCScriptConfig(
                    simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
                ),
                keypath=[49 + HARDENED, 0 + HARDENED, bip44_account],
            ),
        ]
    assert len(script_configs) in (1, 2)

    inputs: List[bitbox02.BTCInputType] = []
    for input_index in range(2):
        script_config_index = input_index if len(script_configs) == 2 else 0
        script_config = script_configs[script_config_index]
        keypath = list(script_config.keypath) + [0, input_index]
        address = device.btc_address(
            coin=coin,
            keypath=keypath,
            script_config=script_config.script_config,
            display=False,
        )
        prev_out_hash, prev_tx = make_prev_tx(address_to_pkscript(address))
        inputs.append(
            {
                "prev_out_hash": prev_out_hash,
                "prev_out_index": 0,
                "prev_out_value": int(1e8 * 0.60005),
                "sequence": 0xFFFFFFFF,
                "keypath": keypath,
                "script_config_index": script_config_index,
                "prev_tx": prev_tx,
            }
        )
    outputs: List[bitbox02.BTCOutputType] = [
        bitbox02.BTCOutputInternal(
            keypath=[84 + HARDENED, 0 + HARDENED, bip44_account, 1, 0],
            value=int(1e8 * 1),
            script_config_index=0,
        ),
        bitbox02.BTCOutputExternal(
            output_type=bitbox02.btc.P2WSH,
            output_payload=b"11111111111111111111111111111111",
            value=int(1e8 * 0.2),
        ),
    ]
    return inputs, outputs


class SendMessage:
    """SendMessage"""

    def __init__(self, device: bitbox02.BitBox02, debug: bool):
        self._device = device
        self._debug = debug
        self._stop = False

    def _change_name_workflow(self, name: Optional[str] = None) -> None:
        """
        Invoke change name workfow.
        """
        if name is None:
            name = input("Enter a name [Mia] (max 64 bytes): ")
            if not name:
                name = "Mia"
        info = self._device.device_info()
        print(f"Old device name: {info['name']}")
        try:
            self._device.set_device_name(name)
        except UserAbortException:
            eprint("Aborted by user")
        else:
            print("Setting new device name.")
            info = self._device.device_info()
            print(f"New device name: {info['name']}")

    def _setup_workflow(self) -> None:
        """TODO: Document"""
        self._change_name_workflow()

        entropy_size_inp = input(
            "Choose a seed size: 32 (24 words) or 16 (12 words). Default is 32: "
        )

        if not entropy_size_inp:
            entropy_size = 32
        else:
            try:
                entropy_size = int(entropy_size_inp)
            except ValueError:
                eprint("Failed")
                return

        if entropy_size not in (16, 32):
            eprint("You must enter 16 or 32")
            return

        print(
            "Please choose a password of the BitBox02. "
            + "This password will be used to unlock your BitBox02."
        )
        while not self._device.set_password(entropy_size=entropy_size):
            eprint("Passwords did not match. please try again")

        print("Your BitBox02 will now create a backup of your wallet...")

        def backup_sd() -> None:
            if not self._device.create_backup():
                eprint("Creating the backup failed")
                return
            print("Backup created sucessfully")
            print("Please Remove SD Card")

        def backup_mnemonic() -> None:
            if self._device.version < semver.VersionInfo(9, 13, 0):
                eprint("Backing up using recovery words is supported from firmware version 9.13.0")
                return

            try:
                self._device.show_mnemonic()
            except Bitbox02Exception:
                eprint("Creating the backup failed")
                return
            print("Backup created sucessfully")

        choice = ask_user(
            (
                ("Backup onto a microSD card", backup_sd),
                ("Backup manually by writing down recovey words", backup_mnemonic),
            ),
        )
        if callable(choice):
            try:
                choice()
            except UserAbortException:
                eprint("Aborted by user")

    def _print_backups(self, backups: Optional[Sequence[bitbox02.Backup]] = None) -> None:
        local_timezone = get_localzone()
        if backups is None:
            backups = list(self._device.list_backups())
        if not backups:
            print("No backups found.")
            return
        fmt = "%Y-%m-%d %H:%M:%S %z"
        for i, (backup_id, backup_name, date) in enumerate(backups):
            date = date.astimezone(local_timezone)
            date_str = date.strftime(fmt)
            print(f"[{i+1}] Backup Name: {backup_name}, Time: {date_str}, ID: {backup_id}")

    def _restore_backup_workflow(self) -> None:
        """TODO: Document"""
        backups = list(self._device.list_backups())
        self._print_backups(backups)
        if not backups:
            return
        ans = input(f"Choose a backup [1-{len(backups)}]: ")
        try:
            item = int(ans)
            if item < 1 or item > len(backups):
                raise ValueError("Out of range")
        except ValueError:
            eprint("Invalid input")
            return
        backup_id, _, _ = backups[item - 1]
        print(f"ID: {backup_id}")
        try:
            self._device.restore_backup(backup_id)
        except UserAbortException:
            eprint("Aborted by user")
            return
        except Bitbox02Exception:
            eprint("Restoring backup failed")
            return
        print("Please Remove SD Card")

    def _restore_from_mnemonic(self) -> None:
        try:
            self._device.restore_from_mnemonic()
            print("Restore successful")
        except UserAbortException:
            print("Aborted by user")

    def _list_device_info(self) -> None:
        pprint.pprint(self._device.device_info())

    def _reboot(self) -> None:
        inp = input("Select one of: 1=upgrade; 2=go to startup settings: ").strip()
        purpose = {
            "1": bitbox02.system.RebootRequest.Purpose.UPGRADE,  # pylint: disable=no-member
            "2": bitbox02.system.RebootRequest.Purpose.SETTINGS,  # pylint: disable=no-member
        }[inp]
        if self._device.reboot(purpose=purpose):
            print("Device rebooted")
            self._stop = True
        else:
            print("User aborted")

    def _check_sd_presence(self) -> None:
        print(f"SD Card inserted: {self._device.check_sdcard()}")

    def _insert_sdcard(self) -> None:
        try:
            self._device.insert_sdcard()
        except UserAbortException:
            print("Aborted by user")

    def _get_root_fingerprint(self) -> None:
        print(f"Root fingerprint: {self._device.root_fingerprint().hex()}")

    def _display_zpub(self) -> None:
        try:
            print(
                "m/84'/0'/0' zpub: ",
                self._device.btc_xpub(
                    keypath=[84 + HARDENED, 0 + HARDENED, 0 + HARDENED],
                    xpub_type=bitbox02.btc.BTCPubRequest.ZPUB,  # pylint: disable=no-member
                ),
            )
        except UserAbortException:
            eprint("Aborted by user")

    def _btc_xpubs(self) -> None:
        xpubs = self._device.btc_xpubs(
            keypaths=[[84 + HARDENED, 0 + HARDENED, i + HARDENED] for i in range(20)],
        )
        print("xpubs for m/84'/0'/{0'..19'}:")
        for xpub in xpubs:
            print(xpub)

    def _get_electrum_encryption_key(self) -> None:
        print(
            "Electrum wallet encryption xpub at keypath m/4541509'/1112098098':",
            self._device.electrum_encryption_key(
                keypath=[4541509 + HARDENED, 1112098098 + HARDENED]
            ),
        )

    def _bip85_bip39(self) -> None:
        try:
            self._device.bip85_bip39()
        except UserAbortException:
            print("Aborted by user")

    def _bip85_ln(self) -> None:
        try:
            entropy = self._device.bip85_ln()
            print("Derived entropy for a Breez Lightning wallet:", entropy.hex())
        except UserAbortException:
            print("Aborted by user")

    def _btc_address(self) -> None:
        def address(display: bool) -> str:
            # pylint: disable=no-member
            return self._device.btc_address(
                keypath=[84 + HARDENED, 0 + HARDENED, 0 + HARDENED, 0, 0],
                script_config=bitbox02.btc.BTCScriptConfig(
                    simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                ),
                display=display,
            )

        print("m/84'/0'/0'/0/0 address: ", address(False))
        address(True)

    def _btc_multisig_config(self, coin: "bitbox02.btc.BTCCoin.V") -> bitbox02.btc.BTCScriptConfig:
        """
        Get a mock multisig 1-of-2 multisig with the current device and some other arbitrary xpub.
        Registers it on the device if not already registered.
        """
        account_keypath = [48 + HARDENED, 0 + HARDENED, 0 + HARDENED, 2 + HARDENED]

        my_xpub = self._device.btc_xpub(
            keypath=account_keypath,
            coin=coin,
            xpub_type=bitbox02.btc.BTCPubRequest.XPUB,
            display=False,
        )
        multisig_config = bitbox02.btc.BTCScriptConfig(
            multisig=bitbox02.btc.BTCScriptConfig.Multisig(
                threshold=1,
                xpubs=[
                    util.parse_xpub(my_xpub),
                    util.parse_xpub(
                        "xpub6FEZ9Bv73h1vnE4TJG4QFj2RPXJhhsPbnXgFyH3ErLvpcZrDcynY65bhWga8PazW"
                        "HLSLi23PoBhGcLcYW6JRiJ12zXZ9Aop4LbAqsS3gtcy"
                    ),
                ],
                our_xpub_index=0,
            )
        )

        is_registered = self._device.btc_is_script_config_registered(
            coin, multisig_config, account_keypath
        )
        if is_registered:
            print("Multisig account already registered on the device.")
        else:
            multisig_name = input("Enter a name for the multisig account: ").strip()
            self._device.btc_register_script_config(
                coin=coin,
                script_config=multisig_config,
                keypath=account_keypath,
                name=multisig_name,
            )

        return multisig_config

    def _btc_policy_config(self, coin: "bitbox02.btc.BTCCoin.V") -> bitbox02.btc.BTCScriptConfig:
        account_keypath = [48 + HARDENED, 1 + HARDENED, 0 + HARDENED, 3 + HARDENED]
        our_xpub = self._device.btc_xpub(
            keypath=account_keypath,
            coin=coin,
            xpub_type=bitbox02.btc.BTCPubRequest.XPUB,
            display=False,
        )
        our_root_fingerprint = self._device.root_fingerprint()

        policy_config = bitbox02.btc.BTCScriptConfig(
            policy=bitbox02.btc.BTCScriptConfig.Policy(
                policy="wsh(and_v(v:pk(@0/**),pk(@1/**)))",
                keys=[
                    bitbox02.common.KeyOriginInfo(
                        root_fingerprint=our_root_fingerprint,
                        keypath=account_keypath,
                        xpub=util.parse_xpub(our_xpub),
                    ),
                    bitbox02.common.KeyOriginInfo(
                        xpub=util.parse_xpub(
                            "xpub6Eq64jDihkRvLg91wnckeTFWDT5jzdoKwX24aL9MHY4pS49E9jH69zFRnHuJzZijQaLZs7t5jtUxUhywhXGtUzsCf5EjunnDUNhzJFqhowa"
                        ),
                    ),
                ],
            )
        )

        is_registered = self._device.btc_is_script_config_registered(
            coin,
            policy_config,
            [],
        )
        if is_registered:
            print("Policy account already registered on the device.")
        else:
            policy_name = input("Enter a name for the policy account: ").strip()
            self._device.btc_register_script_config(
                coin=coin,
                script_config=policy_config,
                keypath=[],
                name=policy_name,
            )

        return policy_config

    def _btc_multisig_address(self) -> None:
        try:
            coin = bitbox02.btc.BTC
            print(
                self._device.btc_address(
                    coin=coin,
                    keypath=[
                        48 + HARDENED,
                        0 + HARDENED,
                        0 + HARDENED,
                        2 + HARDENED,
                        1,
                        2,
                    ],
                    script_config=self._btc_multisig_config(coin),
                    display=True,
                )
            )
        except UserAbortException:
            print("Aborted by user")

    def _btc_policy_address(self) -> None:
        try:
            coin = bitbox02.btc.TBTC
            print(
                self._device.btc_address(
                    coin=coin,
                    keypath=[
                        48 + HARDENED,
                        1 + HARDENED,
                        0 + HARDENED,
                        3 + HARDENED,
                        1,
                        2,
                    ],
                    script_config=self._btc_policy_config(coin),
                    display=True,
                )
            )
        except UserAbortException:
            print("Aborted by user")

    def _sign_btc_normal(
        self,
        format_unit: "bitbox02.btc.BTCSignInitRequest.FormatUnit.V" = bitbox02.btc.BTCSignInitRequest.FormatUnit.DEFAULT,
    ) -> None:
        # pylint: disable=no-member
        bip44_account: int = 0 + HARDENED
        inputs, outputs = _btc_demo_inputs_outputs(self._device, bip44_account)
        sigs = self._device.btc_sign(
            bitbox02.btc.BTC,
            [
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                    ),
                    keypath=[84 + HARDENED, 0 + HARDENED, bip44_account],
                ),
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
                    ),
                    keypath=[49 + HARDENED, 0 + HARDENED, bip44_account],
                ),
            ],
            inputs=inputs,
            outputs=outputs,
            format_unit=format_unit,
        )
        for input_index, sig in sigs:
            print("Signature for input {}: {}".format(input_index, sig.hex()))

    def _sign_btc_send_to_self_same_account(
        self,
        format_unit: "bitbox02.btc.BTCSignInitRequest.FormatUnit.V" = bitbox02.btc.BTCSignInitRequest.FormatUnit.DEFAULT,
    ) -> None:
        # pylint: disable=no-member
        bip44_account: int = 0 + HARDENED
        inputs, outputs = _btc_demo_inputs_outputs(self._device, bip44_account)
        outputs[1] = bitbox02.BTCOutputInternal(
            keypath=[84 + HARDENED, 0 + HARDENED, bip44_account, 0, 0],
            value=int(1e8 * 0.2),
            script_config_index=0,
        )
        sigs = self._device.btc_sign(
            bitbox02.btc.BTC,
            [
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                    ),
                    keypath=[84 + HARDENED, 0 + HARDENED, bip44_account],
                ),
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
                    ),
                    keypath=[49 + HARDENED, 0 + HARDENED, bip44_account],
                ),
            ],
            inputs=inputs,
            outputs=outputs,
            format_unit=format_unit,
        )
        for input_index, sig in sigs:
            print("Signature for input {}: {}".format(input_index, sig.hex()))

    def _sign_btc_send_to_self_different_account(
        self,
        format_unit: "bitbox02.btc.BTCSignInitRequest.FormatUnit.V" = bitbox02.btc.BTCSignInitRequest.FormatUnit.DEFAULT,
    ) -> None:
        # pylint: disable=no-member
        bip44_account: int = 0 + HARDENED
        inputs, outputs = _btc_demo_inputs_outputs(self._device, bip44_account)
        outputs[1] = bitbox02.BTCOutputInternal(
            keypath=[84 + HARDENED, 0 + HARDENED, 1 + HARDENED, 0, 0],
            value=int(1e8 * 0.2),
            script_config_index=0,
            output_script_config_index=0,
        )
        sigs = self._device.btc_sign(
            bitbox02.btc.BTC,
            [
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                    ),
                    keypath=[84 + HARDENED, 0 + HARDENED, bip44_account],
                ),
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
                    ),
                    keypath=[49 + HARDENED, 0 + HARDENED, bip44_account],
                ),
            ],
            inputs=inputs,
            outputs=outputs,
            format_unit=format_unit,
            output_script_configs=[
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                    ),
                    keypath=[84 + HARDENED, 0 + HARDENED, 1 + HARDENED],
                ),
            ],
        )
        for input_index, sig in sigs:
            print("Signature for input {}: {}".format(input_index, sig.hex()))

    def _sign_btc_high_fee(self) -> None:
        # pylint: disable=no-member
        bip44_account: int = 0 + HARDENED
        inputs, outputs = _btc_demo_inputs_outputs(self._device, bip44_account)
        outputs[1].value = int(1e8 * 0.18)
        sigs = self._device.btc_sign(
            bitbox02.btc.BTC,
            [
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                    ),
                    keypath=[84 + HARDENED, 0 + HARDENED, bip44_account],
                ),
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
                    ),
                    keypath=[49 + HARDENED, 0 + HARDENED, bip44_account],
                ),
            ],
            inputs=inputs,
            outputs=outputs,
        )
        for input_index, sig in sigs:
            print("Signature for input {}: {}".format(input_index, sig.hex()))

    def _sign_btc_multiple_changes(self) -> None:
        # pylint: disable=no-member
        bip44_account: int = 0 + HARDENED
        inputs, outputs = _btc_demo_inputs_outputs(self._device, bip44_account)
        # Add a change output.
        outputs.append(
            bitbox02.BTCOutputInternal(
                keypath=[84 + HARDENED, 0 + HARDENED, bip44_account, 1, 0],
                value=int(1),
                script_config_index=0,
            )
        )
        sigs = self._device.btc_sign(
            bitbox02.btc.BTC,
            [
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                    ),
                    keypath=[84 + HARDENED, 0 + HARDENED, bip44_account],
                ),
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
                    ),
                    keypath=[49 + HARDENED, 0 + HARDENED, bip44_account],
                ),
            ],
            inputs=inputs,
            outputs=outputs,
        )
        for input_index, sig in sigs:
            print("Signature for input {}: {}".format(input_index, sig.hex()))

    def _sign_btc_locktime(self, locktime: int) -> None:
        # pylint: disable=no-member
        bip44_account: int = 0 + HARDENED
        inputs, outputs = _btc_demo_inputs_outputs(self._device, bip44_account)
        inputs[0]["sequence"] = 0xFFFFFFFF - 1
        sigs = self._device.btc_sign(
            bitbox02.btc.BTC,
            [
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                    ),
                    keypath=[84 + HARDENED, 0 + HARDENED, bip44_account],
                ),
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
                    ),
                    keypath=[49 + HARDENED, 0 + HARDENED, bip44_account],
                ),
            ],
            inputs=inputs,
            outputs=outputs,
            locktime=locktime,
        )
        for input_index, sig in sigs:
            print("Signature for input {}: {}".format(input_index, sig.hex()))

    def _sign_btc_height_based_locktime(self) -> None:
        self._sign_btc_locktime(963_832)

    def _sign_btc_time_based_locktime(self) -> None:
        self._sign_btc_locktime(1_800_000_000)

    def _sign_btc_taproot_inputs(self) -> None:
        # pylint: disable=no-member
        bip44_account: int = 0 + HARDENED
        inputs, outputs = _btc_demo_inputs_outputs(self._device, bip44_account)
        for inp in inputs:
            inp["keypath"] = [86 + HARDENED] + list(inp["keypath"][1:])
            inp["prev_tx"] = None
            inp["script_config_index"] = 0
        for outp in outputs:
            if isinstance(outp, bitbox02.BTCOutputInternal):
                outp.keypath = [86 + HARDENED] + list(outp.keypath[1:])
        script_configs = [
            bitbox02.btc.BTCScriptConfigWithKeypath(
                script_config=bitbox02.btc.BTCScriptConfig(
                    simple_type=bitbox02.btc.BTCScriptConfig.P2TR
                ),
                keypath=[86 + HARDENED, 0 + HARDENED, bip44_account],
            ),
        ]
        assert not bitbox02.btc_sign_needs_prevtxs(script_configs)
        sigs = self._device.btc_sign(
            bitbox02.btc.BTC,
            script_configs,
            inputs=inputs,
            outputs=outputs,
        )
        for input_index, sig in sigs:
            print("Signature for input {}: {}".format(input_index, sig.hex()))

    def _sign_btc_taproot_output(self) -> None:
        # pylint: disable=no-member
        bip44_account: int = 0 + HARDENED
        inputs, outputs = _btc_demo_inputs_outputs(self._device, bip44_account)
        assert isinstance(outputs[1], bitbox02.BTCOutputExternal)
        outputs[1].type = bitbox02.btc.P2TR
        outputs[1].payload = bytes.fromhex(
            "a60869f0dbcf1dc659c9cecbaf8050135ea9e8cdc487053f1dc6880949dc684c"
        )
        sigs = self._device.btc_sign(
            bitbox02.btc.BTC,
            [
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                    ),
                    keypath=[84 + HARDENED, 0 + HARDENED, bip44_account],
                ),
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
                    ),
                    keypath=[49 + HARDENED, 0 + HARDENED, bip44_account],
                ),
            ],
            inputs=inputs,
            outputs=outputs,
        )
        for input_index, sig in sigs:
            print("Signature for input {}: {}".format(input_index, sig.hex()))

    def _sign_btc_policy(self) -> None:
        bip44_account: int = 0 + HARDENED
        account_keypath = [48 + HARDENED, 1 + HARDENED, bip44_account, 3 + HARDENED]
        coin = bitbox02.btc.TBTC
        script_configs = [
            bitbox02.btc.BTCScriptConfigWithKeypath(
                script_config=self._btc_policy_config(coin),
                keypath=account_keypath,
            ),
        ]
        inputs, outputs = _btc_demo_inputs_outputs(
            self._device,
            bip44_account,
            coin=coin,
            script_configs=script_configs,
        )
        assert isinstance(outputs[0], bitbox02.BTCOutputInternal)
        outputs[0].keypath = account_keypath + [1, 0]

        sigs = self._device.btc_sign(
            coin,
            script_configs,
            inputs=inputs,
            outputs=outputs,
        )
        for input_index, sig in sigs:
            print("Signature for input {}: {}".format(input_index, sig.hex()))

    def _sign_btc_op_return(
        self,
        format_unit: "bitbox02.btc.BTCSignInitRequest.FormatUnit.V" = bitbox02.btc.BTCSignInitRequest.FormatUnit.DEFAULT,
    ) -> None:
        # pylint: disable=no-member
        bip44_account: int = 0 + HARDENED
        inputs, outputs = _btc_demo_inputs_outputs(self._device, bip44_account)
        outputs.append(
            bitbox02.BTCOutputExternal(
                output_type=bitbox02.btc.OP_RETURN,
                output_payload=b"hello world",
                value=0,
            )
        )
        sigs = self._device.btc_sign(
            bitbox02.btc.BTC,
            [
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                    ),
                    keypath=[84 + HARDENED, 0 + HARDENED, bip44_account],
                ),
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
                    ),
                    keypath=[49 + HARDENED, 0 + HARDENED, bip44_account],
                ),
            ],
            inputs=inputs,
            outputs=outputs,
            format_unit=format_unit,
        )
        for input_index, sig in sigs:
            print("Signature for input {}: {}".format(input_index, sig.hex()))

    def _sign_btc_tx_from_raw(self) -> None:
        """
        Experiment with testnet transactions.
        Uses blockchair.com to convert a testnet transaction to the input required by btc_sign(),
        including the previous transactions.
        """
        # pylint: disable=no-member

        def get(tx_id: str) -> Any:
            return requests.get(
                "https://api.blockchair.com/bitcoin/testnet/dashboards/transaction/{}".format(
                    tx_id
                ),
                timeout=30,
            ).json()["data"][tx_id]

        tx_id = input("Paste a btc testnet tx ID: ").strip()
        tx = get(tx_id)

        inputs: List[bitbox02.BTCInputType] = []
        outputs: List[bitbox02.BTCOutputType] = []

        bip44_account: int = 0 + HARDENED

        for inp in tx["inputs"]:
            print("Downloading prev tx")
            prev_tx = get(inp["transaction_hash"])
            print("Downloaded prev tx")
            prev_inputs: List[bitbox02.BTCPrevTxInputType] = []
            prev_outputs: List[bitbox02.BTCPrevTxOutputType] = []

            for prev_inp in prev_tx["inputs"]:
                prev_inputs.append(
                    {
                        "prev_out_hash": binascii.unhexlify(prev_inp["transaction_hash"])[::-1],
                        "prev_out_index": prev_inp["index"],
                        "signature_script": binascii.unhexlify(prev_inp["spending_signature_hex"]),
                        "sequence": prev_inp["spending_sequence"],
                    }
                )
            for prev_outp in prev_tx["outputs"]:
                prev_outputs.append(
                    {
                        "value": prev_outp["value"],
                        "pubkey_script": binascii.unhexlify(prev_outp["script_hex"]),
                    }
                )

            inputs.append(
                {
                    "prev_out_hash": binascii.unhexlify(inp["transaction_hash"])[::-1],
                    "prev_out_index": inp["index"],
                    "prev_out_value": inp["value"],
                    "sequence": inp["spending_sequence"],
                    "keypath": [84 + HARDENED, 1 + HARDENED, bip44_account, 0, 0],
                    "script_config_index": 0,
                    "prev_tx": {
                        "version": prev_tx["transaction"]["version"],
                        "locktime": prev_tx["transaction"]["lock_time"],
                        "inputs": prev_inputs,
                        "outputs": prev_outputs,
                    },
                }
            )

        for outp in tx["outputs"]:
            outputs.append(
                bitbox02.BTCOutputExternal(
                    # TODO: parse pubkey script
                    output_type=bitbox02.btc.P2WSH,
                    output_payload=b"11111111111111111111111111111111",
                    value=outp["value"],
                )
            )

        print("Start signing...")
        self._device.btc_sign(
            bitbox02.btc.TBTC,
            [
                bitbox02.btc.BTCScriptConfigWithKeypath(
                    script_config=bitbox02.btc.BTCScriptConfig(
                        simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
                    ),
                    keypath=[84 + HARDENED, 1 + HARDENED, bip44_account],
                )
            ],
            inputs=inputs,
            outputs=outputs,
        )

    def _sign_btc_tx(self) -> None:
        """btc signing demos"""
        choices = (
            ("Normal tx", self._sign_btc_normal),
            (
                "Normal tx, formatted in sats",
                lambda: self._sign_btc_normal(
                    format_unit=bitbox02.btc.BTCSignInitRequest.FormatUnit.SAT
                ),
            ),
            ("Send to self (same account)", self._sign_btc_send_to_self_same_account),
            ("Send to self (different account)", self._sign_btc_send_to_self_different_account),
            ("High fee warning", self._sign_btc_high_fee),
            ("Multiple change outputs", self._sign_btc_multiple_changes),
            ("Height-based locktime", self._sign_btc_height_based_locktime),
            ("Time-based locktime", self._sign_btc_time_based_locktime),
            ("Taproot inputs", self._sign_btc_taproot_inputs),
            ("Taproot output", self._sign_btc_taproot_output),
            ("Policy", self._sign_btc_policy),
            ("OP_RETURN", self._sign_btc_op_return),
            ("From testnet tx ID", self._sign_btc_tx_from_raw),
        )
        choice = ask_user(choices)
        if callable(choice):
            try:
                choice()
            except UserAbortException:
                eprint("Aborted by user")

    def _sign_btc_message(self) -> None:
        # pylint: disable=no-member

        def sign(
            coin: "bitbox02.btc.BTCCoin.V",
            keypath: Sequence[int],
            script_config: bitbox02.btc.BTCScriptConfig,
            print_address: bool = True,
        ) -> None:
            if print_address:
                address = self._device.btc_address(
                    coin=coin, keypath=keypath, script_config=script_config, display=False
                )

                print("Address:", address)

            msg = input(r"Message to sign (\n = newline): ")
            if msg.startswith("0x"):
                msg_bytes = binascii.unhexlify(msg[2:])
            else:
                msg_bytes = msg.replace(r"\n", "\n").encode("utf-8")

            try:
                _, _, sig65 = self._device.btc_sign_msg(
                    coin,
                    bitbox02.btc.BTCScriptConfigWithKeypath(
                        script_config=script_config, keypath=keypath
                    ),
                    msg_bytes,
                )
                print("Signature:", base64.b64encode(sig65).decode("ascii"))
            except UserAbortException:
                print("Aborted by user")

        def sign_mainnet() -> None:
            keypath = [49 + HARDENED, 0 + HARDENED, 0 + HARDENED, 0, 0]
            script_config = bitbox02.btc.BTCScriptConfig(
                simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
            )
            sign(bitbox02.btc.BTC, keypath, script_config)

        def sign_testnet() -> None:
            keypath = [49 + HARDENED, 1 + HARDENED, 0 + HARDENED, 0, 0]
            script_config = bitbox02.btc.BTCScriptConfig(
                simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH_P2SH
            )
            sign(bitbox02.btc.TBTC, keypath, script_config)

        def sign_external_service(purpose: int) -> None:
            keypath = [purpose + HARDENED, 0 + HARDENED, 0 + HARDENED, 0, 0]
            script_config = bitbox02.btc.BTCScriptConfig(
                simple_type=bitbox02.btc.BTCScriptConfig.P2WPKH
            )
            # The address endpoint only accepts standard keypaths. The sign-message workflow
            # derives and displays the address itself.
            sign(bitbox02.btc.BTC, keypath, script_config, print_address=False)

        choices = (
            ("Mainnet", sign_mainnet),
            ("Testnet", sign_testnet),
            ("External service (m/45')", lambda: sign_external_service(45)),
            ("External service (m/48')", lambda: sign_external_service(48)),
        )
        choice = ask_user(choices)
        if callable(choice):
            try:
                choice()
            except UserAbortException:
                eprint("Aborted by user")

    def _check_backup(self) -> None:
        print("Your BitBox02 will now perform a backup check")
        try:
            backup_id = self._device.check_backup()
        except UserAbortException:
            print("Aborted by user")
        else:
            if backup_id:
                print(f"Check successful. Backup with ID {backup_id} matches")
            else:
                print("No matching backup found")

    def _show_mnemnoic_seed(self) -> None:
        print("Your BitBox02 will now show the mnemonic seed phrase")
        try:
            self._device.show_mnemonic()
            print("Success")
        except UserAbortException:
            print("Aborted by user")

    def _create_backup(self) -> None:
        if self._device.check_backup(silent=True) is not None:
            if input("A backup already exists, continue? Y/n: ") not in ("", "Y", "y"):
                return
        try:
            if not self._device.create_backup():
                eprint("Creating the backup failed")
            else:
                print("Backup created sucessfully")
        except UserAbortException:
            print("Aborted by user")

    def _toggle_mnemonic_passphrase(self) -> None:
        enabled = self._device.device_info()["mnemonic_passphrase_enabled"]
        try:
            if enabled:
                if input("Mnemonic passprase enabled, disable? Y/n: ") not in (
                    "",
                    "Y",
                    "y",
                ):
                    return
                self._device.disable_mnemonic_passphrase()
            else:
                if input("Mnemonic passprase disabled, enable? Y/n: ") not in (
                    "",
                    "Y",
                    "y",
                ):
                    return
                self._device.enable_mnemonic_passphrase()
            enabled = not enabled
        except UserAbortException:
            print("Aborted by user")
        print("Success.")
        if enabled:
            print("You can enter a mnemonic passphrase on the next unlock.")
            print("Replug your BitBox02.")

    def _get_eth_xpub(self) -> None:
        try:
            xpub = self._device.eth_pub(
                keypath=[44 + HARDENED, 60 + HARDENED, 0 + HARDENED, 0],
                output_type=bitbox02.eth.ETHPubRequest.XPUB,  # pylint: disable=no-member
                display=False,
            )
        except UserAbortException:
            eprint("Aborted by user")

        print("Ethereum xpub: {}".format(xpub))

    def _display_eth_address(self, contract_address: bytes = b"") -> None:
        def address(display: bool = False) -> str:
            return self._device.eth_pub(
                keypath=[44 + HARDENED, 60 + HARDENED, 0 + HARDENED, 0, 0],
                output_type=bitbox02.eth.ETHPubRequest.ADDRESS,  # pylint: disable=no-member
                contract_address=contract_address,
                display=display,
            )

        print("Ethereum address: {}".format(address(display=False)))
        try:
            address(display=True)
        except UserAbortException:
            eprint("Aborted by user")

    def _sign_eth_tx(self) -> None:
        # pylint: disable=line-too-long,too-many-branches,too-many-statements

        inp = input(
            "Select one of: 1=normal; 2=erc20; 3=erc721; 4=unknown erc20; 5=large data field; 6=BSC; 7=unknown network; 8=eip1559; 9=Arbitrum; 10=streaming (10KB data); 11=long amounts: "
        ).strip()

        chain_id = 1  # mainnet
        if inp == "6":
            chain_id = 56
        elif inp == "7":
            chain_id = 123456
        elif inp == "9":
            chain_id = 42161  # Arbitrum One

        if inp in ("1", "6", "7", "9"):
            # fmt: off
            tx = bytes([0xf8, 0x6e, 0x82, 0x1f, 0xdc, 0x85, 0x01, 0x65, 0xa0, 0xbc, 0x00, 0x82, 0x52,
            0x08, 0x94, 0x04, 0xf2, 0x64, 0xcf, 0x34, 0x44, 0x03, 0x13, 0xb4, 0xa0, 0x19, 0x2a,
            0x35, 0x28, 0x14, 0xfb, 0xe9, 0x27, 0xb8, 0x85, 0x88, 0x07, 0x5c, 0xf1, 0x25, 0x9e,
            0x9c, 0x40, 0x00, 0x80, 0x25, 0xa0, 0x15, 0xc9, 0x4c, 0x1a, 0x3d, 0xa0, 0xab, 0xc0,
            0xa9, 0x12, 0x4d, 0x28, 0x37, 0x80, 0x9c, 0xcc, 0x49, 0x3c, 0x41, 0x50, 0x4e, 0x45,
            0x71, 0xbc, 0xc3, 0x40, 0xee, 0xb6, 0x8a, 0x91, 0xf6, 0x41, 0xa0, 0x35, 0x99, 0x01,
            0x1d, 0x4c, 0xda, 0x2c, 0x33, 0xdd, 0x3b, 0x00, 0x07, 0x1e, 0xc1, 0x45, 0x33, 0x5e,
            0x5d, 0x2d, 0xd5, 0xed, 0x81, 0x2d, 0x5e, 0xeb, 0xee, 0xcb, 0xa5, 0x26, 0x4e, 0xd1,
            0xbf])
            # fmt: on
        elif inp == "2":
            tx = binascii.unhexlify(
                "f8ac82236785027aca1a808301d04894dac17f958d2ee523a2206206994597c13d831ec780b844a9059cbb000000000000000000000000e6ce0a092a99700cd4ccccbb1fedc39cf53e6330000000000000000000000000000000000000000000000000000000000365c0401ca0265f70103c605eaa1b64c3200d2e7934d7744a3068b377e26c4b080795c744c0a020bbcd34a306621fa8965040390bc240d6d0e3b88915ccdb309d15f1caba81b1"
            )
        elif inp == "3":
            tx = binascii.unhexlify(
                "f87282750b8502cb42fea0830927c0942cab2d282e588f00beabe2bf5577c7644972e10f808b00009470ff1c8de91c861d1ca0f07bca4f43eb1c461ca3cf208e920b60cb393dd37489a14e9f92632acb17dd7ca0694523c8b72052cf6a8b9f93719a664fbbbe59e731cefb1331b2d21ee80b5268"
            )
        elif inp == "4":
            tx = binascii.unhexlify(
                "f8aa81b9843b9aca0083010985949c23d67aea7b95d80942e3836bcdf7e708a747c180b844a9059cbb000000000000000000000000857b3d969eacb775a9f79cabc62ec4bb1d1cd60e000000000000000000000000000000000000000000000098a63cbeb859d027b026a0d3b1a9ba4aff7ebf81dca7dafdbe6d803d174f7805276f45530f2c30e74f5ffca02d86d5290f6ba2c5100e08764d8ab34cf33b03dff3f63219fd839ac9a95f7068"
            )
        elif inp == "5":
            tx = binascii.unhexlify(
                "f9016881b9843b9aca0083010985949c23d67aea7b95d80942e3836bcdf7e708a747c180b90141ef3f3d0b000000000000000000000000eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee0000000000000000000000006b175474e89094c44da98b954eedeac495271d0f0000000000000000000000009be6769ef4fc4ccda30e0e39052070d90d3f0bfe000000000000000000000000000000000000000000000000000000e8d4a51000000000000000000000000000000000000000000000000000000000000000016000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000001e0000000000000000000000000000000000000000000000000000000000000026000000000000000000000000000000000000000000000000000000000000002c0000000000000000000000000000000000000000000000000000179fc8e808080"
            )
        elif inp == "8":
            tx = binascii.unhexlify(
                "02f0010184773594008502540be40082520894d61054f4456d0555dc2dd82b77f7ad6074836149865af3107a400080808080"
            )
        elif inp == "10":
            nonce = b"\x01"
            gas_price = b"\x04\xa8\x17\xc8\x00"  # 20 gwei
            gas_limit = b"\x0f\x42\x40"  # 1,000,000
            recipient = (
                b"\x11\x22\x33\x44\x55\x66\x77\x88\x99\xaa\xbb\xcc\xdd\xee\xff\x00\x11\x22\x33\x44"
            )
            value = b""  # Empty for zero value (no leading zeros allowed)
            data = bytes([i % 256 for i in range(10000)])
            v = b"\x25"  # chain_id=1
            r = b"\x01" * 32
            s = b"\x01" * 32
            tx = rlp.encode([nonce, gas_price, gas_limit, recipient, value, data, v, r, s])
            if self._debug:
                print(f"Streaming test transaction: {len(data)} bytes of data")
        elif inp == "11":
            # Exercise overflow handling for the send amount, total, and fee screens.
            nonce = b"\x01"
            gas_price = b"\xff" * 8
            gas_limit = b"\xff" * 8
            recipient = (
                b"\x11\x22\x33\x44\x55\x66\x77\x88\x99\xaa\xbb\xcc\xdd\xee\xff\x00\x11\x22\x33\x44"
            )
            value = b"\xff" * 32
            data = b""
            v = b"\x25"  # chain_id=1
            r = b"\x01" * 32
            s = b"\x01" * 32
            tx = rlp.encode([nonce, gas_price, gas_limit, recipient, value, data, v, r, s])
        else:
            print("None selected")
            return

        try:
            sig = self._device.eth_sign(
                tx,
                keypath=[44 + HARDENED, 60 + HARDENED, 0 + HARDENED, 0, 0],
                address_case=bitbox02.eth.ETHAddressCase.ETH_ADDRESS_CASE_MIXED,
                chain_id=chain_id,
            )
            print("Signature: {}".format(sig.hex()))
        except UserAbortException:
            eprint("Aborted by user")

    def _sign_eth_message(self) -> None:
        msg = input(r"Message to sign (\n = newline): ")
        if msg.startswith("0x"):
            msg_bytes = binascii.unhexlify(msg[2:])
        else:
            msg_bytes = msg.replace(r"\n", "\n").encode("utf-8")
        msg_hex = binascii.hexlify(msg_bytes).decode("utf-8")
        print(f"signing\nbytes: {repr(msg_bytes)}\nhex: 0x{msg_hex}")
        sig = self._device.eth_sign_msg(
            msg=msg_bytes,
            keypath=[44 + HARDENED, 60 + HARDENED, 0 + HARDENED, 0, 0],
        )

        print("Signature: 0x{}".format(binascii.hexlify(sig).decode("utf-8")))

    def _sign_eth_typed_message(self) -> None:
        msg = """{
    "types": {
        "EIP712Domain": [
            { "name": "name", "type": "string" },
            { "name": "version", "type": "string" },
            { "name": "chainId", "type": "uint256" },
            { "name": "verifyingContract", "type": "address" }
        ],
        "Attachment": [
            { "name": "contents", "type": "string" }
        ],
        "Person": [
            { "name": "name", "type": "string" },
            { "name": "wallet", "type": "address" },
            { "name": "age", "type": "uint8" }
        ],
        "Mail": [
            { "name": "from", "type": "Person" },
            { "name": "to", "type": "Person" },
            { "name": "contents", "type": "string" },
            { "name": "attachments", "type": "Attachment[]" }
        ]
    },
    "primaryType": "Mail",
    "domain": {
        "name": "Ether Mail",
        "version": "1",
        "chainId": 1,
        "verifyingContract": "0xCcCCccccCCCCcCCCCCCcCcCccCcCCCcCcccccccC"
    },
    "message": {
        "from": {
            "name": "Cow",
            "wallet": "0xCD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826",
            "age": 20
        },
        "to": {
            "name": "Bob",
            "wallet": "0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB",
            "age": "0x1e"
        },
        "contents": "Hello, Bob!",
        "attachments": [{ "contents": "attachment1" }, { "contents": "attachment2" }]
    }
}"""
        print("Signing:\n{}".format(msg))
        sig = self._device.eth_sign_typed_msg(
            keypath=[44 + HARDENED, 60 + HARDENED, 0 + HARDENED, 0, 0], msg=json.loads(msg)
        )

        print("Signature: 0x{}".format(binascii.hexlify(sig).decode("utf-8")))

    def _sign_eth_typed_message_large_data(self) -> None:
        large_data = "0x" + os.urandom(50000).hex()
        msg = {
            "types": {
                "EIP712Domain": [
                    {"name": "chainId", "type": "uint256"},
                    {"name": "verifyingContract", "type": "address"},
                ],
                "SafeTx": [
                    {"name": "to", "type": "address"},
                    {"name": "value", "type": "uint256"},
                    {"name": "data", "type": "bytes"},
                    {"name": "operation", "type": "uint8"},
                    {"name": "safeTxGas", "type": "uint256"},
                    {"name": "baseGas", "type": "uint256"},
                    {"name": "gasPrice", "type": "uint256"},
                    {"name": "gasToken", "type": "address"},
                    {"name": "refundReceiver", "type": "address"},
                    {"name": "nonce", "type": "uint256"},
                ],
            },
            "primaryType": "SafeTx",
            "domain": {
                "chainId": "1",
                "verifyingContract": "0x0000000000000000000000000000000000000000",
            },
            "message": {
                "to": "0x0000000000000000000000000000000000000000",
                "value": "0",
                "data": large_data,
                "operation": "0",
                "safeTxGas": "0",
                "baseGas": "0",
                "gasPrice": "0",
                "gasToken": "0x0000000000000000000000000000000000000000",
                "refundReceiver": "0x0000000000000000000000000000000000000000",
                "nonce": "3",
            },
        }
        print(f"Signing SafeTx with {len(large_data)//2 - 1} bytes of data (streaming)")
        sig = self._device.eth_sign_typed_msg(
            keypath=[44 + HARDENED, 60 + HARDENED, 0 + HARDENED, 0, 0],
            msg=msg,
        )
        print("Signature: 0x{}".format(binascii.hexlify(sig).decode("utf-8")))

    @staticmethod
    def _run_altcoin_menu(
        choices: Sequence[Tuple[str, Callable[[], None]]],
    ) -> None:
        choice = ask_user(choices)
        if callable(choice):
            try:
                choice()
            except UserAbortException:
                eprint("Aborted by user")
            except (argparse.ArgumentTypeError, OSError, ValueError) as exc:
                eprint(f"Invalid input: {exc}")

    def _solana(self) -> None:
        # pylint: disable=no-member
        keypath = parse_keypath("m/44'/501'/0'")

        def address() -> None:
            _command_solana_address(
                self._device,
                argparse.Namespace(keypath=keypath, display=True),
            )

        def sign(build_message: Callable[[bytes], bytes]) -> None:
            signer_address = self._device.solana_address(keypath, display=False)
            _command_solana_sign(
                self._device,
                argparse.Namespace(
                    network="mainnet",
                    keypath=keypath,
                    message_hex=build_message(base58.b58decode(signer_address)),
                ),
            )

        self._run_altcoin_menu(
            (
                ("Retrieve address", address),
                ("Sign demo SOL transfer", lambda: sign(_solana_demo_sol_message)),
                ("Sign demo SPL transfer", lambda: sign(_solana_demo_spl_message)),
            )
        )

    def _xrp(self) -> None:
        keypath = parse_keypath("m/44'/144'/0'/0/0")

        def address() -> None:
            _command_xrp_address(
                self._device,
                argparse.Namespace(keypath=keypath, display=True),
            )

        def sign() -> None:
            _command_xrp_sign(
                self._device,
                argparse.Namespace(
                    network="mainnet",
                    keypath=keypath,
                    destination="r9cZA1mLK5R5Am25ArfXFmqgNwjZgnfk59",
                    amount=1_000_000,
                    fee=12,
                    sequence=1,
                    destination_tag=42,
                    last_ledger_sequence=100,
                    memo="BitBox demo",
                    memo_hex=None,
                ),
            )

        self._run_altcoin_menu(
            (
                ("Retrieve address", address),
                ("Sign demo payment", sign),
            )
        )

    def _tron(self) -> None:
        keypath = parse_keypath("m/44'/195'/0'/0/0")

        def address() -> None:
            _command_tron_address(
                self._device,
                argparse.Namespace(keypath=keypath, display=True),
            )

        def sign(build_raw_data: Callable[[bytes], bytes]) -> None:
            owner_address = self._device.tron_address(keypath, display=False)
            _command_tron_sign(
                self._device,
                argparse.Namespace(
                    network="mainnet",
                    keypath=keypath,
                    raw_data_hex=build_raw_data(base58.b58decode_check(owner_address)),
                ),
            )

        self._run_altcoin_menu(
            (
                ("Retrieve address", address),
                ("Sign demo TRX transfer", lambda: sign(_tron_demo_trx_raw_data)),
                ("Sign demo TRC-20 transfer", lambda: sign(_tron_demo_trc20_raw_data)),
            )
        )

    def _zcash(self) -> None:
        # pylint: disable=no-member
        network = bitbox02.zcash.ZCASH_MAINNET
        keypath = parse_keypath("m/44'/133'/0'/0/0")

        def address() -> None:
            _command_zcash_address(
                self._device,
                argparse.Namespace(network="mainnet", keypath=keypath, display=True),
            )

        def sign() -> None:
            source_address = self._device.zcash_address(network, keypath, display=False)
            source_script = _zcash_p2pkh_script(source_address)
            destination_script = b"\x76\xa9\x14" + bytes([0x22]) * 20 + b"\x88\xac"
            response = self._device.zcash_sign_transaction(
                bitbox02.zcash.ZcashSignTransactionRequest(
                    network=network,
                    inputs=[
                        bitbox02.zcash.ZcashSignTransactionRequest.Input(
                            keypath=keypath,
                            prev_out_hash=bytes(range(32)),
                            prev_out_index=0,
                            value=100_000,
                            script_pubkey=source_script,
                            sequence=0xFFFF_FFFE,
                        )
                    ],
                    outputs=[
                        bitbox02.zcash.ZcashSignTransactionRequest.Output(
                            value=99_000,
                            script_pubkey=destination_script,
                        )
                    ],
                    lock_time=0,
                    expiry_height=1_234_567,
                    consensus_branch_id=0xC8E7_1055,
                ),
            )
            for index, signature in enumerate(response.signatures):
                print(f"signature[{index}]: {signature.hex()}")
            print(f"serialized_transaction: {response.serialized_transaction.hex()}")

        self._run_altcoin_menu(
            (
                ("Retrieve transparent address", address),
                ("Sign demo transparent v5 transaction", sign),
            )
        )

    def _cardano(self) -> None:
        def xpubs() -> None:
            xpubs = self._device.cardano_xpubs(
                keypaths=[
                    [1852 + HARDENED, 1815 + HARDENED, HARDENED],
                    [1852 + HARDENED, 1815 + HARDENED, HARDENED + 1],
                ]
            )
            print("m/1852'/1815'/0' xpub: ", xpubs[0].hex())
            print("m/1852'/1815'/1' xpub: ", xpubs[1].hex())

        script_config = bitbox02.cardano.CardanoScriptConfig(
            pkh_skh=bitbox02.cardano.CardanoScriptConfig.PkhSkh(
                keypath_payment=[1852 + HARDENED, 1815 + HARDENED, HARDENED, 0, 0],
                keypath_stake=[1852 + HARDENED, 1815 + HARDENED, HARDENED, 2, 0],
            )
        )

        def get_address(display: bool) -> str:
            return self._device.cardano_address(
                bitbox02.cardano.CardanoAddressRequest(
                    network=bitbox02.cardano.CardanoMainnet,
                    display=display,
                    script_config=script_config,
                )
            )

        def address() -> None:
            print("m/1852'/1815'/0'/0/0 address: ", get_address(False))
            get_address(True)

        def sign() -> None:
            response = self._device.cardano_sign_transaction(
                transaction=bitbox02.cardano.CardanoSignTransactionRequest(
                    network=bitbox02.cardano.CardanoMainnet,
                    inputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Input(
                            keypath=[2147485500, 2147485463, 2147483648, 0, 0],
                            prev_out_hash=bytes.fromhex(
                                "59864ee73ca5d91098a32b3ce9811bac1996dcbaefa6b6247dcaafb5779c2538"
                            ),
                            prev_out_index=0,
                        )
                    ],
                    outputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Output(
                            encoded_address="addr1q9qfllpxg2vu4lq6rnpel4pvpp5xnv3kvvgtxk6k6wp4ff89xrhu8jnu3p33vnctc9eklee5dtykzyag5penc6dcmakqsqqgpt",
                            value=1000000,
                        ),
                        bitbox02.cardano.CardanoSignTransactionRequest.Output(
                            encoded_address=get_address(False),
                            value=4829501,
                            script_config=script_config,
                        ),
                    ],
                    fee=170499,
                    ttl=41115811,
                    certificates=[],
                    validity_interval_start=41110811,
                )
            )
            print(response)

        def sign_zero_ttl() -> None:
            response = self._device.cardano_sign_transaction(
                transaction=bitbox02.cardano.CardanoSignTransactionRequest(
                    network=bitbox02.cardano.CardanoMainnet,
                    inputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Input(
                            keypath=[2147485500, 2147485463, 2147483648, 0, 0],
                            prev_out_hash=bytes.fromhex(
                                "59864ee73ca5d91098a32b3ce9811bac1996dcbaefa6b6247dcaafb5779c2538"
                            ),
                            prev_out_index=0,
                        )
                    ],
                    outputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Output(
                            encoded_address="addr1q9qfllpxg2vu4lq6rnpel4pvpp5xnv3kvvgtxk6k6wp4ff89xrhu8jnu3p33vnctc9eklee5dtykzyag5penc6dcmakqsqqgpt",
                            value=1000000,
                        ),
                        bitbox02.cardano.CardanoSignTransactionRequest.Output(
                            encoded_address=get_address(False),
                            value=4829501,
                            script_config=script_config,
                        ),
                    ],
                    fee=170499,
                    ttl=0,
                    allow_zero_ttl=True,
                    certificates=[],
                    validity_interval_start=41110811,
                )
            )
            print(response)

        def sign_tokens() -> None:
            response = self._device.cardano_sign_transaction(
                transaction=bitbox02.cardano.CardanoSignTransactionRequest(
                    network=bitbox02.cardano.CardanoMainnet,
                    inputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Input(
                            keypath=[2147485500, 2147485463, 2147483648, 0, 0],
                            prev_out_hash=bytes.fromhex(
                                "59864ee73ca5d91098a32b3ce9811bac1996dcbaefa6b6247dcaafb5779c2538"
                            ),
                            prev_out_index=0,
                        )
                    ],
                    outputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Output(
                            encoded_address="addr1q9qfllpxg2vu4lq6rnpel4pvpp5xnv3kvvgtxk6k6wp4ff89xrhu8jnu3p33vnctc9eklee5dtykzyag5penc6dcmakqsqqgpt",
                            value=1000000,
                            asset_groups=[
                                # Asset policy ids and asset names from: https://github.com/cardano-foundation/CIPs/blob/a2ef32d8a2b485fed7f6ffde2781dd58869ff511/CIP-0014/README.md#test-vectors
                                bitbox02.cardano.CardanoSignTransactionRequest.AssetGroup(
                                    policy_id=bytes.fromhex(
                                        "1e349c9bdea19fd6c147626a5260bc44b71635f398b67c59881df209"
                                    ),
                                    tokens=[
                                        # asset1hv4p5tv2a837mzqrst04d0dcptdjmluqvdx9k3
                                        bitbox02.cardano.CardanoSignTransactionRequest.AssetGroup.Token(
                                            asset_name=bytes.fromhex("504154415445"),
                                            value=1,
                                        ),
                                        # asset1aqrdypg669jgazruv5ah07nuyqe0wxjhe2el6f
                                        bitbox02.cardano.CardanoSignTransactionRequest.AssetGroup.Token(
                                            asset_name=bytes.fromhex(
                                                "7eae28af2208be856f7a119668ae52a49b73725e326dc16579dcc373"
                                            ),
                                            value=3,
                                        ),
                                    ],
                                ),
                            ],
                        ),
                        bitbox02.cardano.CardanoSignTransactionRequest.Output(
                            encoded_address=get_address(False),
                            value=4829501,
                            script_config=script_config,
                        ),
                    ],
                    fee=170499,
                    certificates=[],
                )
            )
            print(response)

        def delegate() -> None:
            response = self._device.cardano_sign_transaction(
                transaction=bitbox02.cardano.CardanoSignTransactionRequest(
                    network=bitbox02.cardano.CardanoMainnet,
                    inputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Input(
                            keypath=[2147485500, 2147485463, 2147483648, 0, 0],
                            prev_out_hash=bytes.fromhex(
                                "64c39d60f9d6b4f883d05ae3585d0621d0febc06ad0ea3403bdc00bc23671615"
                            ),
                            prev_out_index=1,
                        ),
                        bitbox02.cardano.CardanoSignTransactionRequest.Input(
                            keypath=[2147485500, 2147485463, 2147483648, 0, 0],
                            prev_out_hash=bytes.fromhex(
                                "b7b2333e72f2670ab82051f426cc84000431975a34e71d5edf70ea6c0ddc9bf8"
                            ),
                            prev_out_index=0,
                        ),
                    ],
                    outputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Output(
                            encoded_address=get_address(False),
                            value=2741512,
                            script_config=script_config,
                        )
                    ],
                    fee=191681,
                    ttl=41539125,
                    certificates=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Certificate(
                            stake_registration=bitbox02.common.Keypath(
                                keypath=[2147485500, 2147485463, 2147483648, 2, 0]
                            )
                        ),
                        bitbox02.cardano.CardanoSignTransactionRequest.Certificate(
                            stake_delegation=bitbox02.cardano.CardanoSignTransactionRequest.Certificate.StakeDelegation(
                                keypath=[2147485500, 2147485463, 2147483648, 2, 0],
                                pool_keyhash=bytes.fromhex(
                                    "abababababababababababababababababababababababababababab"
                                ),
                            )
                        ),
                    ],
                )
            )
            print(response)

        def delegate_vote() -> None:
            response = self._device.cardano_sign_transaction(
                transaction=bitbox02.cardano.CardanoSignTransactionRequest(
                    network=bitbox02.cardano.CardanoMainnet,
                    inputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Input(
                            keypath=[2147485500, 2147485463, 2147483648, 0, 0],
                            prev_out_hash=bytes.fromhex(
                                "b7b2333e72f2670ab82051f426cc84000431975a34e71d5edf70ea6c0ddc9bf8"
                            ),
                            prev_out_index=0,
                        ),
                    ],
                    outputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Output(
                            encoded_address=get_address(False),
                            value=2741512,
                            script_config=script_config,
                        )
                    ],
                    fee=191681,
                    ttl=41539125,
                    certificates=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Certificate(
                            vote_delegation=bitbox02.cardano.CardanoSignTransactionRequest.Certificate.VoteDelegation(
                                # keypath used here is the stake credential
                                keypath=[2147485500, 2147485463, 2147483648, 2, 0],
                                type=bitbox02.cardano.CardanoSignTransactionRequest.Certificate.VoteDelegation.CardanoDRepType.ALWAYS_ABSTAIN,
                            )
                        ),
                    ],
                )
            )
            print(response)

        def withdraw() -> None:
            response = self._device.cardano_sign_transaction(
                transaction=bitbox02.cardano.CardanoSignTransactionRequest(
                    network=bitbox02.cardano.CardanoMainnet,
                    inputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Input(
                            keypath=[2147485500, 2147485463, 2147483648, 0, 0],
                            prev_out_hash=bytes.fromhex(
                                "b7b2333e72f2670ab82051f426cc84000431975a34e71d5edf70ea6c0ddc9bf8"
                            ),
                            prev_out_index=0,
                        )
                    ],
                    outputs=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Output(
                            encoded_address=get_address(False),
                            value=4817591,
                            script_config=script_config,
                        )
                    ],
                    fee=175157,
                    ttl=41788708,
                    withdrawals=[
                        bitbox02.cardano.CardanoSignTransactionRequest.Withdrawal(
                            keypath=[2147485500, 2147485463, 2147483648, 2, 0],
                            value=1234567,
                        )
                    ],
                )
            )
            print([(w.public_key.hex(), w.signature.hex()) for w in response.shelley_witnesses])

        choices = (
            ("Retrieve account xpubs", xpubs),
            ("Retrieve a Shelley address", address),
            ("Sign a transaction", sign),
            ("Sign a transaction with TTL=0", sign_zero_ttl),
            ("Sign a transaction sending tokens", sign_tokens),
            ("Delegate staking to a pool", delegate),
            ("Delegate vote to a dRep", delegate_vote),
            ("Withdraw staking rewards", withdraw),
        )
        choice = ask_user(choices)
        if callable(choice):
            try:
                choice()
            except UserAbortException:
                eprint("Aborted by user")

    def _bluetooth_upgrade(self) -> None:
        filename = input("Enter path to the firmware [bitbox-da14531-firmware.bin]: ")
        if filename == "":
            filename = "bitbox-da14531-firmware.bin"
        firmware = Path(filename).read_bytes()
        try:
            self._device.bluetooth_upgrade(firmware)
        except UserAbortException:
            print("Aborted by user")

    def _bluetooth_toggle_enabled(self) -> None:
        try:
            self._device.bluetooth_toggle_enabled()
        except UserAbortException:
            print("Aborted by user")

    def _reset_device(self) -> None:
        if self._device.reset():
            print("Device RESET")
            self._stop = True
        else:
            print("Device NOT reset")

    def _change_password_workflow(self) -> None:
        """Initiate the change password workflow."""
        try:
            self._device.change_password()
            print("Change password workflow completed")
        except UserAbortException:
            eprint("Aborted by user")

    @staticmethod
    def _bitboxsync_wrap_namespace_dek(
        recipient_public_key: bytes,
        namespace_id: bytes,
        namespace_dek: bytes,
    ) -> bytes:
        # pylint: disable=import-outside-toplevel
        try:
            from pyhpke import AEADId, CipherSuite, KDFId, KEMId
        except ModuleNotFoundError as err:
            raise RuntimeError(
                "pyhpke is required to generate BitBoxSync wrapped DEK test data"
            ) from err

        suite = CipherSuite.new(
            KEMId.DHKEM_X25519_HKDF_SHA256,
            KDFId.HKDF_SHA256,
            AEADId.CHACHA20_POLY1305,
        )
        recipient_public = suite.kem.deserialize_public_key(recipient_public_key)
        enc, sender = suite.create_sender_context(
            recipient_public,
            info=b"bitboxsync-wrap-dek-v1",
        )
        ciphertext = sender.seal(namespace_id + namespace_dek)
        return b"\x01" + enc + ciphertext

    def _bitboxsync_print_identity(self) -> None:
        response = self._device.bitboxsync_identity()
        print("Auth public key:", response["auth_public_key"].hex())
        print("Wrap public key:", response["wrap_public_key"].hex())

    def _bitboxsync_sign_login_intent(self) -> None:
        signature = self._device.bitboxsync_sign_login_intent(BITBOXSYNC_CHALLENGE)
        print("Login signature:", signature.hex())

    def _bitboxsync_sign_refresh_intent(self) -> None:
        signature = self._device.bitboxsync_sign_refresh_intent(BITBOXSYNC_CHALLENGE)
        print("Refresh signature:", signature.hex())

    def _bitboxsync_sign_revoke_all_tokens_intent(self) -> None:
        signature = self._device.bitboxsync_sign_revoke_all_tokens_intent(BITBOXSYNC_CHALLENGE)
        print("Revoke all tokens signature:", signature.hex())

    def _bitboxsync_sign_create_namespace_invite_intent(self) -> None:
        signature = self._device.bitboxsync_sign_create_namespace_invite_intent(
            BITBOXSYNC_CHALLENGE,
            BITBOXSYNC_NAMESPACE_ID,
            BITBOXSYNC_INVITE_ID,
            BITBOXSYNC_INVITE_SERVER_SECRET_HASH,
            BITBOXSYNC_EXPIRES_AT,
            BITBOXSYNC_MAX_ACCEPTED,
        )
        print("Create namespace invite signature:", signature.hex())

    def _bitboxsync_sign_join_request_intent(self) -> None:
        signature = self._device.bitboxsync_sign_join_request_intent(
            BITBOXSYNC_NAMESPACE_ID,
            BITBOXSYNC_INVITE_ID,
            BITBOXSYNC_SERVER_ORIGIN,
            BITBOXSYNC_EXPIRES_AT,
        )
        print("Join request signature:", signature.hex())

    def _bitboxsync_unwrap_namespace_dek(self) -> None:
        identity = self._device.bitboxsync_identity()
        wrapped_dek = self._bitboxsync_wrap_namespace_dek(
            identity["wrap_public_key"],
            BITBOXSYNC_NAMESPACE_ID,
            BITBOXSYNC_NAMESPACE_DEK,
        )
        namespace_dek = self._device.bitboxsync_unwrap_namespace_dek(
            BITBOXSYNC_NAMESPACE_ID,
            wrapped_dek,
        )
        print("Unwrapped namespace DEK:", namespace_dek.hex())
        if namespace_dek != BITBOXSYNC_NAMESPACE_DEK:
            raise Exception("Unexpected namespace DEK returned")

    def _bitboxsync(self) -> None:
        choices = (
            ("Identity", self._bitboxsync_print_identity),
            ("Sign login intent", self._bitboxsync_sign_login_intent),
            ("Sign refresh intent", self._bitboxsync_sign_refresh_intent),
            (
                "Sign revoke all tokens intent",
                self._bitboxsync_sign_revoke_all_tokens_intent,
            ),
            (
                "Sign create namespace invite intent",
                self._bitboxsync_sign_create_namespace_invite_intent,
            ),
            ("Sign join request intent", self._bitboxsync_sign_join_request_intent),
            ("Unwrap namespace DEK", self._bitboxsync_unwrap_namespace_dek),
        )
        choice = ask_user(choices)
        if callable(choice):
            try:
                choice()
            except UserAbortException:
                eprint("Aborted by user")

    def _menu_notinit(self) -> None:
        """TODO: Document

        Returns:
            bool: If the user should be prompted again
        """
        choices = (
            ("Set up a new wallet", self._setup_workflow),
            ("Restore from backup", self._restore_backup_workflow),
            ("Restore from mnemonic", self._restore_from_mnemonic),
            ("List device info", self._list_device_info),
            ("Reboot into bootloader", self._reboot),
            ("Check if SD card inserted", self._check_sd_presence),
            ("Upgrade Bluetooth firmware", self._bluetooth_upgrade),
        )
        choice = ask_user(choices)
        if isinstance(choice, bool):
            self._stop = True
            return
        if choice is None:
            return
        choice()

    def _menu_init(self) -> None:
        """Print the menu"""
        choices = (
            ("List device info", self._list_device_info),
            ("Change device name", self._change_name_workflow),
            ("Get root fingerprint", self._get_root_fingerprint),
            ("Retrieve zpub of first account", self._display_zpub),
            ("Retrieve multiple xpubs", self._btc_xpubs),
            ("Retrieve a BTC address", self._btc_address),
            ("Retrieve a BTC Multisig address", self._btc_multisig_address),
            ("Retrieve a BTC policy address", self._btc_policy_address),
            ("Sign a BTC tx", self._sign_btc_tx),
            ("Sign a BTC Message", self._sign_btc_message),
            ("List backups", self._print_backups),
            ("Check backup", self._check_backup),
            ("Show mnemonic", self._show_mnemnoic_seed),
            ("Create backup", self._create_backup),
            ("Reboot into bootloader", self._reboot),
            ("Check if SD card inserted", self._check_sd_presence),
            ("Insert SD card", self._insert_sdcard),
            ("Toggle BIP39 Mnemonic Passphrase", self._toggle_mnemonic_passphrase),
            ("Retrieve Ethereum xpub", self._get_eth_xpub),
            ("Retrieve Ethereum address", self._display_eth_address),
            (
                "Retrieve ERC20 address with long token name",
                lambda: self._display_eth_address(
                    contract_address=b"\xba\x11\xd0\x0c\x5f\x74\x25\x5f\x56\xa5\xe3\x66\xf4\xf7\x7f\x5a\x18\x6d\x7f\x55"
                ),
            ),
            ("Sign Ethereum tx", self._sign_eth_tx),
            ("Sign Ethereum Message", self._sign_eth_message),
            ("Sign Ethereum Typed Message (EIP-712)", self._sign_eth_typed_message),
            (
                "Sign Ethereum Typed Message (50KB streaming) ",
                self._sign_eth_typed_message_large_data,
            ),
            ("Cardano", self._cardano),
            ("Solana", self._solana),
            ("XRP", self._xrp),
            ("Tron", self._tron),
            ("Zcash (transparent)", self._zcash),
            ("Show Electrum wallet encryption key", self._get_electrum_encryption_key),
            ("BIP85 - BIP39", self._bip85_bip39),
            ("BIP85 - LN", self._bip85_ln),
            ("Upgrade Bluetooth firmware", self._bluetooth_upgrade),
            ("Toggle bluetooth", self._bluetooth_toggle_enabled),
            ("BitBoxSync", self._bitboxsync),
            ("Reset Device", self._reset_device),
            ("Change Password", self._change_password_workflow),
        )
        choice = ask_user(choices)
        if isinstance(choice, bool):
            self._stop = True
            return
        if choice is None:
            return
        choice()

    def _menu(self) -> None:
        if not self._device.device_info()["initialized"]:
            self._menu_notinit()
            return
        self._menu_init()

    def run(self) -> int:
        """Entry point for program"""
        if self._debug:
            self._device.debug = True

        while not self._stop:
            self._menu()
        self._device.close()
        return 0


class SendMessageBootloader:
    """Simple test application for bootloader"""

    def __init__(self, device: Bootloader):
        self._device = device
        self._stop = False

    def _boot(self) -> None:
        self._device.reboot()
        self._stop = True

    def _get_versions(self) -> None:
        if self._device.erased():
            print("No firmware on device")
        else:
            version = self._device.versions()
            print(f"Firmware version: {version[0]}, Pubkeys version: {version[1]}")

    def _get_hardware(self) -> None:
        secure_chip = self._device.hardware()["secure_chip_model"]
        print("Hardware variant:")
        print(f"- Secure Chip: {secure_chip.value}")

    def _erase(self) -> None:
        self._device.erase()

    def _show_fw_hash(self) -> None:
        self._device.set_show_firmware_hash(True)

    def _dont_show_fw_hash(self) -> None:
        self._device.set_show_firmware_hash(False)

    def _get_hashes(self) -> None:
        firmware_hash, sigkeys_hash = self._device.get_hashes()
        print("Firmware hash:")
        print("\n".join(textwrap.wrap(firmware_hash.hex(), 16)))
        if input("Display on device? y/[n]: ") == "y":
            self._device.get_hashes(display_firmware_hash=True)
        print("Signature keys hash:")
        print("\n".join(textwrap.wrap(sigkeys_hash.hex(), 16)))
        if input("Display on device? y/[n]: ") == "y":
            self._device.get_hashes(display_signing_keydata_hash=True)

    def _menu(self) -> None:
        choices = (
            ("Boot", self._boot),
            ("Print versions", self._get_versions),
            ("Print hardware variant", self._get_hardware),
            ("Erase firmware", self._erase),
            ("Show firmware hash at startup", self._show_fw_hash),
            ("Don't show firmware hash at startup", self._dont_show_fw_hash),
            ("Get firmware & sigkey hashes", self._get_hashes),
            ("Rotate screen", self._device.screen_rotate),
        )
        choice = ask_user(choices)
        if isinstance(choice, bool):
            self._stop = True
            return
        if choice is None:
            return
        choice()

    def run(self) -> int:
        while not self._stop:
            self._menu()
        self._device.close()
        return 0


class U2FApp:
    """App"""

    APPID = "http://example.com"

    def __init__(self, device: u2f.bitbox02.BitBox02U2F, debug: bool):
        self._device = device
        self._stop = False
        self._dev_keyhandle: bytes = b"0" * 64
        self._dev_pubkey: bytes = b"0" * 64
        if debug:
            self._device.debug = True

    def _wink(self) -> None:
        print("Wink")
        self._device.u2fhid_wink()

    def _ping(self) -> None:
        ans = input("Message: ")
        res = self._device.u2fhid_ping(ans.encode("utf-8"))
        print(res.decode("utf-8"))

    def _register(self) -> None:
        try:
            res = self._device.u2f_register(self.APPID)
            if res is not None:
                (self._dev_pubkey, self._dev_keyhandle) = res
        except u2f.ConditionsNotSatisfiedException:
            print("Not registered")

    def _bogus(self) -> None:
        ans = input("Vendor [chromium, firefox]: ")
        try:
            self._device.u2f_register_bogus(ans)
        except ValueError as err:
            print("Invalid vendor, try again: {}".format(err))
        except u2f.ConditionsNotSatisfiedException:
            print("User not present")

    def _authenticate(self) -> None:
        if self._dev_keyhandle == b"0" * 64:
            print("Not yet registered, authenticating anyway...")
        try:
            self._device.u2f_authenticate(self.APPID, self._dev_keyhandle, self._dev_pubkey)
            print("User present")
        except u2f.ConditionsNotSatisfiedException:
            print("User not present")
        except u2f.WrongDataException:
            print("Keyhandle not for this key")

    def _menu(self) -> None:
        """Menu"""
        print("What would you like to do?")
        choices = (
            ("Wink", self._wink),
            ("Ping", self._ping),
            ("Register", self._register),
            ("Register with bogus AppId", self._bogus),
            ("Authenticate", self._authenticate),
        )
        choice = ask_user(choices)
        if isinstance(choice, bool):
            self._stop = True
            return
        if choice is None:
            return
        choice()

    def run(self) -> int:
        """Main function"""
        while not self._stop:
            self._menu()
        self._device.close()
        return 0


def connect_to_simulator_bitbox(
    debug: bool,
    port: int,
    command: Optional[Callable[[bitbox02.BitBox02], None]] = None,
) -> int:
    """
    Connects and runs the main menu on host computer,
    simulating a BitBox02 connected over USB.
    """

    class Simulator(PhysicalLayer):
        """
        Simulator class handles the communication
        with the firmware simulator
        """

        def __init__(self) -> None:
            self.client_socket = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            self.client_socket.connect(("127.0.0.1", port))
            if debug:
                print("Connected to the simulator")

        def write(self, data: bytes) -> None:
            self.client_socket.send(data[1:])
            if debug:
                print(f"Written to the simulator:\n{data.hex()[2:]}")

        def read(self, size: int, timeout_ms: int) -> bytes:
            res = self.client_socket.recv(64)
            if debug:
                print(f"Read from the simulator:\n{res.hex()}")
            return res

        def close(self) -> None:
            return None

        def __del__(self) -> None:
            print("Simulator quit")
            self.client_socket.close()

    simulator = Simulator()
    noise_config = bitbox_api_protocol.BitBoxNoiseConfig()
    bitbox_connection = bitbox02.BitBox02(
        transport=u2fhid.U2FHid(simulator),
        device_info=None,
        noise_config=noise_config,
    )
    try:
        bitbox_connection.check_min_version()
    except FirmwareVersionOutdatedException as exc:
        print("WARNING: ", exc)

    return _run_cli_command(bitbox_connection, debug, command)


def connect_to_usb_bitbox(
    debug: bool,
    use_cache: bool,
    command: Optional[Callable[[bitbox02.BitBox02], None]] = None,
) -> int:
    """
    Connects and runs the main menu on a BitBox02 connected
    over USB.
    """

    def connect_to_bootloader() -> int:
        try:
            bootloader = devices.get_any_bitbox02_bootloader()
        except devices.TooManyFoundException:
            print("Multiple bitbox bootloaders detected. Only one supported")
            return 1
        except devices.NoneFoundException:
            print("Neither bitbox nor bootloader found.")
            return 1
        hid_device = hid.device()
        try:
            hid_device.open_path(bootloader["path"])
        except OSError:
            print(
                "Could not connect to the BitBox, device may be already connected to another app."
            )
            return 1
        bootloader_connection = Bootloader(u2fhid.U2FHid(hid_device), bootloader)
        if command is not None:
            print("Altcoin commands require a device running firmware, not the bootloader.")
            bootloader_connection.close()
            return 1
        boot_app = SendMessageBootloader(bootloader_connection)
        return boot_app.run()

    try:
        bitbox = devices.get_any_bitbox02()
    except devices.TooManyFoundException:
        print("Multiple bitboxes detected. Only one supported")
        return 1
    except devices.NoneFoundException:
        return connect_to_bootloader()

    def show_pairing(code: str, device_response: Callable[[], bool]) -> bool:
        print("Please compare and confirm the pairing code on your BitBox02:")
        print(code)
        if not device_response():
            return False
        return input("Accept pairing? [y]/n: ").strip() != "n"

    class NoiseConfig(util.NoiseConfigUserCache):
        """NoiseConfig extends NoiseConfigUserCache"""

        def __init__(self) -> None:
            super().__init__("shift/send_message")

        def show_pairing(self, code: str, device_response: Callable[[], bool]) -> bool:
            return show_pairing(code, device_response)

        def attestation_check(self, result: bool) -> None:
            if result:
                print("Device attestation PASSED")
            else:
                print("Device attestation FAILED")

    class NoiseConfigNoCache(bitbox_api_protocol.BitBoxNoiseConfig):
        """NoiseConfig extends BitBoxNoiseConfig"""

        def show_pairing(self, code: str, device_response: Callable[[], bool]) -> bool:
            return show_pairing(code, device_response)

        def attestation_check(self, result: bool) -> None:
            if result:
                print("Device attestation PASSED")
            else:
                print("Device attestation FAILED")

    if use_cache:
        config: bitbox_api_protocol.BitBoxNoiseConfig = NoiseConfig()
    else:
        config = NoiseConfigNoCache()

    hid_device = hid.device()
    try:
        hid_device.open_path(bitbox["path"])
    except OSError:
        print("Could not connect to the BitBox, device may be already connected to another app.")
        return 1
    bitbox_connection = bitbox02.BitBox02(
        transport=u2fhid.U2FHid(hid_device), device_info=bitbox, noise_config=config
    )
    try:
        bitbox_connection.check_min_version()
    except FirmwareVersionOutdatedException as exc:
        print("WARNING: ", exc)

    if debug:
        print("Device Info:")
        pprint.pprint(bitbox)
    return _run_cli_command(bitbox_connection, debug, command)


def main() -> int:
    """Main function"""
    parser = argparse.ArgumentParser(description="Tool for communicating with bitbox device")
    parser.add_argument("--debug", action="store_true", help="Print messages sent and received")
    parser.add_argument("--u2f", action="store_true", help="Use u2f menu instead")
    parser.add_argument(
        "--simulator",
        action="store_true",
        help="Connect to the BitBox02 simulator instead of a real BitBox02",
    )
    parser.add_argument(
        "--simulator-port",
        default=15423,
        type=int,
        help="Simulator port",
    )
    parser.add_argument(
        "--no-cache", action="store_true", help="Don't use cached or store noise keys"
    )
    parser.set_defaults(command_handler=None)
    _add_altcoin_commands(parser)
    args = parser.parse_args()

    command = (
        None if args.command_handler is None else lambda device: args.command_handler(device, args)
    )

    if args.u2f:
        if command is not None:
            parser.error("--u2f cannot be combined with an altcoin command")
        try:
            u2fbitbox = u2f.bitbox02.get_bitbox02_u2f_device()
        except devices.TooManyFoundException:
            print("Multiple bitboxes detected. Only one supported")
        except devices.NoneFoundException:
            print("No bitboxes detected")
        else:
            hid_device = hid.device()
            hid_device.open_path(u2fbitbox["path"])
            u2fdevice = u2f.bitbox02.BitBox02U2F(hid_device)
            u2fapp = U2FApp(u2fdevice, args.debug)
            return u2fapp.run()
        return 1

    if args.simulator:
        return connect_to_simulator_bitbox(args.debug, args.simulator_port, command)

    return connect_to_usb_bitbox(args.debug, not args.no_cache, command)


if __name__ == "__main__":
    sys.exit(main())
