# ars-proxy [WIP]
Fast, minimal, asynchronous HTTP/HTTPS proxy server written in Rust

```bash
# Usage:
ars-proxy <local_port> <remote_url> <remote_port> [--cert <crt_path> --pass-file <pass_file_path>] [--to-https]
```
### Notes:

- If `--to-https` parameter is specified (useful only in HTTP mode), the server will proxy all the received http requests to HTTPS.

- If a TLS certificate path is specified (by `--cert` parameter), the server will listen on HTTPS only.
  - The only certificate format supported is .pfx/.p12, it is possible to create a .pfx certificate from .crt and .key files using openssl:
    ```bash
    openssl pkcs12 -export -out cert.pfx -inkey cert.key -in cert.crt
    ```
  - The password file (whose path can be specified by `--pass-file` parameter) is supposed to be a file containing only the certificate password (setting file permissions to 600 is recommended). This avoids specifying the certificate password in command line, that can be a security problem.
  - If argument `--pass-file` is not specified, the certificate password is assumed to be blank ("").

### Credits:

Credit for a working implementation of a (Tokio-based) Hyper HTTPS server goes to @izderadicka [[link]](https://github.com/izderadicka/tokio-tls/tree/new-tokio "[link]")