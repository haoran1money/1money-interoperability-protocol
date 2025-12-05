# Local Sidechain Devnet

Spin up the sidechain devnet with custom 1money L1 validator keys.

## Collect validator private keys

```bash
cat /tmp/1m-network/node*/conf/consensus_secret_key.hex
```

You should see four keys like:

```
0x0b03eac696409d63e0512e2e0b39ac5be3dd58dc86b3e8b3a0ddab929c2bf693
0xfa1c3f2b6488e9a9a83873df22b50ddbef221525bb8158fbb647f2fc6fe0a0dc
0x0466a99f2607d41f009e6145a661fcb91051c2c5de6d7983561b73b34a242373
0x87e906c8870d3bc997e529edc64344bcc69031aa31a511254c342b8c86c89bda
0x7b0a34502d28ff0e38be81782d8fa562281c91c6cef1a27b14a791964cb7c795
0x2fb14000aa767cf73e99cbed450f84fad7e114887c884320853599ac6dad5d6d
0x9b14d725669c0966a44072edd8721dbd924496e6d32498e107250a289a4ad238
```

## Clone emerald

This setup works with commit `93f8b04e025d40ce4710486cde680be6ddd52b31`

```bash
git clone git@github.com:informalsystems/emerald
cd emerald
make build # builds cargo binaries and solidity contracts
```

## Generate devnet configuration

Create `.testnet` with your keys (example uses the ones above):

```bash
./scripts/generate_testnet_config.sh \
  --node-keys 0x0b03eac696409d63e0512e2e0b39ac5be3dd58dc86b3e8b3a0ddab929c2bf693 \
  --node-keys 0xfa1c3f2b6488e9a9a83873df22b50ddbef221525bb8158fbb647f2fc6fe0a0dc \
  --node-keys 0x0466a99f2607d41f009e6145a661fcb91051c2c5de6d7983561b73b34a242373 \
  --node-keys 0x87e906c8870d3bc997e529edc64344bcc69031aa31a511254c342b8c86c89bda \
  --node-keys 0x7b0a34502d28ff0e38be81782d8fa562281c91c6cef1a27b14a791964cb7c795 \
  --node-keys 0x2fb14000aa767cf73e99cbed450f84fad7e114887c884320853599ac6dad5d6d \
  --node-keys 0x9b14d725669c0966a44072edd8721dbd924496e6d32498e107250a289a4ad238 \
  --testnet-config-dir .testnet \
  --fee-recipient <address as hex>
```

> Note: If the `--fee-recipient` is not specified it will be set to
> `0x4242424242424242424242424242424242424242`. It is still possible to update
> the value in the generated configurations in
> `.testnet/config/<node_id>/config.toml` after the configurations have been
> generated.

This sets node count from the keys, writes `.testnet/testnet_config.toml`, and
emits per-node configs in `.testnet/config/`.

## Generate node configs

```bash
cargo run --bin emerald -- testnet \
  --home nodes \
  --testnet-config .testnet/testnet_config.toml
```

Node directories and validator keys land in `./nodes/{0,1,2,3}/`.

## Extract validator public keys

```bash
ls nodes/*/config/priv_validator_key.json | \
  xargs -I{} cargo run --bin emerald show-pubkey {} \
  > nodes/validator_public_keys.txt
```

## Create the genesis file

```bash
cargo run --bin emerald-utils genesis \
  --public-keys-file ./nodes/validator_public_keys.txt \
  --poa-owner-address "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266" \
  --devnet
```

`./assets/genesis.json` is produced for Reth.

## Start execution clients and monitoring

```bash
docker compose up -d reth0 reth1 reth2 reth3 reth4 reth5 reth6 prometheus grafana otterscan
```

## Connect Reth peers

```bash
./scripts/add_peers.sh --nodes 7
```

The script waits for readiness, reads each enode, and sets trusted peers across
nodes.

## Start consensus nodes

```bash
bash scripts/spawn.bash --nodes 7 --home nodes --no-delay
```

This builds the binary, starts all nodes, logs to
`nodes/{0,1,2,3}/logs/node.log`, and keeps running until you press Ctrl+C.

## Utilities

- Otterscan block explorer is available at http://localhost:80
- Grafana dashboards are available at http://localhost:3000
