# crates-lsp
Language Server implementation targeted specifically towards the `Cargo.toml`
file of Rust projects, providing auto-completion for crate versions and
in-editor hints when the selected crate versions are out of date.

Project is heavily inspired by the [crates](https://github.com/serayuzgur/crates) plugin for VSCode.

# Usage

In order to use `crates-lsp`, you must:
1. Install the `crates-lsp` binary somewhere on your machine.
2. Configure your editor to use the `crates-lsp` binary as a Language Server.


## 1. Installing crates-lsp
You can either download the pre-built `crates-lsp` binary from a [release](https://github.com/MathiasPius/crates-lsp/releases/latest),
or if you have Cargo (the Rust build tool) installed, you can build and install it yourself directly from [crates.io](https://crates.io/crates/crates-lsp):

```bash
cargo install --locked crates-lsp
```

## 2. Configuring your editor
This step is highly dependent on which editor you are using, see the guides below for known example configurations.

In most cases, these require `crates-lsp` to be available in your `$PATH`, or for you to specify the full path to it.

### [Neovim](https://neovim.io/)
```lua
vim.lsp.config['crates'] = {
  cmd = { 'crates-lsp' },
  filetypes = { 'toml' },
  root_markers = { 'Cargo.toml', '.git' },
  init_options = {
    -- Configuration options go here
  }
}
```

### [Helix](https://helix-editor.com/)
```toml
[language-servers.crates-lsp]
except-features = ["format"]
# config = {} # Configuration options go here

[[language]]
name = "toml"
language-servers = [ "crates-lsp", "taplo" ]
formatter = { command = "taplo", args = ["fmt", "-"] }
```
See the [Languages](https://docs.helix-editor.com/languages.html#languages) section of the Helix wiki, if you're unsure of where to put this.

See [Lsp](https://neovim.io/doc/user/lsp.html) section of the Neovim wiki, if you're unsure of where to put this.

# Configuration
`crates-lsp` has the following configuration options, which can be passed in from your editor (See [Usage](#usage) section above):

| Option    | Type   | Default | Description |
|-----------|--------|---------|-------------|
| `cache_directory` | `string/path` | OS-specific `crates-lsp` cache directory | Directory in which to cache information about available crate versions, to avoid constantly querying crates.io. Uses the OS-specific cache directory. See [directories-rs](https://codeberg.org/dirs/directories-rs) |
| `files` | `[string]` | `["Cargo.toml"]` | List of exact filenames for which `crates-lsp` should provide feedback. Avoids `crates-lsp` throwing errors if you happen to open a `toml` file with a `[dependencies]` section, which does not contain Rust package names. |
| `use_api` | `bool` | `false` | If true, uses the [Crates API](https://crates.io/data-access#api) instead of the [Crate Index](https://crates.io/data-access#crate-index). There are almost no reasons to ever enable this, and doing so puts a lot more strain on the services provided by crates.io. Please don't! |
| `inlay_hints` | `bool` | `true` | If false, disables inlay hints. |
| `up_to_date_hint` | `string` | `✓` | Text of inlay hint to show, when package is up to date. |
| `needs_update_hint` | `string` | ` {}` | Text of inlay hint to show next to packages which should be updated. Any appearance of `{}` within the string, will be replaced by the newer version, which the package should be updated to. |
| `diagnostics` | `bool` | `true` | If false, disables diagnostics. |
| `unknown_dep_severity` | `int` | `2` (WARNING) | Sets severity of diagnostics indicating that a package could not be looked up. See [Diagnostic Severity](#diagnostic-severity) |
| `needs_update_severity` | `int` | `3` (INFO) | Sets severity of diagnostics indicating that a package needs to be updated. See [Diagnostic Severity](#diagnostic-severity) |
| `up_to_date_severity` | `int` | `4` (HINT) | Sets severity of diagnostics indicating that a package is up to date. See [Diagnostic Severity](#diagnostic-severity) |

## Diagnostic Severity
Integer indicating the severity to use when `crates-lsp` conveys a *diagnostic* about packages, such as: *Needs Update*, *Unknown Package*, or *Up To Date*. The following severities are available:

* `1` ERROR
* `2` WARNING
* `3` INFORMATION
* `4` HINT

## Examples
These examples *only* showcase configuration options, and are not complete examples!

### [Neovim](https://neovim.io/)
The `init_options` field contains the `crates-lsp` configuration options.
```lua
vim.lsp.config['crates'] = {
  init_options = {
    inlay_hints = false,      -- Disable inlay hints entirely
    needs_update_severity = 1 -- Report necessary updates as ERRORs
  }
}
```

### [Helix](https://helix-editor.com/)
```toml
[language-server.crates-lsp]
# Disable inlay hints, Report necessary updates as ERRORs
config = { inlay_hints = false, needs_update_severity = 1 } 
```