//! Low-level FFI bindings to libspatialite (the SpatiaLite SQLite extension).
//!
//! This crate exposes only the small set of `extern "C"` functions needed to
//! attach libspatialite to a SQLite connection. The hundreds of SpatiaLite
//! SQL functions (`AsBinary`, `GeomFromWKB`, `ST_*`, `Transform`, ...) are
//! invoked through SQL after the connection has been initialized, so they do
//! not need direct Rust FFI declarations.
//!
//! ## Cargo features
//!
//! - default: declarations only. Library resolution and runtime loading are
//!   left to the consumer.
//! - `bundled`: compile the vendored libspatialite source with `cc::Build`
//!   and statically link it.
//! - `bundled-vcpkg`: resolve libspatialite from a vcpkg installation
//!   (recommended for Windows MSVC).
//!
//! ## Static link initialization sequence
//!
//! When libspatialite is linked statically, it is built without
//! `LOADABLE_EXTENSION`, so `sqlite3_modspatialite_init` is not available.
//! Callers initialize the connection with a three-step sequence instead:
//!
//! 1. `spatialite_initialize()` — once per process.
//! 2. `spatialite_alloc_connection()` — once per `sqlite3*` connection.
//! 3. `spatialite_init_ex(db, cache, verbose)` — registers the cache with
//!    the connection. The cache is freed automatically by libspatialite when
//!    `sqlite3_close` runs; do not call `spatialite_cleanup_ex` from Rust
//!    (it would double-free).
//!
//! ## Dynamic loading
//!
//! When `mod_spatialite.{so,dylib,dll}` is loaded via SQLite's
//! `load_extension` mechanism, SQLite calls `sqlite3_modspatialite_init`
//! itself. Most consumers go through `rusqlite::Connection::load_extension`
//! and never need to reference the FFI symbols below directly.

#![deny(unsafe_op_in_unsafe_fn)]

// The crates referenced below emit `cargo:rustc-link-lib=` directives via
// their build scripts, but those directives are only applied to the final
// binary when at least one Rust source file in this crate references the
// crate. We add inert `extern crate ... as _` lines so each transitive
// system library is forced to be linked.
//
// - `link_cplusplus`: pulls libstdc++ / libc++ for libgeos's C++ symbols.
// - `proj_sys`     : pulls libproj for libspatialite's PROJ_NEW path.
// - `libz_sys`     : pulls vendored zlib for `gg_relations.c`'s `crc32`.
//
// These are only needed for the source-build path (`bundled`); when
// `bundled-vcpkg` is on, vcpkg's port brings its own GEOS / PROJ / zlib
// and emits all the necessary link directives via the `vcpkg` crate's
// build.rs (called from our own build.rs). MSVC's linker pulls the C++
// runtime automatically, so `link-cplusplus` is unnecessary on the vcpkg
// path too.
//
// `libsqlite3_sys` is referenced from `tests/bundled_smoke.rs` and is
// always pulled regardless of feature, so it does not need its own
// extern-crate marker here.
#[cfg(feature = "bundled")]
extern crate link_cplusplus as _;
#[cfg(feature = "bundled")]
extern crate proj_sys as _;
#[cfg(feature = "bundled")]
extern crate libz_sys as _;

use std::os::raw::{c_int, c_void};

/// Opaque handle to a SQLite connection (`sqlite3 *`).
///
/// Layout-compatible with `libsqlite3_sys::sqlite3`. Consumers that already
/// depend on `libsqlite3-sys` can cast freely between the two pointer types.
/// We re-declare the type here to keep the crate self-contained for users
/// who interact with libspatialite through other SQLite bindings.
#[repr(C)]
pub struct sqlite3 {
    _private: [u8; 0],
}

extern "C" {
    /// Process-global libspatialite initialization. Sets up GEOS / PROJ
    /// global state. Call once per process; libspatialite guards against
    /// repeated invocations internally, but it is cleaner to gate it on
    /// the caller side (e.g. `std::sync::Once`).
    pub fn spatialite_initialize();

    /// Allocate a per-connection SpatiaLite cache.
    ///
    /// The returned pointer must be passed to `spatialite_init_ex` and
    /// otherwise treated as opaque. Returns `NULL` on allocation failure.
    pub fn spatialite_alloc_connection() -> *mut c_void;

    /// Register the per-connection cache with a SQLite connection.
    ///
    /// `verbose != 0` enables libspatialite's stderr initialization log.
    /// The cache is freed automatically by libspatialite when the
    /// underlying `sqlite3_close` runs; callers must not invoke
    /// `spatialite_cleanup_ex` themselves (doing so would double-free).
    pub fn spatialite_init_ex(db_handle: *mut sqlite3, p_cache: *const c_void, verbose: c_int);

    /// Loadable-extension entry point used by `sqlite3_load_extension`.
    ///
    /// This symbol is only available when libspatialite is built with
    /// `LOADABLE_EXTENSION` (the dynamic build path). Static builds do
    /// not export it. Most consumers should use
    /// `rusqlite::Connection::load_extension` rather than calling this
    /// function directly. The declaration is included for completeness.
    pub fn sqlite3_modspatialite_init(
        db: *mut sqlite3,
        pz_err_msg: *mut *mut std::os::raw::c_char,
        p_api: *const c_void,
    ) -> c_int;
}
