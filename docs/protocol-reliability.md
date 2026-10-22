# Protocol dependencies and connection recovery

## Build dependencies

The protocol uses checked-in copies of linejs 3.5.0, linejs-types 3.5.0,
loose-types 1.0.1, and the standard packages matching the original lockfile:
path 1.1.6, internal 1.0.14, assert 1.0.19. They come from the upstream GitHub
release sources, with upstream MIT licenses retained. `protocol/dependencies.json`
maps package exports to local files, including the ShopService previously
imported directly from a JSR URL. No application module is loaded from JSR.

`scripts/vendor-protocol.py --verify` checks the import map and all vendored
source hashes against `protocol/vendor/deps/manifest.json`. The build calls it
before compiling with a frozen Deno lockfile. npm dependencies and their
transitive dependencies remain in `protocol/deno.lock` with integrity hashes.

The first build needs npm access and Deno's compiler runtime. Subsequent builds
can use `DENO_OFFLINE=1 ./scripts/build-sidecar.sh` with the existing cache.
This removes the JSR/Cloudflare dependency; it does not make a fresh Rust/Deno
toolchain installation or an empty npm cache work without network access.

To change library versions, review and update the refs and archive SHA-256
values in `scripts/vendor-protocol.py`, the npm pins, and any affected local
call patches. Then run `./scripts/update-protocol.sh`, inspect the resulting
source/lockfile diff, run protocol tests, and rebuild the sidecar. The script
does not select a floating `latest` version. Already downloaded archives can
be supplied through `--archives DIRECTORY`.

## Packaged sidecar

Keep `!strip` in both AUR PKGBUILDs. Rust's release profile strips only the
Rust executable, while makepkg must leave Deno's appended payload intact.
The build smoke-tests the executable; `packaging/check-package.sh` also tests
the extracted final archive. Run that checker after makepkg, since package
post-processing happens after `package()` returns.

## LINE connection failures

Every LINE HTTP exchange has a deadline. QR authentication permits a longer
exchange for phone confirmation. Only session restoration is retried: three
retries after 1, 2, and 4 seconds, for network failures and HTTP 408/429/5xx.
Writes such as sending messages are never automatically replayed, since a
timeout does not prove the server failed to accept the original message.

Saved tokens are cleared only for explicit LINE authentication errors. A
network outage, HTTP 403, unsupported API, or server failure preserves the
session. The UI distinguishes connection/server/protocol failures and offers
Retry through the existing recovery action, which preserves credentials.
An actual expired session still requires QR login.

Boot restoration runs without blocking protocol stdin. A new login/logout
cancels its network requests so a late restoration cannot replace the new
session. Detached linejs polling failures are classified too: transient errors
trigger bounded process recovery, and authentication failures request login.
Repeated errors cannot schedule duplicate recoveries. The recovery budget only
resets after the restored process has remained alive for 30 seconds.

These measures handle client recovery and packaging. They cannot prevent LINE
outages, remove account restrictions/rate limits, or guarantee compatibility
after LINE changes its protocol. Unknown errors remain visible in the log and
may require a reviewed linejs update.
