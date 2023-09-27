# ETH Transaction Simulator

A PoC for an Ethereum transaction simulator, which compares two approaches:

1. Use standard ETH RPC calls to fetch the needed state for the simulation and run the EVM code locally
2. Use a local reth DB to fetch the data and run the EVM code locally

The local DB approach is much faster compared to using RPC, even when the RPC is running locally.

Here are some benchmark results of simulating mempool transactions using reth running locally on WSL2 on Windows 11, Ryzen 5 5600X, 24GB RAM, Seagate FireCuda 530 4TB NVME. There is some virtualization overhead, because of the WSL2. Running these natively on a linux server should yield even faster results.

https://docs.google.com/spreadsheets/d/1Hj3_zIlqblrIYF4wM16qTNP6GYexrePaXvDjo0NNCkQ/edit?usp=sharing

## Run

In order to run the mempool simulator copy the `.env.example` file to `.env`. Set `WS_RPC_URL` and `HTTP_RPC_URL` to a node which supports fetching the pending transactions in the mempool. Also set `RETH_DB_PATH` to the location of the DB of a locally synced reth node.

```bash
$ cp .env.example .env
$ cargo run --release
```

Make sure to run with `--release`, so that you can see the true performance of the reth DB simulator.