# Change Log

## 0.3.0

### Breaking

* Now uses the OS-specific standards for cached data:
  * Linux: `$XDG_CACHE_HOME/crates-lsp` or `$HOME/.cache/crates-lsp`
  * macOS: `$HOME/Library/Caches/crates-lsp`
  * Windows: `{FOLDERID_LocalAppData}\crates-lsp\cache`

  Previously a `.crates-lsp` directory within the current workspace was used, but this was a leftover from when the Lapce Editor was the primary target of this language server.

  See [directories-rs](https://codeberg.org/dirs/directories-rs) for more information.

### Added

* Added `cache_directory` configuration option for overriding the defaults.

## 0.2.0

### Breaking

* Flattened settings structure.

  Previously `crates-lsp` configuration settings all lived in a nested `lsp` object within the initialization parameters:

  ```lua
  vim.lsp.config['crates'] = {
    cmd = { 'crates-lsp' },
    filetypes = { 'toml' },
    root_markers = { 'Cargo.toml', '.git' },
    init_options = {
      lsp = {
        inlay_hints = false
      }
    }
  }
  ```

  Since all settings are necessarily LSP settings, this didn't really make sense.

  All settings now live in the root initialization parameters object:

  ```lua
  vim.lsp.config['crates'] = {
    cmd = { 'crates-lsp' },
    filetypes = { 'toml' },
    root_markers = { 'Cargo.toml', '.git' },
    init_options = {
      inlay_hints = false
    }
  }
  ```

### Added

* `files` configuration option, allowing you to limit the files `crates-lsp` should activate for, without relying on the lsp client which might not be able to filter. Solves #20.

  Note:
  * If not specified, this option defaults to `Cargo.toml`.
  * Uses exact matching on the entire filename. Partial matches, patterns, globbing, regex is not supported.

  Example (nvim):
  ```lua
  vim.lsp.config['crates'] = {
    cmd = { 'crates-lsp' },
    filetypes = { 'toml' },
    root_markers = { 'Cargo.toml', '.git' },
    init_options = {
      files = { 'Cargo.toml', 'AlternativeCargoFilename.toml']
    }
  }
  ```


## 0.1.8

### Fixed

* Dependency updates, linting, etc.

## 0.1.7

### Fixed

* Updated tokio dependency. Resolves a vulnerability warning, which does not affect crates-lsp.

## 0.1.6

### Fixed

* Updated ring dependency in response to vulnerability.
* Renamed crates cache directory from .lapce to .crates-lsp

## 0.1.5

### Added

* Added crates.io build pipeline (#14)

## 0.1.4

### Fixed

* Replaced OpenSSL dependency with rustls/webpki
* Updated dependencies

## 0.1.3

### Added

* Add *Code Action* for updating version by [@Vulpesx](https://github.com/MathiasPius/crates-lsp/pull/9)
* Add *Inlay Hints* by [@Vulpesx](https://github.com/MathiasPius/crates-lsp/pull/10)

## 0.1.2

### Added

* Implement diagnostic levels by [@Vulpesx](https://github.com/MathiasPius/crates-lsp/pull/8)

## 0.1.1

### Fixed

* Updated hyper-rustls from 0.26.0 to 0.27.0

## 0.1.0

### Fixed
* Update to hyper 0.x to 1.2.0
* Update mio dependency to resolve [CVE-2024-27308](https://github.com/advisories/GHSA-r8w9-5wcg-vfj7/dependabot)
* Switch to rustls webpki roots instead of native ones. Potentially breaking change.

## 0.0.6

### Added
* Added crate name search courtesy of @jm-observer

## 0.0.4

### Added
* Implemented CrateCache which can be used with either of the crates.io backends.
* Implemented crates.io sparse index backend and set it as the default.

### Fixed
* Left-over test code would create file CRATE_CACHE_DIR/test.

## 0.0.3

### Fixed
* Fix vulnerability in [rustls-webpki](https://github.com/briansmith/webpki/issues/69)
* Check crate versions immediately on open, instead of only on change.
