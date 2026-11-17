These certificates and the private key are public test fixtures. Never use them to serve an environment.

OpenSSL 4.1 generated the P-256 CA and server certificate. Both certificates are valid from 2020-01-01 through 2120-01-01. The server certificate has the IP subject alternative name `127.0.0.1` and the `serverAuth` extended key usage. The CA private key was discarded.

The TLS tests trust this CA only in their own client configuration. They do not install it in an OS trust store. Tests connect directly to a loopback socket, including when verifying a mismatched hostname.
