from web3 import Web3
import eth_utils
import os
import sys
import toml
import dydx3.constants as dydx
from dydx3 import Client


INFURA_URL = "https://mainnet.infura.io/v3/669bebaf71c248f59cb43cb6a5ebd72d"

USDC_CONTRACT_ADDRESS = '0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48'

CONTRACT_ABI = [
    {
        "constant": True,
        "inputs": [
            {
                "name": "_owner",
                "type": "address"
            }
        ],
        "name": "balanceOf",
        "outputs": [
            {
                "name": "balance",
                "type": "uint256"
            }
        ],
        "payable": False,
        "stateMutability": "view",
        "type": "function"
    },

    {
        "constant": False,
        "inputs": [
            {
                "name": "_to",
                "type": "address"
            },
            {
                "name": "_value",
                "type": "uint256"
            }
        ],
        "name": "transfer",
        "outputs": [
            {
                "name": "",
                "type": "bool"
            }
        ],
        "payable": False,
        "stateMutability": "nonpayable",
        "type": "function"
    },
]

try:
    CFG = toml.load('config.toml')
    ETH_KEY = CFG['wallet']['key']
    ETH_PRIVATE_KEY = CFG['wallet']['secret']
    WEB3 = Web3(Web3.HTTPProvider(INFURA_URL))
    USDC_CONTRACT = WEB3.eth.contract(abi=CONTRACT_ABI, address=USDC_CONTRACT_ADDRESS)
    CMD = sys.argv[1]
    if CMD == "--balance":
        usdc_amount = USDC_CONTRACT.functions.balanceOf(ETH_KEY).call()
        print(f"USDC: {usdc_amount}")
        eth_amount = WEB3.eth.getBalance(ETH_KEY)
        print(f"ETH: {eth_amount}")
    elif CMD == "--deposit":
        amount = int(sys.argv[3])
        to = sys.argv[2]
        if to == "1":
            #
            # To dYdX:
            # NOTE: based on dYdX test examples:
            #
            client = Client(
                host=dydx.API_HOST_MAINNET,
                network_id=dydx.NETWORK_ID_MAINNET,
                stark_private_key=CFG['dydx']['stark_private_key'],
                default_ethereum_address=ETH_KEY,
                eth_private_key=ETH_PRIVATE_KEY,
                web3=WEB3,
            )
            account_response = client.private.get_account()
            position_id = account_response.data['account']['positionId']
            print(f"position_id = {position_id}")
            # TODO make it once for the account
            # approve_tx_hash = client.eth.set_token_max_allowance(
            #     client.eth.get_exchange_contract().address,
            # )
            # print(f"Waiting for approve (approve tx hash: {approve_tx_hash})")
            # sys.exit(1)
            # #client.eth.wait_for_tx(approve_tx_hash)
            # tx_receipt = WEB3.eth.wait_for_transaction_receipt(approve_tx_hash, timeout=600)
            # if tx_receipt['status'] == 0:
            #     print(f"Approve transaction {approve_tx_hash} reverted", file=sys.stderr)
            #     sys.exit(1)
            # print('...done.')
            human_amount = amount / 1000000
            print(f"Deposit {human_amount} USDC")
            deposit_tx_hash = client.eth.deposit_to_exchange(position_id, human_amount)
            print(f'Waiting for deposit (tx hash: {deposit_tx_hash})...')
            # client.eth.wait_for_tx(deposit_tx_hash)
            tx_receipt = WEB3.eth.wait_for_transaction_receipt(deposit_tx_hash, timeout=600)
            if tx_receipt['status'] == 0:
                print(f"Transaction {deposit_tx_hash} reverted", file=sys.stderr)
                sys.exit(1)
            print('...done.')
            print(f"TXHASH: {deposit_tx_hash}")
        elif to == "2" or to == "3":
            #
            # To Kraken/Binance:
            #
            if to == "2":
                to_account = Web3.toChecksumAddress(CFG['kraken']['usdc_account'])
            elif to == "3":
                to_account = Web3.toChecksumAddress(CFG['binance']['usdc_account'])
            else:
                print(f"Unknown account {to}")
                sys.exit(1)
            print(f"DEPOSIT: '{to}' (address: {to_account}), '{amount}'")
            if True:
                nonce = WEB3.eth.getTransactionCount(ETH_KEY)
                print(f'nonce = {nonce}')
                tx = {'type': '0x2',
                      'nonce': nonce,
                      'from': ETH_KEY,
                      #'to': to_account,
                      #'value':  0xf4240,
                      'maxFeePerGas': WEB3.toWei('150', 'gwei'), # TODO
                      'maxPriorityFeePerGas': WEB3.toWei('10', 'gwei'), # TODO
                      'chainId': 1
                     }
                print(f'tx = {tx}')
                estGas = WEB3.eth.estimateGas(tx)
                tx['gas'] = int(estGas * 1.3) # TODO make right gas calculation
                print(f"GAS: {tx['gas']}")
                tx = USDC_CONTRACT.functions.transfer(to_account, amount).build_transaction(tx)
                tx['to'] = USDC_CONTRACT_ADDRESS
                #print(f"RES: {tx}")
                signed_tx = WEB3.eth.account.sign_transaction(tx, ETH_PRIVATE_KEY)
                tx_hash = WEB3.eth.send_raw_transaction(signed_tx.rawTransaction)
                tx_hash_str = str(WEB3.toHex(tx_hash))
                print(f"Waiting for transacton {tx_hash_str}...")
                tx_receipt = WEB3.eth.wait_for_transaction_receipt(tx_hash_str, timeout=600)
                if tx_receipt['status'] == 0:
                    print(f"Transaction {tx_hash_str} reverted", file=sys.stderr)
                    sys.exit(1)
                print('...done.')
                print(f"TXHASH: {tx_hash_str}")
            else:
                # Just debug output:
                print("TXHASH: 0x306d9089ac74119d9e7f96b73c56de473f9e66643c2e4924a90a5daa0b56a760")
        else:
            print(f"Unknown account: {to}", file=sys.stderr)
            sys.exit(1)
    else:
        print(f'Unknown command: "{cmd}"', file=sys.stderr)
        sys.exit(1)

except:
    ei = sys.exc_info()
    print(f'{ei[0].__name__}: {repr(ei[1])}', file=sys.stderr)
    sys.exit(1)

