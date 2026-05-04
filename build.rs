//! Build script for libspatialite-sys (skeleton stage).
//!
//! Cargo requires a build script whenever a package declares the `links` key,
//! which we do (`links = "spatialite"` in Cargo.toml). At this stage no
//! feature emits link directives; this is a placeholder that does nothing.
//!
//! Subsequent commits will add:
//!   - `bundled`       : compile vendored libspatialite via cc::Build and
//!                       static-link it together with GEOS / PROJ / zlib.
//!   - `bundled-vcpkg` : resolve libspatialite via `vcpkg::Config::find_package`
//!                       to use a user-provisioned vcpkg port (Windows MSVC).
//!
//! The default build (no feature) emits no link directives. Consumers are
//! expected to load libspatialite at runtime via, for example,
//! `rusqlite::Connection::load_extension("mod_spatialite")`.

fn main() {
    // Only re-run when the build script itself changes. The bundled feature
    // implementation will add `rerun-if-changed` for the vendored source tree.
    println!("cargo:rerun-if-changed=build.rs");
}
