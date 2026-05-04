//! Build script for libspatialite-sys.
//!
//! Behavior depends on which Cargo feature is active:
//!
//! - default (no feature): emits no link directives. Consumers are expected to
//!   load libspatialite at runtime themselves (e.g. via SQLite's
//!   `load_extension`).
//! - `bundled`: compiles vendored libspatialite source via `cc::Build` and
//!   statically links it together with GEOS (via `geos-src`), PROJ (via
//!   `proj-sys`), and zlib (via `libz-sys`). Tested on Linux and macOS.
//! - `bundled-vcpkg` (added in a follow-up commit): resolves libspatialite
//!   from a vcpkg installation; recommended for Windows MSVC.
//!
//! ## Header path resolution
//!
//! - `sqlite3.h` / `sqlite3ext.h`: from `libsqlite3-sys` (which declares
//!   `links = "sqlite3"` and exposes `DEP_SQLITE3_INCLUDE`).
//! - `geos_c.h` / `proj.h`: neither `geos-src` 0.2 nor `proj-sys` 0.25 emit a
//!   `cargo:include=` directive, so we cannot use Cargo's `DEP_*_INCLUDE`
//!   mechanism. Instead, this build script scans sibling build directories
//!   (`<build_root>/geos-src-*/out/include/...`) at compile time. See
//!   `locate_sibling_out` for details. If those upstream crates eventually
//!   start emitting `cargo:include=`, the helpers can be replaced with
//!   `std::env::var("DEP_GEOS_INCLUDE")` / `DEP_PROJ_INCLUDE`.
//!
//! ## Link emission policy
//!
//! - GEOS (`libgeos_c`, `libgeos`): emitted by this script (`cargo:rustc-link-lib=static=geos_c`
//!   followed by `static=geos`; `geos-src` 0.2 does not emit them itself).
//! - PROJ (`libproj`): NOT emitted by this script. `proj-sys` declares
//!   `links = "proj"` and emits the canonical link directive on its own;
//!   duplicating it would surface as `cargo:rustc-link-lib=proj` twice and
//!   could surprise users.
//! - zlib (`libz`): emitted by this script (`static=z`). `libz-sys`'s
//!   `static` feature builds vendored zlib but does not emit a link directive
//!   on its own when the consumer (us) does not explicitly reference any
//!   zlib symbol from Rust — and we don't, since libspatialite calls
//!   `crc32` purely from C.

#[cfg(not(feature = "bundled"))]
fn main() {
    // Re-run only when this script changes; vendored source edits are
    // irrelevant in the no-feature build path.
    println!("cargo:rerun-if-changed=build.rs");
}

