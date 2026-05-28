# Swift ↔ Rust SSH Bridge

This document is kept as historical context for the older experiment where
Rust terminal session code could call back into a Swift-owned `SSHClient`
through a C vtable.

Production SSH in this worktree is Rust-owned through `russh_client.rs`.
Swift still exposes the app-facing `SSHClient` protocol, but
`RusshSSHClient` is now only a small wrapper around the Rust FFI surface:

- `bt_russh_client_create`
- `bt_russh_client_connect`
- `bt_russh_client_write`
- `bt_russh_client_resize`
- `bt_russh_client_disconnect`
- `bt_russh_client_release`

The old `ssh_bridge.rs` vtable remains available for tests and future
session-port experiments, but it is no longer the production transport path.
