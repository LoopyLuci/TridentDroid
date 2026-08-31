#!/usr/bin/env bash
# Generate CA, server, and client certificates for TridentDroid mTLS.
# Run once before first launch: ./tools/gen_certs.sh
set -euo pipefail

CERTS_DIR="$(dirname "$0")/../certs"
mkdir -p "$CERTS_DIR"
cd "$CERTS_DIR"

DAYS=3650
SUBJ_CA="/CN=TridentDroid CA/O=TridentDroid/C=US"
SUBJ_SRV="/CN=tridentd/O=TridentDroid/C=US"
SUBJ_CLI="/CN=trident-client/O=TridentDroid/C=US"

echo "==> Generating CA key and certificate"
openssl genrsa -out ca.key 4096
openssl req -new -x509 -days "$DAYS" -key ca.key -out ca.crt -subj "$SUBJ_CA"

echo "==> Generating server key and CSR"
openssl genrsa -out server.key 4096
openssl req -new -key server.key -out server.csr -subj "$SUBJ_SRV"
openssl x509 -req -days "$DAYS" -in server.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out server.crt \
    -extfile <(echo "subjectAltName=IP:127.0.0.1,IP:::1")

echo "==> Generating client key and CSR"
openssl genrsa -out client.key 4096
openssl req -new -key client.key -out client.csr -subj "$SUBJ_CLI"
openssl x509 -req -days "$DAYS" -in client.csr -CA ca.crt -CAkey ca.key \
    -CAcreateserial -out client.crt

echo "==> Certificates written to $CERTS_DIR/"
echo "    ca.crt  server.crt  server.key  client.crt  client.key"
