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

## 使い方

### クイックスタート: rusqlite との組み合わせ (推奨)

ほとんどの利用者は「libspatialite を static link した単一の Rust
バイナリ」を作りたいはずです。本クレートを [`rusqlite`] の `bundled`
feature と組み合わせると、Cargo の `links = "sqlite3"` 重複検出によって
両者が **同じ `libsqlite3-sys` インスタンス** を共有するので、最終
バイナリには SQLite が 1 コピーしか入りません。

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

`rusqlite` のバージョンは、本クレートが依存している `libsqlite3-sys`
と **同じメジャー (現状 `0.28`)** に揃えてください。メジャーがズレると
build 時に `links = "sqlite3"` 重複エラーで停止します — 1 つのバイナリに
SQLite が 2 コピー入るのを防ぐ意図的なセーフガードです。

**接続の初期化**

libspatialite を static link した場合、loadable-extension の入口関数
(`sqlite3_modspatialite_init`) は使えません。代わりに以下の 3 ステップ
ブートストラップを実行します。`spatialite_initialize` はプロセス全体で
1 度だけ呼ぶ仕様なので `Once` でガードしてください:

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
    // libspatialite_sys::sqlite3 と libsqlite3_sys::sqlite3 はどちらも
    // opaque ZST で layout 互換。
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

> **`spatialite_cleanup_ex` は呼ばないでください。** 接続ごとの cache は
> `sqlite3_close` 時に libspatialite 内部で自動 free されるので、明示
> 呼び出しは double-free になります。

### 動的ロード経路 (`bundled*` feature 無し)

`mod_spatialite.{so,dylib,dll}` を別途配布して実行時にロードする運用
なら、**feature 無し** で本クレートを依存に入れます:

```toml
[dependencies]
libspatialite-sys = "0.0.1"
rusqlite = { version = "0.31", features = ["bundled", "load_extension"] }
```

この場合の本クレートの役割は FFI シンボル宣言を必要な人向けに提供する
だけで、実際のロードは rusqlite (または利用中の SQLite バインディング)
側で行います:

```rust
unsafe { conn.load_extension_enable()? };
unsafe { conn.load_extension("mod_spatialite", None)? };
unsafe { conn.load_extension_disable()? };
```

利用者の環境に `mod_spatialite` が事前 install されていることが前提です
(macOS Homebrew、Debian / Ubuntu の `libsqlite3-mod-spatialite`、Windows
の vcpkg など)。「単一バイナリ配布」の利点は失われますが、ビルドが軽い
経路です。

### Windows ローカル開発 (`bundled-vcpkg`)

Windows 経路では、`x64-windows-static-md` triplet で libspatialite を
build した vcpkg installation が必要です。**triplet は必ずこれを使って
ください** — cargo-dist が標準で前提とする `/MD` (動的 UCRT) ABI と整合
します。`x64-windows-static` (`/MT`) を混在させると実行時 ABI 不整合が
起きます。

**一度だけのセットアップ**

```pwsh
git clone https://github.com/microsoft/vcpkg C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat -disableMetrics

# libspatialite + GEOS + PROJ + sqlite3 + zlib をソースからビルド。
# 初回 cold は ~20 分。以降は C:\vcpkg\installed\ にキャッシュされ
# 再利用される。
C:\vcpkg\vcpkg.exe install libspatialite:x64-windows-static-md
```

**シェルごとの環境変数**

```pwsh
$env:VCPKG_ROOT = "C:\vcpkg"
$env:VCPKGRS_TRIPLET = "x64-windows-static-md"
```

`VCPKGRS_TRIPLET` を毎回打ちたくなければ、repo に
`.cargo/config.toml` を置きます:

```toml
[env]
VCPKGRS_TRIPLET = "x64-windows-static-md"
```

これで `cargo build --release --features bundled-vcpkg` が DLL 依存の
ない単一 `.exe` を生成します (libspatialite / GEOS / PROJ / SQLite /
zlib すべて static link)。

### CI への組み込み

GitHub Actions の Windows runner には vcpkg checkout が `C:\vcpkg` に
入っていますが、cold な `vcpkg install libspatialite` は ~20 分かかり、
`actions/cache` の標準的な archive 方式 (7 日 idle 退去・tarball
all-or-nothing) では実用的になりません。これを解決するため、companion
Marketplace action を別 repo で公開しています:
[`jumboly/setup-vcpkg-nuget-cache`][nugetcache]。各 vcpkg port を NuGet
package として GitHub Packages にキャッシュし、2 回目以降は事前ビルド
済み artifact を install します。

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

Linux / macOS ジョブは CMake (transitive な `geos-src` / `proj-sys`
ビルド用) と C/C++ toolchain があれば動きます:

```yaml
- run: sudo apt-get install -y cmake   # ubuntu
- run: brew install cmake              # macos
- run: cargo build --release --features bundled
```

3 OS をカバーする参考実装は
[`.github/workflows/ci.yml`](.github/workflows/ci.yml) を参照して
ください。

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
