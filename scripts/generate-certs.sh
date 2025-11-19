#!/bin/bash
set -e

mkdir -p certs

echo "Generating CA..."
openssl req -x509 -newkey rsa:4096 -days 365 -nodes -keyout certs/ca-key.pem -out certs/ca-cert.pem -subj "/C=JP/ST=Tokyo/L=Tokyo/O=Capsuled/OU=Dev/CN=Capsuled CA"

echo "Generating Server Cert (for Engine)..."
openssl req -newkey rsa:4096 -nodes -keyout certs/server-key.pem -out certs/server-req.pem -subj "/C=JP/ST=Tokyo/L=Tokyo/O=Capsuled/OU=Engine/CN=localhost"
openssl x509 -req -in certs/server-req.pem -days 365 -CA certs/ca-cert.pem -CAkey certs/ca-key.pem -CAcreateserial -out certs/server-cert.pem -extfile <(printf "subjectAltName=DNS:localhost,IP:127.0.0.1")

echo "Generating Client Cert (for Client/Coordinator)..."
openssl req -newkey rsa:4096 -nodes -keyout certs/client-key.pem -out certs/client-req.pem -subj "/C=JP/ST=Tokyo/L=Tokyo/O=Capsuled/OU=Client/CN=client"
openssl x509 -req -in certs/client-req.pem -days 365 -CA certs/ca-cert.pem -CAkey certs/ca-key.pem -CAcreateserial -out certs/client-cert.pem

echo "Certificates generated in certs/"
