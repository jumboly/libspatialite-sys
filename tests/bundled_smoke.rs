//! End-to-end smoke test for the `bundled` feature.
//!
//! Runs only when libspatialite-sys is compiled with `--features bundled`.
//! Exercises the full static-link initialization sequence
//! (`spatialite_initialize` -> `spatialite_alloc_connection` -> `spatialite_init_ex`)
//! against a freshly-opened in-memory SQLite database, and runs two SQL
//! statements that depend on libspatialite functionality being live in the
//! connection:
//!
//! - `InitSpatialMetadata(1)` populates the SpatiaLite metadata tables. It
//!   only succeeds if the extension was attached correctly.
//! - `SELECT spatialite_version()` returns the linked libspatialite version
//!   and confirms SpatiaLite SQL functions are callable.
//!
//! Together these two checks catch the most common bundled regressions:
//! missing symbols at link time, init function pointer mismatches, and
//! GEOS / PROJ being absent or version-incompatible.

#![cfg(feature = "bundled")]

use std::ffi::{c_char, CStr};
use std::ptr;
use std::sync::Once;

// Reference the lib's FFI symbols rather than re-declaring them locally,
// so that Cargo links this integration test against `libspatialite-sys`'s
// rlib and inherits the `cargo:rustc-link-lib=static=spatialite` directive
// emitted by our build.rs. A second local `extern "C"` block would not
// pull the rlib in.
use libspatialite_sys::{spatialite_alloc_connection, spatialite_init_ex, spatialite_initialize};

use libsqlite3_sys::{
    sqlite3, sqlite3_close, sqlite3_column_text, sqlite3_exec, sqlite3_finalize, sqlite3_open_v2,
    sqlite3_prepare_v2, sqlite3_step, SQLITE_DONE, SQLITE_OK, SQLITE_OPEN_CREATE,
    SQLITE_OPEN_MEMORY, SQLITE_OPEN_READWRITE, SQLITE_ROW,
};

static GLOBAL_INIT: Once = Once::new();

/// Open an in-memory SQLite database and attach libspatialite via the
/// static-link initialization sequence. The caller must `sqlite3_close`
/// the returned handle.
fn open_with_spatialite() -> *mut sqlite3 {
    GLOBAL_INIT.call_once(|| unsafe { spatialite_initialize() });

    let mut db: *mut sqlite3 = ptr::null_mut();
    let uri = c":memory:";
    let rc = unsafe {
        sqlite3_open_v2(
            uri.as_ptr(),
            &mut db,
            SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_MEMORY,
            ptr::null(),
        )
    };
    assert_eq!(rc, SQLITE_OK, "sqlite3_open_v2 failed: rc={rc}");
    assert!(!db.is_null(), "sqlite3_open_v2 returned null handle");

    let cache = unsafe { spatialite_alloc_connection() };
    assert!(
        !cache.is_null(),
        "spatialite_alloc_connection returned null"
    );
    // The lib re-exports `spatialite_init_ex` with its own `sqlite3` opaque
    // type. We cast to/from `libsqlite3_sys::sqlite3` because the two
    // pointers are layout-compatible (libspatialite-sys re-declares the
    // type only to keep itself self-contained; see src/lib.rs).
    unsafe {
        spatialite_init_ex(
            db.cast::<libspatialite_sys::sqlite3>(),
            cache,
            0,
        );
    }

    db
}

fn exec(db: *mut sqlite3, sql: &str) {
    let cstr = std::ffi::CString::new(sql).unwrap();
    let mut errmsg: *mut c_char = ptr::null_mut();
    let rc = unsafe { sqlite3_exec(db, cstr.as_ptr(), None, ptr::null_mut(), &mut errmsg) };
    if rc != SQLITE_OK {
        let msg = if errmsg.is_null() {
            "(no error message)".to_string()
        } else {
            unsafe { CStr::from_ptr(errmsg) }
                .to_string_lossy()
                .into_owned()
        };
        panic!("sqlite3_exec({sql:?}) failed: rc={rc}, msg={msg}");
    }
}

#[test]
fn init_spatial_metadata_succeeds() {
    let db = open_with_spatialite();
    // FastInit (= 1) seeds only the WGS84 SRS, which matches our vendored
    // EPSG subset (OMIT_EPSG drops the full table; only WGS84 + extra
    // dispatcher are kept).
    exec(db, "SELECT InitSpatialMetadata(1)");
    unsafe { sqlite3_close(db) };
}

#[test]
fn spatialite_version_returns_string() {
    let db = open_with_spatialite();
    let sql = c"SELECT spatialite_version()";
    let mut stmt = ptr::null_mut();
    let rc = unsafe { sqlite3_prepare_v2(db, sql.as_ptr(), -1, &mut stmt, ptr::null_mut()) };
    assert_eq!(rc, SQLITE_OK, "prepare failed: rc={rc}");

    let rc = unsafe { sqlite3_step(stmt) };
    assert_eq!(rc, SQLITE_ROW, "step expected ROW: rc={rc}");

    let text = unsafe {
        let p = sqlite3_column_text(stmt, 0);
        assert!(!p.is_null(), "spatialite_version() returned null text");
        CStr::from_ptr(p as *const c_char)
            .to_string_lossy()
            .into_owned()
    };
    // Vendored libspatialite is 5.1.0; bump this prefix when the pinned
    // upstream version moves to 6.x.
    assert!(
        text.starts_with("5."),
        "unexpected spatialite_version() output: {text:?}"
    );

    let rc = unsafe { sqlite3_finalize(stmt) };
    assert_eq!(rc, SQLITE_OK);

    // Suppress "unused" warning for SQLITE_DONE which we re-export above
    // for documentation purposes (some callers may want to compare against
    // it when extending these tests).
    let _ = SQLITE_DONE;

    unsafe { sqlite3_close(db) };
}
