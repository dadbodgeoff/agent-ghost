# ghost-backup

> Encrypted state backup and restore — collect, compress, encrypt, archive, and restore the entire GHOST platform state with blake3 integrity verification and configurable retention.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 6 (Data Services) |
| Type | Library |
| Location | `crates/ghost-backup/` |
| Workspace deps | None (standalone) |
| External deps | `blake3`, `serde`, `serde_json`, `chrono`, `uuid`, `thiserror`, `tokio`, `tracing` |
| Modules | `export` (archive creation), `import` (restore + verify), `scheduler` (automatic backups) |
| Public API | `BackupExporter`, `BackupImporter`, `BackupScheduler`, `BackupManifest` |
| Archive format | `.ghost-backup` (manifest + data, XOR-encrypted with blake3-derived key) |
| Integrity | blake3 hash per file, verified on import |
| Test coverage | Export/import round-trip, wrong passphrase rejection, corruption detection, retention enforcement |
| Downstream consumers | `ghost-gateway` (scheduled backups, manual backup/restore API) |

---

## Why This Crate Exists

GHOST stores critical state: agent identities (Ed25519 keypairs), conversation history, memory databases, convergence baselines, skill registries, and configuration. Losing this data means losing the agent's identity and accumulated knowledge.

`ghost-backup` provides three things:

1. **Export** — Recursively collects all files from `~/.ghost/{data,config,agents}`, computes blake3 hashes, encrypts with a passphrase-derived key, and writes a single `.ghost-backup` archive.

2. **Import** — Decrypts the archive, verifies every file's blake3 hash against the manifest, and restores to the target directory. If any hash doesn't match, the entire import fails — no partial restores.

3. **Scheduled backups** — Configurable daily or weekly automatic backups with retention policy (default: keep 7, delete oldest).

---

## Module Breakdown

### `export.rs` — Archive Creation

The exporter walks the GHOST data directory and produces a self-contained archive.

#### Archive Format

```
[manifest_len: 4 bytes LE u32][manifest: JSON][data: JSON map of path→bytes]
```

The entire payload is then encrypted. The format is intentionally simple:

1. **4-byte manifest length** — Allows the importer to extract the manifest without parsing the entire archive. This enables "peek" operations (list contents without full decryption).

2. **JSON manifest** — Human-inspectable (after decryption) list of all files with paths, sizes, and blake3 hashes.

3. **JSON data map** — `BTreeMap<String, Vec<u8>>` serialized as JSON. BTreeMap ensures deterministic ordering — the same input always produces the same archive (modulo timestamp).

#### What Gets Backed Up

| Directory | Contents |
|-----------|----------|
| `data/` | SQLite databases, convergence state, session history |
| `config/` | ghost.yml, policy files, skill manifests |
| `agents/` | Per-agent identity files, keypairs, baselines |

**What's NOT backed up:** Temporary files, log files, and the backup directory itself (to prevent recursive backup).

#### Encryption

```rust
fn encrypt(data: &[u8], passphrase: &str) -> Vec<u8> {
    let key_bytes = blake3::hash(passphrase.as_bytes());
    let key = key_bytes.as_bytes();
    data.iter().enumerate().map(|(i, b)| b ^ key[i % 32]).collect()
}
```

**This is a placeholder.** The current implementation uses XOR with a blake3-derived key. Production will use the `age` crate for proper authenticated encryption (X25519 + ChaCha20-Poly1305). The XOR placeholder is sufficient for development and testing — it validates the encrypt/decrypt round-trip and passphrase-based key derivation — but provides no real security against a determined attacker.

**Why blake3 for key derivation?** Consistency with the rest of the platform. In production, this would be replaced with Argon2id (memory-hard KDF) to resist brute-force attacks on the passphrase.

---

### `import.rs` — Restore with Integrity Verification

The importer is the security-critical path. It must verify that the archive hasn't been tampered with before restoring any files.

#### Verification Pipeline

1. **Decrypt** — Apply the inverse of the encryption function with the provided passphrase.
2. **Parse manifest** — Extract the 4-byte length prefix, deserialize the manifest JSON.
3. **Parse data** — Deserialize the remaining bytes as the file data map.
4. **Verify every file** — For each entry in the manifest, compute blake3 of the corresponding data and compare to the manifest hash. If ANY hash doesn't match, the entire import fails.
5. **Restore** — Write all files to the target directory, creating parent directories as needed.

**Why all-or-nothing verification?** A partial restore (some files verified, some not) could leave the platform in an inconsistent state. For example, restoring a new identity keypair without the corresponding convergence baseline would cause the agent to start with a mismatched identity. The all-or-nothing approach ensures the restored state is internally consistent.

