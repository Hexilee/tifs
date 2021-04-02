#! /bin/bash
set +e

mkdir -p easyrsa
cd easyrsa
curl -L https://github.com/OpenVPN/easy-rsa/releases/download/v3.0.6/EasyRSA-unix-v3.0.6.tgz \
    | tar xzv --strip-components=1

./easyrsa init-pki \
    && ./easyrsa build-ca nopass

NUM_PD_NODES=1
for i in $(seq 1 $NUM_PD_NODES); do
    ./easyrsa gen-req pd$i nopass
    ./easyrsa sign-req server pd$i
done

NUM_TIKV_NODES=1
for i in $(seq 1 $NUM_TIKV_NODES); do
    ./easyrsa gen-req tikv$i nopass
    ./easyrsa sign-req server tikv$i
done

./easyrsa gen-req client nopass
./easyrsa sign-req server client
