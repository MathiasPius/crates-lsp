# crates-lsp
Language Server implementation targeted specifically towards the `Cargo.toml`
file of Rust projects, providing auto-completion for crate versions and
in-editor hints when the selected crate versions are out of date.

This project was started specifically to be used with the Lapce editor plugin [lapce-crates](https://github.com/MathiasPius/lapce-crates/), but should work with any LSP-capable editor.

Project is heavily inspired by the [crates](https://github.com/serayuzgur/crates) plugin for VSCode.

# Usage
To use this with Lapce, install the Crates plugin from within the Lapce editor.
