# libspatialite-sys

[![CI](https://github.com/jumboly/libspatialite-sys/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jumboly/libspatialite-sys/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/libspatialite-sys.svg)](https://crates.io/crates/libspatialite-sys)
[![docs.rs](https://docs.rs/libspatialite-sys/badge.svg)](https://docs.rs/libspatialite-sys)

[日本語版 README はこちら / Japanese README](README.ja.md)

Low-level FFI bindings and bundled build of [libspatialite][upstream] (the
SpatiaLite SQLite extension) for Rust.

[upstream]: https://www.gaia-gis.it/fossil/libspatialite/index

> **Status: pre-0.1, work in progress.**
> This crate is being extracted from a real-world consumer ([shpx]) that
> already ships libspatialite bundled on Linux / macOS. All three feature
> paths (default headers-only, `bundled`, `bundled-vcpkg`) are implemented
> and exercised by CI. The API surface and feature flag names may still
> change before 0.1.0 is published to crates.io.

[shpx]: https://github.com/jumboly/shpx

## What this crate provides

- A minimal `extern "C"` FFI surface to attach libspatialite to a SQLite
  connection (`spatialite_initialize` / `spatialite_alloc_connection` /
  `spatialite_init_ex`) — enough for static link consumers to bootstrap
  SpatiaLite SQL functions inside a `rusqlite::Connection` (or any other
  SQLite binding using `libsqlite3-sys`).
- An opt-in `bundled` Cargo feature that builds libspatialite, GEOS (via
  [`geos-src`]), PROJ (via [`proj-sys`]), and zlib (via [`libz-sys`]) from
  source and statically links them into your binary. Targets Linux and
  macOS.
- An opt-in `bundled-vcpkg` Cargo feature for Windows MSVC, which delegates
  the libspatialite + dependencies build to a user-provisioned vcpkg
  installation. This is the recommended Windows path because the upstream
  libspatialite source has known MSVC parser incompatibilities that
  vcpkg's port already patches.

[`geos-src`]: https://crates.io/crates/geos-src
[`proj-sys`]: https://crates.io/crates/proj-sys
[`libz-sys`]: https://crates.io/crates/libz-sys

This crate **does not** re-export the hundreds of SpatiaLite SQL functions
(`AsBinary`, `GeomFromWKB`, `ST_*`, `Transform`, etc.) as Rust functions.
Those are invoked through SQL after the connection has been initialized,
which is how libspatialite is conventionally used.

## Cargo features

| Feature | Default | Purpose |
|---|---|---|
| (none) | yes | Headers + FFI declarations only. Library resolution is left to the consumer. Useful for `rusqlite::Connection::load_extension("mod_spatialite")` users. |
| `bundled` | no | Build libspatialite from vendored source via `cc::Build`. Requires CMake (for transitive `geos-src` / `proj-sys`) and a C/C++ toolchain. **Linux & macOS only**; Windows MSVC has known build failures (use `bundled-vcpkg` instead). |
| `bundled-vcpkg` | no | Resolve libspatialite via vcpkg (`vcpkg install libspatialite:x64-windows-static-md`). Recommended for Windows MSVC. Requires `VCPKG_ROOT` to be set or vcpkg to be on `PATH`. |

## Platform support matrix

| Target | `default` (system) | `bundled` | `bundled-vcpkg` |
|---|---|---|---|
| `x86_64-unknown-linux-gnu` | ✅ pkg-config / `LD_LIBRARY_PATH` | ✅ tested in CI | (not applicable) |
| `aarch64-unknown-linux-gnu` | ✅ pkg-config / `LD_LIBRARY_PATH` | ✅ tested in CI | (not applicable) |
| `aarch64-apple-darwin` | ✅ Homebrew (`libspatialite`) | ✅ tested in CI | (not applicable) |
| `x86_64-apple-darwin` | ✅ Homebrew | ✅ tested in CI | (not applicable) |
| `x86_64-pc-windows-msvc` | ⚠️ user-provided `mod_spatialite.dll` | ❌ MSVC parser failures (use `bundled-vcpkg`) | ✅ tested in CI |
| `x86_64-pc-windows-gnu` | ⚠️ MSYS2 | ✅ (untested by CI) | (not applicable) |

## License

The crate metadata (Cargo.toml, build.rs, src/lib.rs, README.md, etc.) is
dual-licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

The bundled libspatialite source (under the `bundled` feature) is
**triple-licensed under MPL 1.1 / GPL 2.0 / LGPL 2.1** by the upstream
project. Distributors who statically link libspatialite into a binary need
to comply with one of these three licenses for the binary itself; see
[`NOTICE`](NOTICE) for details and obligations under each option.

The `bundled-vcpkg` path links to libspatialite built by your local vcpkg
installation; the same upstream license applies, but the vcpkg port itself
is MIT-licensed.

## Versioning

This crate follows semver. While in `0.0.x`, the FFI surface and feature
flags are subject to breaking changes between releases. Once 0.1.0 is
published, only the FFI surface, public Cargo features, and documented
environment variables are considered stable.

## Acknowledgments

- libspatialite by Alessandro Furieri and the SpatiaLite contributors.
- The build system is heavily informed by prior art in `proj-sys`,
  `geos-src`, and `libsqlite3-sys`.