#[cfg(feature = "bundled")]
fn main() {
    use std::path::PathBuf;

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src_dir = manifest_dir.join("vendor/libspatialite-5.1.0/src");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let sqlite_include = std::env::var("DEP_SQLITE3_INCLUDE")
        .expect("DEP_SQLITE3_INCLUDE not set; libsqlite3-sys with `bundled` must be a direct dep");

    // Discover the geos-src OUT_DIR so we can feed `geos_c.h` to cc::Build
    // and emit `rustc-link-search` for `libgeos*.a`.
    let geos_root = locate_geos_root(&out_dir);
    println!(
        "cargo:rustc-link-search=native={}",
        geos_root.join("lib").display()
    );
    // Order matters: geos_c depends on geos.
    println!("cargo:rustc-link-lib=static=geos_c");
    println!("cargo:rustc-link-lib=static=geos");
    // libspatialite's `gg_relations.c::evalGeosCache` calls zlib's `crc32`.
    // libz-sys with the `static` feature builds vendored zlib and exposes
    // its include path via `DEP_Z_INCLUDE`, but it doesn't emit a link
    // directive (we don't reference zlib from Rust, only from libspatialite C).
    println!("cargo:rustc-link-lib=static=z");

    // proj-sys handles the PROJ link itself via its `links = "proj"`
    // declaration. We only need the include path here, which we discover
    // from the sibling OUT_DIR (`proj-sys` 0.25 does not emit
    // `cargo:include=`). When proj-sys eventually does emit it, replace
    // this with `std::env::var("DEP_PROJ_INCLUDE")`.
    let proj_root = locate_proj_root(&out_dir);

    write_generated_headers(&out_dir);

    let mut build = cc::Build::new();
    build
        // Put OUT_DIR first so our generated `gaiaconfig.h` overrides the
        // upstream-baked `vendor/.../headers/spatialite/gaiaconfig.h`
        // (which has different ENABLE_* / OMIT_* defaults).
        .include(&out_dir)
        .include(&sqlite_include)
        .include(geos_root.join("include"))
        .include(proj_root.join("include"))
        .include(src_dir.join("headers"))
        .define("VERSION", "\"5.1.0\"");

    // Receive zlib's include path via `DEP_Z_INCLUDE` (libz-sys declares
    // `links = "z"`). `spatialite_private.h` includes `<zlib.h>`. On
    // Linux/macOS the system zlib header is also available, but always
    // preferring libz-sys keeps the zlib version pinned by Cargo.lock for
    // reproducibility across the 3 OS targets.
    if let Ok(zlib_include) = std::env::var("DEP_Z_INCLUDE") {
        build.include(zlib_include);
    }

    build
        // PROJ_NEW=1 must be passed on the cc command line, not just in
        // gaiaconfig.h. Some .c files (e.g. srid_aux.c) evaluate
        // `#ifdef PROJ_NEW ... #include <proj.h> #else #include <proj_api.h>`
        // before they include `<spatialite/gaiaconfig.h>`. proj_api.h was
        // removed in PROJ 8+, so the build fails without this flag.
        .define("PROJ_NEW", "1")
        // MSVC: flex-generated `lex.*.c` files (included by gg_*.c) call
        // `#include <unistd.h>` unconditionally. Defining YY_NO_UNISTD_H
        // skips that include. POSIX systems have unistd.h anyway, so the
        // define is a no-op on Linux/macOS.
        .define("YY_NO_UNISTD_H", "1")
        .warnings(false);
    for omit in OMIT_FEATURES {
        build.define(&format!("OMIT_{omit}"), None);
    }
    for flag in [
        "-Wno-unused-parameter",
        "-Wno-unused-variable",
        "-Wno-unused-function",
        "-Wno-unused-but-set-variable",
        "-Wno-sign-compare",
        "-Wno-implicit-function-declaration",
        "-Wno-deprecated-declarations",
        "-Wno-format",
        "-Wno-pointer-sign",
    ] {
        build.flag_if_supported(flag);
    }

    // `spatialite.c` (the extension entry point) unconditionally references
    // functions defined in dxf / stored_procedures / gaiaexif / cutter /
    // control_points / shapefiles / virtualtext / topology / wfs / geopackage.
    // We can't drop those subdirs from the link tree even when the
    // corresponding feature is OMIT-ed; the OMIT_* defines stub out their
    // GEOS/PROJ-touching internals instead. So compile every .c file in
    // every relevant subdir.
    let subdirs: &[&str] = &[
        "spatialite",
        "gaiageo",
        "gaiaaux",
        "gaiaexif",
        "srsinit",
        "connection_cache",
        "versioninfo",
        "md5",
        "shapefiles",
        "dxf",
        "stored_procedures",
        "cutter",
        "control_points",
        "virtualtext",
        "topology",
        "wfs",
        "geopackage",
    ];
    for sub in subdirs {
        let dir = src_dir.join(sub);
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read_dir {} failed: {e}", dir.display()))
        {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("c") {
                continue;
            }
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if EXCLUDED_C_FILES.contains(&fname) {
                continue;
            }
            build.file(&path);
        }
        // Per-file `rerun-if-changed` would mean 100+ stat()s every build.
        // Subdir-level granularity is enough: Cargo also tracks dir mtime,
        // which catches added/removed/modified .c/.h files inside.
        println!("cargo:rerun-if-changed={}", dir.display());
    }
    println!(
        "cargo:rerun-if-changed={}",
        src_dir.join("headers").display()
    );
    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("vendor/SHA256SUMS").display()
    );

    build.compile("spatialite");
}

