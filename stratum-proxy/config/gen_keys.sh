#!/bin/bash
if [ "${PWD##*/}" == "config" ]; then
	KPATH="../../protocols/stratum"
	TARGET="${PWD}"
else
	KPATH="../protocols/stratum"
	TARGET="${PWD}/config"
fi
KBIN=ii-stratum-keytool
cd "$KPATH"
cargo build --release --bin $KBIN
cp target/release/$KBIN $TARGET
cd $TARGET
./$KBIN gen-ca-key
./$KBIN gen-noise-key
./$KBIN sign-key --public-key-to-sign server-noise-static-public.key --signing-key ca-ed25519-secret.key
rm $KBIN
