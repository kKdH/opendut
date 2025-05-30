#!/bin/bash
set -e
set -x

PROVISION_ROOT_DIR="/provision/"
PROVISION_PKI_DIR="$PROVISION_ROOT_DIR/pki/"
OPENDUT_PASSWORD_FILE="$PROVISION_ROOT_DIR/.env-pki"
OPENDUT_ENV_FILE="$PROVISION_ROOT_DIR/.env"
CA_PATH="$PROVISION_PKI_DIR/opendut-ca"

SERVERNAME="$1"
CERT_PATH="$PROVISION_PKI_DIR/$SERVERNAME"
mkdir -p "$PROVISION_PKI_DIR/deploy"
CERT_DEPLOY_PATH="$PROVISION_PKI_DIR/deploy/$SERVERNAME"

if [ ! -e "$OPENDUT_PASSWORD_FILE" ]; then
  echo "Password file $OPENDUT_PASSWORD_FILE missing. You may override the environment variable OPENDUT_PASSWORD_FILE."
  exit 1
fi

if [ -z "$SERVERNAME" ]; then
  echo "Servername missing"
  echo "$0 <FQDN>"
  exit 1
fi

# certificate signing request
openssl req -new -sha512 -passout file:"$OPENDUT_PASSWORD_FILE" -out "$CERT_PATH".csr -newkey rsa:4096 -keyout "$CERT_PATH".key -subj "/CN=$SERVERNAME/C=XX/ST=Some-State/O=ExampleOrg"


# create a v3 ext file for SAN properties
cat > "$CERT_PATH".v3.ext << EOF
authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names
[alt_names]
DNS.1 = $SERVERNAME
EOF


# CARL certificate signing
openssl x509 -req -in "$CERT_PATH".csr -CA "$CA_PATH".pem -CAkey "$CA_PATH".key -passin file:"$OPENDUT_PASSWORD_FILE" -CAcreateserial -outform PEM -out "$CERT_PATH".pem -days 9999 -sha256 -extfile "$CERT_PATH".v3.ext


cp "$CERT_PATH".pem "$CERT_DEPLOY_PATH".pem
openssl rsa -in "$CERT_PATH".key -passin file:"$OPENDUT_PASSWORD_FILE" -out "$CERT_DEPLOY_PATH".key

rm "$CERT_PATH".csr
rm "$CERT_PATH".v3.ext