/// Locate `<build_root>/geos-src-<hash>/out`, the cmake install prefix
/// produced by the `geos-src` build script.
///
/// Our own OUT_DIR is `<build_root>/libspatialite-sys-<hash>/out`. The
/// sibling directory contains `out/include/geos_c.h` and
/// `out/lib/libgeos_c.a`. If multiple `geos-src-*` directories exist we
/// pick the most recently modified one.
#[cfg(feature = "bundled")]
fn locate_geos_root(out_dir: &std::path::Path) -> std::path::PathBuf {
    locate_sibling_out(out_dir, "geos-src-", "include/geos_c.h").unwrap_or_else(|build_root| {
        panic!(
            "could not locate geos-src OUT_DIR (expected {}/geos-src-*/out/include/geos_c.h). \
             Make sure `geos-src` is declared as a build-dependency.",
            build_root.display()
        )
    })
}

/// Locate `<build_root>/proj-sys-<hash>/out`.
///
/// Same shape as `locate_geos_root`. proj-sys 0.25 does not emit
/// `cargo:include=` / `cargo:root=`, so we cannot receive the include path
/// via `DEP_PROJ_INCLUDE`. With the `bundled_proj` feature on, proj-sys
/// installs `libproj.a` and `proj.h` into its OUT_DIR via cmake.
#[cfg(feature = "bundled")]
fn locate_proj_root(out_dir: &std::path::Path) -> std::path::PathBuf {
    locate_sibling_out(out_dir, "proj-sys-", "include/proj.h").unwrap_or_else(|build_root| {
        panic!(
            "could not locate proj-sys OUT_DIR (expected {}/proj-sys-*/out/include/proj.h). \
             Make sure the `bundled` feature pulls `proj-sys` with `bundled_proj` on.",
            build_root.display()
        )
    })
}

/// Find a sibling build directory shaped `<prefix><hash>/out/<sentinel>`
/// and return the path up to `out/`. If multiple candidates exist we pick
/// the most recently modified one. On total failure we return the search
/// root in `Err` so the caller can format a useful panic message.
///
/// Cross-target invocation (e.g. `cargo build --target=<triple>` used by
/// cargo-dist) places `[build-dependencies]` OUT_DIRs in the host build
/// dir (`target/<profile>/build/`) while we run in the target build dir
/// (`target/<triple>/<profile>/build/`). So the primary scan and the
/// host-fallback scan together cover both layouts.
#[cfg(feature = "bundled")]
fn locate_sibling_out(
    out_dir: &std::path::Path,
    prefix: &str,
    sentinel_rel: &str,
) -> std::result::Result<std::path::PathBuf, std::path::PathBuf> {
    let primary_root = out_dir
        .parent()
        .and_then(std::path::Path::parent)
        .expect("OUT_DIR has no <build_root> ancestor");

    if let Some(found) = scan_build_root_for_sibling(primary_root, prefix, sentinel_rel) {
        return Ok(found);
    }

    if let Some(host_root) = host_build_root_from_target_out_dir(out_dir) {
        if host_root != primary_root {
            if let Some(found) = scan_build_root_for_sibling(&host_root, prefix, sentinel_rel) {
                return Ok(found);
            }
        }
    }

    Err(primary_root.to_path_buf())
}

/// Scan a single `build/` directory for `<prefix>...` and return the most
/// recently modified `out/`.
#[cfg(feature = "bundled")]
fn scan_build_root_for_sibling(
    build_root: &std::path::Path,
    prefix: &str,
    sentinel_rel: &str,
) -> Option<std::path::PathBuf> {
    let mut best: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    let entries = std::fs::read_dir(build_root).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with(prefix) {
            continue;
        }
        let root = entry.path().join("out");
        if !root.join(sentinel_rel).exists() {
            continue;
        }
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        if best.as_ref().is_none_or(|(t, _)| mtime > *t) {
            best = Some((mtime, root));
        }
    }
    best.map(|(_, p)| p)
}

/// From an OUT_DIR shaped `target/<triple>/<profile>/build/<crate>/out`,
/// derive the host build dir `target/<profile>/build/`. Returns `None` when
/// `--target` is not in use (primary == host).
#[cfg(feature = "bundled")]
fn host_build_root_from_target_out_dir(out_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut components: Vec<_> = out_dir.components().collect();
    let build_pos = components.iter().rposition(|c| c.as_os_str() == "build")?;
    if build_pos < 3 {
        return None;
    }
    let profile = components[build_pos - 1];
    if components[build_pos - 3].as_os_str() != "target" {
        return None;
    }
    let mut host_root = std::path::PathBuf::new();
    for c in components.drain(..build_pos - 2) {
        host_root.push(c.as_os_str());
    }
    host_root.push(profile.as_os_str());
    host_root.push("build");
    Some(host_root)
}

