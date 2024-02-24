# Orchid Language Server

This repository hosts editor support for [Orchid](https://github.com/lbfalvy/orchid). The editor support is split into a language server written in Rust that links the interpreter statically, and a VSCode plugin. The two communicate using the Language Server Plugin.

The server uses the LSP liberally, for example, we defined a new server -> client notification `client/syntacticTokens` to mimic the existing concept of semantic tokens, but with fewer restrictions. This approach requires that we develop the clients and server together.

Integrations for other editors are planned, contributions are welcome.

## Development

For the dev scaffold to work correctly, Orchid must be checked out in `../orchid`. The project requires an up-to-date VSCode, Node, NPM and stable and nightly Rust toolchains. Nightly rust is only used for rustfmt.