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

## Usage

### Quickstart with rusqlite (recommended)

Most consumers want a single Rust binary that ships with libspatialite
statically linked. Pair this crate with [`rusqlite`] using its `bundled`
feature; both crates share a single `libsqlite3-sys` instance via
Cargo's `links = "sqlite3"` deduplication, so only one copy of SQLite
ends up in your binary.

[`rusqlite`]: https://crates.io/crates/rusqlite

**Cargo.toml**

```toml
[dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }

[target.'cfg(not(target_os = "windows"))'.dependencies]
libspatialite-sys = { version = "0.0.1", features = ["bundled"] }

[target.'cfg(target_os = "windows")'.dependencies]
libspatialite-sys = { version = "0.0.1", features = ["bundled-vcpkg"] }
```

Pin `rusqlite` to a version that depends on the same `libsqlite3-sys`
major as this crate (currently `0.28`). Mismatched majors trigger a
duplicate `links = "sqlite3"` error at build time — a deliberate
safeguard against linking two SQLite copies into one binary.

**Initialize a connection**

When libspatialite is statically linked, the loadable-extension entry
point is unavailable. Run the three-step bootstrap below, with
`spatialite_initialize` guarded by `Once` so it fires only once per
process:

```rust
use rusqlite::Connection;
use libspatialite_sys::{
    spatialite_alloc_connection, spatialite_init_ex, spatialite_initialize,
};
use std::sync::Once;

static GLOBAL_INIT: Once = Once::new();

fn open_with_spatialite() -> rusqlite::Result<Connection> {
    GLOBAL_INIT.call_once(|| unsafe { spatialite_initialize() });

    let conn = Connection::open_in_memory()?;
    let raw = unsafe { conn.handle() }; // *mut libsqlite3_sys::sqlite3
    let cache = unsafe { spatialite_alloc_connection() };
    // libspatialite_sys::sqlite3 is layout-compatible with
    // libsqlite3_sys::sqlite3 — both are opaque ZSTs.
    unsafe {
        spatialite_init_ex(raw.cast::<libspatialite_sys::sqlite3>(), cache, 0);
    }
    Ok(conn)
}

fn main() -> rusqlite::Result<()> {
    let conn = open_with_spatialite()?;
    conn.execute("SELECT InitSpatialMetadata(1)", [])?;
    let v: String =
        conn.query_row("SELECT spatialite_version()", [], |r| r.get(0))?;
    println!("SpatiaLite {v}");
    Ok(())
}
```

> **Do not call `spatialite_cleanup_ex`.** libspatialite frees the
> per-connection cache automatically when the underlying `sqlite3_close`
> runs; calling cleanup explicitly would double-free.

### Dynamic loading (no `bundled*` feature)

If you would rather distribute `mod_spatialite.{so,dylib,dll}`
separately and load it at runtime, depend on this crate with **no
features**:

```toml
[dependencies]
libspatialite-sys = "0.0.1"
rusqlite = { version = "0.31", features = ["bundled", "load_extension"] }
```

In this mode the crate's only role is to expose FFI symbol declarations
for users who need them; the actual load is done by rusqlite (or
whichever SQLite binding you use):

```rust
unsafe { conn.load_extension_enable()? };
unsafe { conn.load_extension("mod_spatialite", None)? };
unsafe { conn.load_extension_disable()? };
```

This requires `mod_spatialite` to be present on the user's machine
(Homebrew on macOS, `libsqlite3-mod-spatialite` on Debian / Ubuntu,
vcpkg on Windows, etc.) and gives up the single-binary distribution
benefit.

### Local Windows development (`bundled-vcpkg`)

The Windows path requires a vcpkg installation with libspatialite built
under the `x64-windows-static-md` triplet. Use **that exact triplet** —
it matches the default `/MD` (dynamic UCRT) ABI cargo-dist ships with;
mixing in `x64-windows-static` (`/MT`) creates runtime ABI mismatches.

**One-time setup**

```pwsh
git clone https://github.com/microsoft/vcpkg C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat -disableMetrics

# Compiles libspatialite + GEOS + PROJ + sqlite3 + zlib from source.
# A cold install takes ~20 minutes; subsequent rebuilds reuse the
# cache under C:\vcpkg\installed\.
C:\vcpkg\vcpkg.exe install libspatialite:x64-windows-static-md
```

**Per-shell environment**

```pwsh
$env:VCPKG_ROOT = "C:\vcpkg"
$env:VCPKGRS_TRIPLET = "x64-windows-static-md"
```

To avoid setting `VCPKGRS_TRIPLET` every time, drop a
`.cargo/config.toml` into your repo:

```toml
[env]
VCPKGRS_TRIPLET = "x64-windows-static-md"
```

Then `cargo build --release --features bundled-vcpkg` produces a
self-contained `.exe` with no DLL dependencies (libspatialite, GEOS,
PROJ, SQLite, and zlib are all linked statically into the binary).

### CI integration

GitHub Actions Windows runners ship with a vcpkg checkout under
`C:\vcpkg`, but a cold `vcpkg install libspatialite` takes ~20 minutes
and the default `actions/cache` archive churn (7-day idle eviction,
all-or-nothing tarball) makes it painful in practice. We maintain a
companion Marketplace action,
[`jumboly/setup-vcpkg-nuget-cache`][nugetcache], that caches each
vcpkg port as a NuGet package on GitHub Packages, so the second-and-
later runs install pre-built artifacts:

[nugetcache]: https://github.com/marketplace/actions/setup-vcpkg-nuget-cache

```yaml
jobs:
  build:
    runs-on: windows-latest
    permissions:
      contents: read
      packages: write
    env:
      VCPKGRS_TRIPLET: x64-windows-static-md
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: jumboly/setup-vcpkg-nuget-cache@v1
        with:
          ports: libspatialite
          triplet: ${{ env.VCPKGRS_TRIPLET }}
          token: ${{ secrets.GITHUB_TOKEN }}
      - run: cargo build --release --features bundled-vcpkg
        env:
          VCPKG_ROOT: C:\vcpkg
```

Linux and macOS jobs need only CMake (for the transitive `geos-src` /
`proj-sys` builds) and a working C/C++ toolchain:

```yaml
- run: sudo apt-get install -y cmake   # ubuntu
- run: brew install cmake              # macos
- run: cargo build --release --features bundled
```

See [`.github/workflows/ci.yml`](.github/workflows/ci.yml) for the
in-repo reference matrix that covers all three OSes.

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