/// libspatialite OMIT_* defines applied to both `cc::Build` and the
/// generated `gaiaconfig.h`. Keeping the lists in sync prevents the
/// preprocessor from seeing two different views of the same TU.
///
/// The current selection mirrors the conservative bundle that the original
/// shpx consumer needed; future revisions may expose individual entries as
/// Cargo features (e.g. `enable-iconv`, `enable-rttopo`) so callers can opt
/// back into the dropped functionality.
#[cfg(feature = "bundled")]
const OMIT_FEATURES: &[&str] = &["ICONV", "FREEXL", "MATHSQL", "EPSG", "KNN", "GEOCALLBACKS"];

/// Lemon/flex-generated parser/lexer outputs included as data by their
/// `gg_*.c` wrappers via `#include`. Compiling them as standalone TUs
/// would produce duplicate symbols, so they are excluded from the source
/// set even though they live next to compilable files.
#[cfg(feature = "bundled")]
const EXCLUDED_C_FILES: &[&str] = &[
    "Ewkt.c",
    "geoJSON.c",
    "Gml.c",
    "Kml.c",
    "vanuatuWkt.c",
    "lex.Ewkt.c",
    "lex.GeoJson.c",
    "lex.Gml.c",
    "lex.Kml.c",
    "lex.VanuatuWkt.c",
];

#[cfg(feature = "bundled")]
fn write_generated_headers(out_dir: &std::path::Path) {
    let spatialite_dir = out_dir.join("spatialite");
    std::fs::create_dir_all(&spatialite_dir).expect("create OUT_DIR/spatialite");
    write_if_changed(&out_dir.join("config.h"), &config_h_body());
    // On Windows MSVC, libspatialite uses `#if defined(_WIN32) && !defined(__MINGW32__)`
    // to read `config-msvc.h` and `gaiaconfig-msvc.h` instead of the POSIX
    // counterparts. Empty stubs leave SPATIALITE_VERSION / OMIT_* undefined
    // and the build fails. We write near-identical bodies for both, with
    // the POSIX-only bits (HAVE_DLFCN_H, HAVE_UNISTD_H) toggled off in the
    // MSVC variant.
    write_if_changed(&out_dir.join("config-msvc.h"), config_msvc_h_body());
    write_if_changed(&spatialite_dir.join("gaiaconfig.h"), &gaiaconfig_h_body());
    write_if_changed(
        &spatialite_dir.join("gaiaconfig-msvc.h"),
        &gaiaconfig_h_body(),
    );
}

/// Write only when content differs to avoid mtime churn that confuses
/// `cc-rs`'s incremental compile checks.
#[cfg(feature = "bundled")]
fn write_if_changed(path: &std::path::Path, body: &str) {
    if std::fs::read_to_string(path).is_ok_and(|cur| cur == body) {
        return;
    }
    std::fs::write(path, body).unwrap_or_else(|e| panic!("write {} failed: {e}", path.display()));
}

#[cfg(feature = "bundled")]
fn config_h_body() -> String {
    // Replacement for the `config.h` autoconf would have produced. We
    // declare the minimum POSIX + standard C + sqlite3 set assumed by
    // libspatialite source on Linux / macOS.
    r"#ifndef SPATIALITE_BUNDLED_CONFIG_H
#define SPATIALITE_BUNDLED_CONFIG_H

#define HAVE_DLFCN_H 1
#define HAVE_FCNTL_H 1
#define HAVE_FLOAT_H 1
#define HAVE_INTTYPES_H 1
#define HAVE_LIMITS_H 1
#define HAVE_LOCALE_H 1
#define HAVE_MATH_H 1
#define HAVE_MEMORY_H 1
#define HAVE_STDINT_H 1
#define HAVE_STDIO_H 1
#define HAVE_STDLIB_H 1
#define HAVE_STRINGS_H 1
#define HAVE_STRING_H 1
#define HAVE_SYS_STAT_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_UNISTD_H 1
#define HAVE_SQLITE3EXT_H 1
#define HAVE_SQLITE3_H 1

#define HAVE_FDATASYNC 1
#define HAVE_FTRUNCATE 1
#define HAVE_GETCWD 1
#define HAVE_GETTIMEOFDAY 1
#define HAVE_LOCALTIME_R 1
#define HAVE_MEMMOVE 1
#define HAVE_MEMSET 1
#define HAVE_STRCASECMP 1
#define HAVE_STRERROR 1

#define HAVE_DECL_SQLITE_INDEX_CONSTRAINT_LIKE 1

#define _LARGEFILE_SOURCE 1
#define NDEBUG 1

#endif
"
    .to_string()
}

