#!/usr/bin/env bash
# Make a throwaway private CA and a server certificate for 127.0.0.1, for the
# local HTTPS publish fixture. Usage: make-test-ca.sh OUTDIR
set -euo pipefail
out="$1"
mkdir -p "$out"
cd "$out"
openssl req -x509 -newkey rsa:2048 -nodes -days 2 -sha256 \
  -keyout ca.key -out ca.pem -subj "/CN=rust-mirror-gui qualification test CA" \
  -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null
openssl req -newkey rsa:2048 -nodes -keyout server.key -out server.csr -subj "/CN=127.0.0.1" 2>/dev/null
printf 'basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n' > server.ext
openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial -days 2 -sha256 \
  -extfile server.ext -out server.pem 2>/dev/null
rm -f server.csr ca.srl
echo "test CA and server certificate written to $out"
