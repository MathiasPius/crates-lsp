# crates-lsp
Language Server implementation targeted specifically towards the `Cargo.toml`
file of Rust projects, providing auto-completion for crate versions and
in-editor hints when the selected crate versions are out of date.

This project was started specifically to be used with the Lapce editor plugin [lapce-crates](https://github.com/MathiasPius/lapce-crates/), but should work with any LSP-capable editor.

Project is heavily inspired by the [crates](https://github.com/serayuzgur/crates) plugin for VSCode.

# Usage


## Lapce
To use this with Lapce, install the Crates plugin from within the Lapce editor.

## Helix
@ameknite kindly provided this example configuration for the [Helix](https://helix-editor.com/) editor:

```toml
[[language]]
name = "toml"
language-servers = [
    { name = "crates-lsp", except-features = [
        "format",
    ] },
    "taplo",
]

formatter = { command = "taplo", args = ["fmt", "-"] }
```
