//! Tests for ghost-migrate (Task 6.5).

use ghost_migrate::migrator::{MigrationResult, OpenClawMigrator};
use std::fs;
use tempfile::TempDir;

fn setup_openclaw(tmp: &TempDir) -> std::path::PathBuf {
    let oc = tmp.path().join("openclaw");
    fs::create_dir_all(&oc).unwrap();
    fs::write(
        oc.join("SOUL.md"),
        "# My Agent\n\nI am a helpful assistant.\n\n<!-- AGENT-MUTABLE -->\nThis should be stripped.\n<!-- /AGENT-MUTABLE -->\n\nEnd.",
    )
    .unwrap();

    let memories = oc.join("memories");
    fs::create_dir_all(&memories).unwrap();
    fs::write(memories.join("note.md"), "A memory entry").unwrap();

    let skills = oc.join("skills");
    fs::create_dir_all(&skills).unwrap();
    fs::write(
        skills.join("signed.yml"),
        "name: test\nsignature: abc\n-----BEGIN\ndata\n-----END",
    )
    .unwrap();
    fs::write(
        skills.join("unsigned.yml"),
        "name: unsigned_skill\naction: do_thing",
    )
    .unwrap();

    fs::write(
        oc.join("config.yml"),
        "agent:\n  name: my-agent\n  model: gpt-4",
    )
    .unwrap();

    oc
}

#[test]
fn detect_valid_openclaw_installation() {
    let tmp = TempDir::new().unwrap();
    let oc = setup_openclaw(&tmp);
    assert!(OpenClawMigrator::detect(&oc));
}

#[test]
fn detect_missing_installation() {
    let tmp = TempDir::new().unwrap();
    let missing = tmp.path().join("nonexistent");
    assert!(!OpenClawMigrator::detect(&missing));
}

#[test]
fn soul_importer_produces_valid_ghost_soul() {
    let tmp = TempDir::new().unwrap();
    let oc = setup_openclaw(&tmp);
    let target = tmp.path().join("ghost");

    let migrator = OpenClawMigrator::new(&oc, &target);
    migrator.migrate().unwrap();

    let soul = fs::read_to_string(target.join("SOUL.md")).unwrap();
    assert!(soul.contains("I am a helpful assistant"));
    assert!(!soul.contains("This should be stripped"));
}

#[test]
fn memory_importer_assigns_conservative_importance() {
    let tmp = TempDir::new().unwrap();
    let oc = setup_openclaw(&tmp);
    let target = tmp.path().join("ghost");

    let migrator = OpenClawMigrator::new(&oc, &target);
    migrator.migrate().unwrap();

    let memory = fs::read_to_string(target.join("memories/note.md")).unwrap();
    assert!(memory.contains("importance: Low"));
}

#[test]
fn skill_importer_quarantines_unsigned() {
    let tmp = TempDir::new().unwrap();
    let oc = setup_openclaw(&tmp);
    let target = tmp.path().join("ghost");

    let migrator = OpenClawMigrator::new(&oc, &target);
    let result = migrator.migrate().unwrap();

    // Unsigned skill should be in review_items
    assert!(result.review_items.iter().any(|r| r.contains("unsigned")));
    // Quarantine directory should have the unsigned skill
    assert!(target.join("skills_quarantine/unsigned.yml").exists());
}

#[test]
fn config_importer_produces_valid_ghost_yml() {
    let tmp = TempDir::new().unwrap();
    let oc = setup_openclaw(&tmp);
    let target = tmp.path().join("ghost");

    let migrator = OpenClawMigrator::new(&oc, &target);
    migrator.migrate().unwrap();

    let config = fs::read_to_string(target.join("ghost.yml")).unwrap();
    assert!(config.contains("gateway"));
}

#[test]
fn migration_result_contains_all_categories() {
    let tmp = TempDir::new().unwrap();
    let oc = setup_openclaw(&tmp);
    let target = tmp.path().join("ghost");

    let migrator = OpenClawMigrator::new(&oc, &target);
    let result = migrator.migrate().unwrap();

    assert!(!result.imported.is_empty());
}

#[test]
fn full_migration_from_mock() {
    let tmp = TempDir::new().unwrap();
    let oc = setup_openclaw(&tmp);
    let target = tmp.path().join("ghost");

    let migrator = OpenClawMigrator::new(&oc, &target);
    let result = migrator.migrate().unwrap();

    assert!(!result.is_empty());
    assert!(target.join("SOUL.md").exists());
}

#[test]
fn source_files_never_modified() {
    let tmp = TempDir::new().unwrap();
    let oc = setup_openclaw(&tmp);
    let target = tmp.path().join("ghost");

    let soul_before = fs::read_to_string(oc.join("SOUL.md")).unwrap();

    let migrator = OpenClawMigrator::new(&oc, &target);
    migrator.migrate().unwrap();

    let soul_after = fs::read_to_string(oc.join("SOUL.md")).unwrap();
    assert_eq!(soul_before, soul_after, "Source SOUL.md was modified!");
}

#[test]
fn corrupted_openclaw_graceful_error() {
    let tmp = TempDir::new().unwrap();
    let oc = tmp.path().join("openclaw");
    fs::create_dir_all(&oc).unwrap();
    // Create SOUL.md so detect passes, but corrupt config
    fs::write(oc.join("SOUL.md"), "valid soul").unwrap();
    fs::write(oc.join("config.yml"), "{{{{invalid yaml").unwrap();

    let target = tmp.path().join("ghost");
    let migrator = OpenClawMigrator::new(&oc, &target);
    let result = migrator.migrate().unwrap();
    // Should have warnings about config parse failure
    assert!(result.warnings.iter().any(|w| w.contains("config")));
}
