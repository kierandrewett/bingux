# Crates

Rust libraries that the apps and services build on. A directory here has no
program of its own that a person launches, apart from `config` and `ui`'s
examples. Cargo package names keep the `bingux-` prefix.

| Directory       | Cargo package         | What it holds                                                                              |
| --------------- | --------------------- | ------------------------------------------------------------------------------------------ |
| `ui/`           | `bingux-ui`           | The Bingux design system for Freya applications. See `docs/bingux-ui.md`.                  |
| `config/`       | `bingux-config`       | The declared configuration schema, the TOML store and the `bingux-config` command.         |
| `files/`        | `bingux-files`        | The filesystem model and file chooser logic shared by Files and the chooser.               |
| `preview-core/` | `bingux-preview-core` | What a file is and which viewer opens it. The host process is `services/preview-host`.     |
| `search/`       | `bingux-search`       | The search engine: documents, schema and inverted index. The daemon is `services/searchd`. |
| `data-usage/`   | `bingux-data-usage`   | The network data usage recorder and reader.                                                |