#[cfg(feature = "bundled")]
fn config_msvc_h_body() -> &'static str {
    // Windows MSVC variant. Drops POSIX-only declarations (DLFCN_H,
    // UNISTD_H, FDATASYNC, FTRUNCATE, LOCALTIME_R, STRCASECMP) and keeps
    // the standard C headers MSVC ships. libspatialite's source uses
    // `#ifdef _WIN32` to switch between `_stricmp` (Windows) and
    // `strcasecmp` (POSIX), so HAVE_STRCASECMP need not be defined here.
    "#ifndef SPATIALITE_BUNDLED_CONFIG_MSVC_H
#define SPATIALITE_BUNDLED_CONFIG_MSVC_H

#define HAVE_FCNTL_H 1
#define HAVE_FLOAT_H 1
#define HAVE_INTTYPES_H 1
#define HAVE_LIMITS_H 1
#define HAVE_LOCALE_H 1
#define HAVE_MATH_H 1
#define HAVE_MEMORY_H 1
#define HAVE_STDINT_H 1
#define HAVE_STDIO_H 1
#define HAVE_STDLIB_H 1
#define HAVE_STRING_H 1
#define HAVE_SYS_STAT_H 1
#define HAVE_SYS_TYPES_H 1
#define HAVE_SQLITE3EXT_H 1
#define HAVE_SQLITE3_H 1

#define HAVE_GETCWD 1
#define HAVE_GETTIMEOFDAY 1
#define HAVE_MEMMOVE 1
#define HAVE_MEMSET 1
#define HAVE_STRERROR 1

#define HAVE_DECL_SQLITE_INDEX_CONSTRAINT_LIKE 1

#define _LARGEFILE_SOURCE 1
#define NDEBUG 1

#endif
"
}

#[cfg(feature = "bundled")]
fn gaiaconfig_h_body() -> String {
    // Overrides the upstream `vendor/.../headers/spatialite/gaiaconfig.h`
    // to align public-API switches with this build. The upstream-baked
    // `ENABLE_*` set assumes a feature-rich autotools build; we undef it
    // and re-declare the minimum we want.
    //
    // - `PROJ_NEW`: select PROJ 6+ API (`proj_create_crs_to_crs` etc.).
    //   proj-sys 0.25 ships PROJ 9.4, so this must be on. Without it,
    //   `gg_transform.c` and friends try to include the legacy
    //   `proj_api.h`, which was removed in PROJ 8+.
    // - `GEOS_REENTRANT`: use libspatialite's `spatialite_alloc_reentrant`
    //   path. Disabling it falls into a non-reentrant fallback in
    //   `alloc_cache.c` that calls the legacy `pj_ctx_alloc()` (also
    //   removed in PROJ 8+) and the build fails. libgeos 3.x is
    //   fully reentrant, so this is safe to enable.
    let mut body = String::from(
        "#ifndef GAIACONFIG_H_BUNDLED\n#define GAIACONFIG_H_BUNDLED\n\n\
         #undef ENABLE_GCP\n#undef ENABLE_GEOPACKAGE\n#undef ENABLE_LIBXML2\n\
         #undef ENABLE_MINIZIP\n#undef ENABLE_RTTOPO\n\
         #undef GEOS_370\n#undef GEOS_3100\n#undef GEOS_3110\n\
         #undef GEOS_ADVANCED\n#undef GEOS_ONLY_REENTRANT\n\
         #define GEOS_REENTRANT 1\n\
         #define PROJ_NEW 1\n\n",
    );
    for omit in OMIT_FEATURES {
        use std::fmt::Write;
        writeln!(body, "#define OMIT_{omit} 1").unwrap();
    }
    body.push_str(
        "\n#define SPATIALITE_TARGET_CPU \"libspatialite-sys-bundled\"\n\
         #define SPATIALITE_VERSION \"5.1.0\"\n\n#endif\n",
    );
    body
}
