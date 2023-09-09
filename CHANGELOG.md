# Change Log

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