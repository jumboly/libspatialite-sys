# libspatialite-sys (日本語)

[![CI](https://github.com/jumboly/libspatialite-sys/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jumboly/libspatialite-sys/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/libspatialite-sys.svg)](https://crates.io/crates/libspatialite-sys)
[![docs.rs](https://docs.rs/libspatialite-sys/badge.svg)](https://docs.rs/libspatialite-sys)

[English README](README.md)

[libspatialite][upstream] (SpatiaLite SQLite 拡張) の Rust 向け低レベル FFI バインディング
および bundled ビルド。

[upstream]: https://www.gaia-gis.it/fossil/libspatialite/index

> **状態: 0.1 未満、開発中。**
> このクレートは、Linux / macOS で libspatialite を bundle 配布している実利用者
> ([shpx]) から切り出して整備中です。3 つの feature 経路 (デフォルトの
> ヘッダ宣言のみ / `bundled` / `bundled-vcpkg`) はすべて実装済みで CI でも
> 検証しています。0.1.0 を crates.io に publish するまでに API 表面や feature
> 名が変更される可能性があります。

[shpx]: https://github.com/jumboly/shpx

## このクレートが提供するもの

- libspatialite を SQLite connection に attach するための最小限の `extern "C"`
  FFI 表面 (`spatialite_initialize` / `spatialite_alloc_connection` /
  `spatialite_init_ex`)。`rusqlite::Connection` (あるいは `libsqlite3-sys` を
  使う任意の SQLite バインディング) と組み合わせて、static link 経由で
  SpatiaLite SQL 関数群を初期化できる。
- `bundled` Cargo feature: libspatialite と GEOS ([`geos-src`]) / PROJ
  ([`proj-sys`]) / zlib ([`libz-sys`]) をソースから build し、バイナリに
  static link する経路。Linux / macOS が対象。
- `bundled-vcpkg` Cargo feature: Windows MSVC 向けに、ユーザー側で install
  された vcpkg installation 経由で libspatialite を解決する経路。上流
  libspatialite ソースには MSVC parser 非互換の既知問題があり、vcpkg port が
  patch を当てているため、Windows ではこちらを推奨します。

[`geos-src`]: https://crates.io/crates/geos-src
[`proj-sys`]: https://crates.io/crates/proj-sys
[`libz-sys`]: https://crates.io/crates/libz-sys

このクレートは SpatiaLite が提供する数百の SQL 関数 (`AsBinary` / `GeomFromWKB`
/ `ST_*` / `Transform` 等) を Rust 関数として **再 export しません**。これらは
connection 初期化後に SQL 経由で呼び出すのが SpatiaLite の慣習的な使い方です。

## Cargo features

| Feature | デフォルト | 役割 |
|---|---|---|
| (なし) | はい | ヘッダ + FFI 宣言のみ。ライブラリ解決は呼び出し側に委ねる。`rusqlite::Connection::load_extension("mod_spatialite")` で動的ロードするユーザー向け |
| `bundled` | いいえ | vendored ソースを `cc::Build` で build。`geos-src` / `proj-sys` / `libz-sys` も自動で pull する。CMake と C/C++ toolchain が必要。**Linux / macOS のみ**。Windows MSVC は既知の build 失敗があるので `bundled-vcpkg` を使うこと |
| `bundled-vcpkg` | いいえ | vcpkg 経由で libspatialite を解決 (`vcpkg install libspatialite:x64-windows-static-md`)。Windows MSVC 推奨。`VCPKG_ROOT` 設定または vcpkg が PATH 上にあることが必要 |

## プラットフォームサポート

| Target | `default` (system) | `bundled` | `bundled-vcpkg` |
|---|---|---|---|
| `x86_64-unknown-linux-gnu` | ✅ pkg-config / `LD_LIBRARY_PATH` | ✅ CI で動作確認 | (該当なし) |
| `aarch64-unknown-linux-gnu` | ✅ pkg-config / `LD_LIBRARY_PATH` | ✅ CI で動作確認 | (該当なし) |
| `aarch64-apple-darwin` | ✅ Homebrew (`libspatialite`) | ✅ CI で動作確認 | (該当なし) |
| `x86_64-apple-darwin` | ✅ Homebrew | ✅ CI で動作確認 | (該当なし) |
| `x86_64-pc-windows-msvc` | ⚠️ ユーザー側で `mod_spatialite.dll` を用意 | ❌ MSVC parser 非互換 (`bundled-vcpkg` を使うこと) | ✅ CI で検証 |
| `x86_64-pc-windows-gnu` | ⚠️ MSYS2 | ✅ (CI 未検証) | (該当なし) |

## ライセンス

クレート本体のメタ部分 (Cargo.toml, build.rs, src/lib.rs, README, etc.) は
以下のいずれか:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

の選択でデュアルライセンスです。

`bundled` feature 有効時に同梱される libspatialite ソースは上流プロジェクトに
より **MPL 1.1 / GPL 2.0 / LGPL 2.1 のトリプルライセンス** で提供されます。
libspatialite を静的リンクしたバイナリを再配布する場合、これら 3 つのうち
いずれかへの適合が必要です。詳細と各オプションでの義務については
[`NOTICE`](NOTICE) を参照してください。

`bundled-vcpkg` 経路は、ローカル vcpkg installation で build された
libspatialite にリンクします。同じ上流ライセンスが適用されますが、vcpkg port
自体は MIT ライセンスです。

## バージョニング

semver に従います。`0.0.x` の間は FFI 表面と feature flag の破壊的変更があり
得ます。0.1.0 publish 後は、FFI 表面・公開 Cargo features・ドキュメント化された
環境変数を stable として扱います。

## 謝辞

- libspatialite 本体: Alessandro Furieri 氏および SpatiaLite contributors。
- ビルド系統は `proj-sys` / `geos-src` / `libsqlite3-sys` の先行知見に大きく
  依拠しています。
