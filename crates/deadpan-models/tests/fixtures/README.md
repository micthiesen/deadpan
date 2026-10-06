# Test-only TLS fixtures

`test-ca.der`, `test-localhost.der` and `test-localhost.key.der` let
`src/packs/interrupted_download_tests.rs` serve HTTPS from `127.0.0.1`. They are test-only and
trusted nowhere else; the CA private key was discarded after signing. Generated
once on 2026-10-05 with `/usr/bin/openssl` (LibreSSL 3.3.6):

```sh
openssl ecparam -name prime256v1 -genkey -noout -out ca.key
openssl req -x509 -new -key ca.key -sha256 -days 36500 \
  -subj "/CN=Deadpan test CA (test only)" \
  -addext "basicConstraints=critical,CA:TRUE" \
  -addext "keyUsage=critical,keyCertSign,cRLSign" -out ca.pem
openssl ecparam -name prime256v1 -genkey -noout -out leaf.key
openssl req -new -key leaf.key -subj "/CN=localhost" -out leaf.csr
printf 'basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost,IP:127.0.0.1\n' > leaf.ext
openssl x509 -req -in leaf.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
  -sha256 -days 36500 -extfile leaf.ext -out leaf.pem
openssl x509 -in ca.pem -outform DER -out test-ca.der
openssl x509 -in leaf.pem -outform DER -out test-localhost.der
openssl pkcs8 -topk8 -nocrypt -in leaf.key -outform DER -out test-localhost.key.der
```
