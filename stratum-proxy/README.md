# Overview

This proxy demonstrates 1:1 translation of Stratum V2 protocol to Stratum V1. It
listens for downstream Stratum V2 connections and translates the protocol to
upstream Stratum V1 node.


# Build

## Prerequisites

- Rust toolchain installed via [rustup](https://rustup.rs/)

## Building it

`cargo build --release`

## Running the proxy via cargo
See sample configurations in `config` directory, check if listen_address (proxy socket address)
and upstream_address are correctly set or leave provided defaults.

If secure mode is required, check that certificate and secret key fiel paths are set correctly.
### Running insecure Stratum V2 protocol version
1. run proxy with insecure configuration option:
   `cargo run --release -- --conf config/insecure.toml`
1. configure bosminer pool url to use insecure scheme:
    `stratum2+tcp+insecure://<proxy socket address>`
### Running secure Stratum V2 protocol version
1. generate keys and certificates: `bash config/gen_keys.sh`. Generated keys and certificates are stored in
   config directory so that their relative path matches default sample configuration for secure mode.
1. `cargo run --release -- --conf config/secure.toml`
1. configure bosminer pool url to validate against generated authority_public_key:
    1. `cat config/ca-ed25519-public.key`
    2.  -> `{"ed25519_public_key": "ZZ6uJT6kaDRKmJZvUdcYnFoUYv2T4SK5VcB88MVuVVHrJe6rw"}`
    3. `stratum2+tcp://<proxy socket address>/ZZ6uJT6kaDRKmJZvUdcYnFoUYv2T4SK5VcB88MVuVVHrJe6rw`

## Running it directly
`cargo build` command generates binary file `./target/release/ii-stratum-proxy`.

This file can be run directly, e. g.

`./target/release/ii-stratum-proxy --conf config/insecure.toml`



# Future Work

Below is a high level list of areas that still need to be resolved:

- handle multiple channels on a single downstream connection
- use V2 submission sequence numbers for batch acknowledgement of valid job
  solutions
- improve logging
- resolve all TODO's in the sources