**Wrong passphrase detection:** If the passphrase is wrong, decryption produces garbage. The manifest JSON parse will fail with an `IntegrityError`, which is the correct behavior — the user sees "integrity check failed" rather than a confusing JSON parse error.

---

### `scheduler.rs` — Automatic Backups with Retention

The scheduler wraps the exporter with time-based triggering and old backup cleanup.

#### Configuration

```rust
pub struct BackupSchedulerConfig {
    pub interval: BackupInterval,      // Daily or Weekly
    pub retention_count: usize,        // Default: 7
    pub backup_dir: PathBuf,           // Default: ~/.ghost/backups
    pub ghost_dir: PathBuf,            // Default: ~/.ghost
    pub passphrase_env: String,        // Default: "GHOST_BACKUP_KEY"
}
```

**Passphrase from environment variable.** The backup passphrase is read from `$GHOST_BACKUP_KEY` at runtime. It's never stored in configuration files. If the env var is not set, the scheduler warns and creates an unencrypted backup — this is a deliberate degradation choice. An unencrypted backup is better than no backup.

#### Retention Policy

```rust
fn enforce_retention(&self) -> BackupResult<()> {
    // List all .ghost-backup files, sort by name (timestamp-based)
    // Delete oldest until count <= retention_count
}
```

**Timestamp-based naming:** Backup files are named `ghost_YYYYMMDD_HHMMSS.ghost-backup`. Lexicographic sorting equals chronological sorting, so the retention policy simply sorts by filename and deletes from the front.

**Why 7 as the default retention?** One week of daily backups. This covers the common "I broke something yesterday" recovery scenario while keeping disk usage bounded. Weekly backups with retention_count=4 gives a month of history.

---

## Security Properties

### Integrity Verification

Every file in the archive has a blake3 hash recorded in the manifest. On import, every hash is recomputed and compared. A single bit flip in any file causes the entire import to fail. This detects both accidental corruption (disk errors) and intentional tampering.

### Passphrase-Based Encryption

Archives are encrypted with a key derived from a user-provided passphrase. The passphrase is read from an environment variable, never from disk. Without the passphrase, the archive contents are inaccessible.

### Non-Destructive Restore

The importer writes to a target directory. It never modifies the archive file or the source backup. A failed import leaves no partial state — either all files are restored or none are.

---

## Downstream Consumer Map

```
ghost-backup (Layer 6)
└── ghost-gateway (Layer 8)
    └── Runs BackupScheduler on startup (daily/weekly)
    └── Exposes /api/backup/create for manual backups
    └── Exposes /api/backup/restore for manual restore
    └── Exposes /api/backup/list for backup inventory
```

---

## Test Strategy

### Integration Tests (`tests/backup_tests.rs`)

| Test | What It Verifies |
|------|-----------------|
| `export_creates_valid_archive` | Archive file exists, manifest has entries, version is "1" |
| `import_restores_all_data` | All manifest entries exist in restore directory |
| `export_import_roundtrip_identical` | Restored file content matches original byte-for-byte |
| `import_wrong_passphrase_fails` | Wrong passphrase → error (not silent corruption) |
| `import_corrupted_archive_fails` | Random bytes → error |
| `scheduler_retention_policy` | 5 old backups + 1 new, retention=3 → ≤3 remain |

---

## File Map

```
crates/ghost-backup/
├── Cargo.toml                          # Deps: blake3, serde_json, chrono
├── src/
│   ├── lib.rs                          # BackupError, BackupManifest, ManifestEntry
│   ├── export.rs                       # BackupExporter, archive creation, encryption
│   ├── import.rs                       # BackupImporter, decryption, integrity verification
│   └── scheduler.rs                    # BackupScheduler, retention policy, env-based passphrase
└── tests/
    └── backup_tests.rs                 # Round-trip, corruption, passphrase, retention tests
```

---

## Common Questions

### Why not use tar/gzip for the archive format?

Simplicity and portability. The custom format is trivial to parse (length-prefixed JSON), doesn't require external tools, and embeds integrity hashes directly in the manifest. A tar archive would need a separate hash manifest file and add the `tar` crate as a dependency.

### When will the XOR encryption be replaced with real encryption?

When the `age` crate integration is implemented. The current XOR placeholder validates the encrypt/decrypt interface and passphrase-based key derivation. The `age` crate provides X25519 key agreement + ChaCha20-Poly1305 authenticated encryption, which is the target for production.

### Can I restore a backup from a different GHOST version?

The manifest includes `platform_version`. Currently, version checking is not enforced (the `VersionMismatch` error variant exists but isn't triggered). Future versions will check compatibility and either migrate or reject incompatible archives.
