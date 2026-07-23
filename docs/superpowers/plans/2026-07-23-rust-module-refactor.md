# Rust Module Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the current large Rust crate entry files into focused modules without changing synchronization behavior.

**Architecture:** Keep existing crate boundaries and public APIs. Move implementation into responsibility-based modules, then re-export the public surface from each crate root so current tests and binaries continue to compile.

**Tech Stack:** Rust 2024, Cargo workspace, Axum, SeaORM, ldap3, reqwest, Tokio.

---

## File Structure

- `crates/adss-agent/src/lib.rs`: thin crate root with module declarations and public re-exports.
- `crates/adss-agent/src/config.rs`: process and LDAP configuration parsing.
- `crates/adss-agent/src/control_plane.rs`: HTTP control-plane client traits and implementation.
- `crates/adss-agent/src/state.rs`: local revision state and file-backed state store.
- `crates/adss-agent/src/runtime.rs`: agent sync loop orchestration.
- `crates/adss-agent/src/directory/`: directory execution, dry-run client, and LDAPS client.
- `crates/adss-server/src/lib.rs`: thin crate root with module declarations and public re-exports.
- `crates/adss-server/src/config.rs`: server environment configuration.
- `crates/adss-server/src/state.rs`: application state construction.
- `crates/adss-server/src/routes.rs`: HTTP routes and handlers.
- `crates/adss-server/src/auth.rs`: Agent key authentication helpers.
- `crates/adss-server/src/password/`: password envelope and hash providers.
- `crates/adss-server/src/error.rs`: API error response mapping.
- `crates/adss-persistence/src/lib.rs`: thin crate root with module declarations and public re-exports.
- `crates/adss-persistence/src/entities.rs`: SeaORM entities.
- `crates/adss-persistence/src/models.rs`: repository input and output structs.
- `crates/adss-persistence/src/repository.rs`: repository methods.
- `crates/adss-persistence/src/revision.rs`: revision allocation and confirmation helpers.
- `crates/adss-persistence/src/mapping.rs`: model conversions and storage mapping.

## Tasks

### Task 1: Split Agent Runtime

**Files:**
- Modify: `crates/adss-agent/src/lib.rs`
- Create: `crates/adss-agent/src/config.rs`
- Create: `crates/adss-agent/src/control_plane.rs`
- Create: `crates/adss-agent/src/state.rs`
- Create: `crates/adss-agent/src/runtime.rs`
- Create: `crates/adss-agent/src/directory/mod.rs`
- Create: `crates/adss-agent/src/directory/dry_run.rs`
- Create: `crates/adss-agent/src/directory/ldap.rs`

- [ ] Move code into modules without changing behavior.
- [ ] Keep crate-root exports used by current tests and `main.rs`.
- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo test -p adss-agent`.

### Task 2: Split Server Runtime

**Files:**
- Modify: `crates/adss-server/src/lib.rs`
- Create: `crates/adss-server/src/config.rs`
- Create: `crates/adss-server/src/state.rs`
- Create: `crates/adss-server/src/routes.rs`
- Create: `crates/adss-server/src/auth.rs`
- Create: `crates/adss-server/src/error.rs`
- Create: `crates/adss-server/src/password/mod.rs`
- Create: `crates/adss-server/src/password/envelope.rs`
- Create: `crates/adss-server/src/password/hash.rs`

- [ ] Move code into modules without changing handler behavior.
- [ ] Keep crate-root exports `ServerConfig`, `AppState`, and `build_router`.
- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo test -p adss-server`.

### Task 3: Split Persistence Runtime

**Files:**
- Modify: `crates/adss-persistence/src/lib.rs`
- Create: `crates/adss-persistence/src/entities.rs`
- Create: `crates/adss-persistence/src/models.rs`
- Create: `crates/adss-persistence/src/repository.rs`
- Create: `crates/adss-persistence/src/revision.rs`
- Create: `crates/adss-persistence/src/mapping.rs`

- [ ] Move code into modules without changing SQL, schema, transaction, or revision behavior.
- [ ] Keep crate-root exports used by current tests and server code.
- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo test -p adss-persistence`.

### Task 4: Workspace Verification

**Files:**
- All Rust workspace files touched by Tasks 1-3.

- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings`.
- [ ] Review `git diff --stat` and `git diff --check`.
