# Connect through a remote link

Run Ferrite Server on the computer that owns your repositories and agent processes. Use a certificate for the hostname clients connect to. The gateway keeps the existing Server running and forwards authenticated connections to its owner-only local socket.

## Start the gateway

Run this command on the remote computer. Replace the hostname and certificate paths with your own.

```sh
ferrited remote serve \
  --listen 0.0.0.0:7443 \
  --public-url https://dev.example.com:7443 \
  --cert /etc/ferrite/fullchain.pem \
  --key /etc/ferrite/privkey.pem
```

Keep the gateway running in your service manager. `FERRITE_STORE` selects the Server store for both the gateway and the administration commands. The default is `~/.ferrite/threads`.

The desktop verifies the certificate and hostname. There is no option to skip certificate verification. A bare hostname in the Add environment form uses HTTPS.

## Pair the desktop

In another shell on the remote computer, issue a code.

```sh
ferrited remote pairing create
```

The JSON response contains `url`, `code`, `server_id`, and `expires_in`. The URL places the code in its fragment. The code expires after ten minutes and works once.

Open **Settings > Connections > Add** in Ferrite. Select **Remote link**, paste the URL into **Host**, and select **Add**. You can also enter the public host and code separately.

Ferrite checks the environment identity, exchanges the code for a separate session secret, and verifies a real Server welcome and initial state. It saves the environment only after verification. Closing the form discards an unfinished registration. An existing environment reconnects with its saved session credential.

The desktop stores the session secret in the OS credential store. The store must be available and unlocked. Linux uses Secret Service together with its native keyring. Credentials are never stored in `settings.json` or terminal command arguments. Saved link profiles contain only the origin, server identity, and credential reference.

## Revoke access

List active sessions on the remote computer.

```sh
ferrited remote sessions list
```

Revoke the session id from that output.

```sh
ferrited remote sessions revoke SESSION_ID
```

Revocation rejects new connections and closes active links and their terminals. Already forwarded Server operations can finish. Existing agent processes keep running. Sessions also expire after ninety days. A revoked or expired desktop session requires a fresh pairing code.

Removing a connection from the desktop removes its local registration and stored credential. It does not stop remote agents. Use the remote revocation command when you also want the host to invalidate that session.

## Run a loopback fixture

Plain HTTP requires an explicit loopback IP address and the gateway flag below. It is intended for local test fixtures. Remote addresses always require HTTPS.

```sh
FERRITE_STORE=/tmp/ferrite-remote-fixture/threads ferrited remote serve \
  --listen 127.0.0.1:7443 \
  --public-url http://127.0.0.1:7443 \
  --allow-loopback-http
```

Use the same `FERRITE_STORE` when issuing or revoking fixture credentials. Client settings and the remote store must use separate directories to prove that files, Changes, and terminal commands execute remotely.

## Terminal behavior

A Remote link terminal opens a PTY on the remote computer in the selected thread directory. Input, output, and resize messages use the authenticated connection. Closing the terminal or losing authorization closes its PTY and shell. Terminal sessions do not resume after a disconnect. Existing SSH connections keep their SSH terminal transport.

## Transport limits

The gateway accepts up to 32 active links and terminals, and up to 64 concurrent HTTP requests. Pairing request bodies are limited to 1 KiB, responses to 16 KiB, and the global pairing budget to 20 attempts per minute. A store holds up to 16 active pairing grants and 64 unexpired sessions. WebSocket frames are limited to 256 KiB. Protocol lines retain the existing 64 MiB limit.

Access administration uses an owner-only local socket. The host persists hashes of pairing and session credentials. Its identity is the Server's own stable random id, kept in `server-id` in the Server store (a store paired before that file existed keeps the identity it was paired with). Copying `server-id` copies the environment identity, and copying `remote-access.json` copies its active session hashes. Create a separate store for a separate environment.
