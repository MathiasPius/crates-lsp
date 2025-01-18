# Change Log

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