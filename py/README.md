# BitBox python scripts

This directory contains scripts to talk to the BitBox device directly via the command line
 (e.g. `send_message.py`, `load_firmware.py`).

## Setup

These instructions require Python 3.10 or newer, pip, and venv (a lightweight "virtual
environment"). Inside the virtual environment, `python` and `pip` refer to Python 3.

All commands below assume you are in the `py/` directory.

### Requirements

- Python >= 3.10
- pip >= 25

Editable installs (`pip install -e …`) are only supported with pip 25 or newer.  
Older pip versions will fail due to changes in how editable installs are handled (PEP 660).

### Installing dependencies

Install the required Python dependencies listed in `requirements.txt`:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

## Activate the virtual environment

If you open a new shell, remember to re-activate the virtual environment,
to use the scripts and communicate with the BitBox.

```
source .venv/bin/activate
```

You can deactivate a virtual environment by typing `deactivate` in your shell.


## Communicate with the BitBox

This assumes that the firmware was installed on the BitBox device. To flash the
firmware manually read the next section or install with the official BitBoxApp.

Connect your BitBox and "tap this side".

List and execute all available commands by running:

```bash
python ./send_message.py
```

This command will list what commands are currently possible, depending on which
mode the device currently is, i.e "Bootloader mode" accepts different commands.
From here you can execute any command the BitBox accepts.

```
What would you like to do?
- (1) List device info
- (2) Change device name
- (3) Get root fingerprint
- (4) Retrieve zpub of first account
- (5) Retrieve multiple xpubs
- (6) …
```

### Altcoin command-line tests

Solana, XRP, Tron, and transparent Zcash are available as entries in the interactive menu. Each
entry includes ready-to-sign demo transfers using the first account, including SPL and TRC-20 token
examples. These transactions contain dummy block references or outpoints and are intended only for
testing, not broadcasting.

Custom requests can be sent directly using the commands below. Global options such as `--simulator`
and `--debug` must precede the command. Run `python ./send_message.py COMMAND --help` for all
options.

```bash
python ./send_message.py solana-address --display
python ./send_message.py solana-sign --network devnet --message-hex "$MESSAGE_HEX"

python ./send_message.py xrp-address --display
python ./send_message.py xrp-sign \
    --destination r9cZA1mLK5R5Am25ArfXFmqgNwjZgnfk59 \
    --amount 1000000 --fee 12 --sequence 1 --destination-tag 42

python ./send_message.py tron-address --display
python ./send_message.py tron-sign --network testnet --raw-data-hex "$RAW_DATA_HEX"

python ./send_message.py zcash-address --display
python ./send_message.py zcash-sign --transaction ./zcash-transaction.json
```

Keypaths use `m/...` notation and default to each coin's first BIP44 account/address. Solana's
`--message-hex` is a serialized legacy or v0 message without signatures. Tron's
`--raw-data-hex` is the canonical protobuf encoding of `protocol.Transaction.raw`.

The Zcash JSON format is transparent-only. An output without a `keypath` is external; an output
with a `keypath` is verified as change by the device. Byte fields are hexadecimal, while integer
fields may be JSON numbers or strings such as `"0xc8e71055"`.

```json
{
  "inputs": [
    {
      "keypath": "m/44'/133'/0'/0/0",
      "prev_out_hash": "0000000000000000000000000000000000000000000000000000000000000000",
      "prev_out_index": 0,
      "value": 100000,
      "script_pubkey": "76a914000000000000000000000000000000000000000088ac",
      "sequence": "0xfffffffe"
    }
  ],
  "outputs": [
    {
      "value": 99000,
      "script_pubkey": "76a914111111111111111111111111111111111111111188ac"
    }
  ],
  "lock_time": 0,
  "expiry_height": 3000000,
  "consensus_branch_id": "0xc8e71055"
}
```

The input script must match the key derived at its `keypath`; replace the placeholder hashes in
the example with real transaction data before signing.

When connecting the first time to an initialized but unpaired BitBox, the device
will prompt to unlock and continue to compare and confirm the Noise pairing key.
This is a one-time action.


## Flash the firmware.bin

Use the following script to flash the firmware.bin onto the BitBox.
The script prompts to enter the bootloader when necessary and confirms the detected firmware and
device types before flashing.

```bash
python ./load_firmware.py ./firmware.signed.bin
```

Signed firmware is detected by its header; all other input is treated as raw unsigned firmware.
The file name is not used. A recognized but malformed signed-firmware container is rejected.

The supported combinations are:

| Firmware input | Production device | Development device |
| --- | --- | --- |
| Signed | Flash firmware and signature data; all errors are fatal | Flash firmware, attempt signature data, warn if the signature data is rejected, and reboot anyway |
| Raw unsigned | Warn that firmware verification will fail, then flash without signature data | Flash firmware without signature data |

On production devices the bootloader only accepts newer signed firmware versions and
[prevents downgrades](https://bitbox.swiss/bitbox02/security-features/#secure-bootloader). On a
production device, unsigned firmware cannot boot. A signed firmware for a different product or
edition is allowed after a warning, but installing its signature data is expected to fail.

Every flash requires confirmation. Use `-y` or `--yes` to skip the prompt for non-interactive use:

```bash
python ./load_firmware.py --yes ./firmware.bin
```

The deprecated `--debug` option is accepted for backwards compatibility but has no effect.

Contributors that don't have a dev-devices please refer to the
[simulator](https://github.com/BitBoxSwiss/bitbox02-firmware?tab=readme-ov-file#simulator).

For building the BitBox firmware please refer to the
[reproduce the firmware](https://github.com/BitBoxSwiss/bitbox02-firmware/tree/master/releases#reproducible-builds) documentation.


## Development

To work on the library or scripts, install them in editable mode.
Editable installs are only needed if you want to modify the scripts or library code.

```bash
pip install -e ./bitbox02
```

For developing the Python sources, almost all of it can be done on the host easily.
To regenerate protobufs it is recommended to use the Docker container.
Read more about dockerized setup in [BUILD.md](../BUILD.md).
