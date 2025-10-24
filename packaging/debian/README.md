# Harmoniq Studio Debian packaging

This directory contains helper scripts for producing native Debian packages of
Harmoniq Studio. The generated `.deb` installs the desktop application binary,
launcher, and icon under the conventional system paths.

## Build prerequisites

- `cargo`
- `jq`
- `dpkg-deb`
- `dpkg-shlibdeps` (optional, recommended for automatic dependency discovery)

## Building

```bash
./packaging/debian/build-deb.sh
```

The script builds `harmoniq-app` in release mode, stages the runtime assets, and
creates a versioned package under `dist/`. The architecture is detected via
`dpkg-architecture`/`dpkg --print-architecture` with a fallback to `uname -m`.
Set the `DEB_ARCH` environment variable to override the detected value when
cross-compiling.

Dependency metadata is populated automatically when `dpkg-shlibdeps` is
available. If it is not installed the package is still produced, but you may
need to manually verify runtime library requirements on the target system.
