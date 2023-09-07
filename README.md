# ETH Transaction Simulator

The current implementation uses Mainnet forks against an RPC node to do the simulations. In the future, the simulator will use the reth internal DB to remove the network latency.

## Run

In order to run the mempool simulator copy the `.env.example` file to `.env`. Set an alternative `RPC_URL` if you want to do that. Then run the main:

```bash
$ cp .env.example.env
$ cargo run
```