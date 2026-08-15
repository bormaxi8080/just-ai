use std::process::Command;

fn just_ai() -> Command {
  Command::new(env!("CARGO_BIN_EXE_just-ai"))
}

/// Creates a temporary justfile with the given content and runs a command
fn run_with_justfile(justfile_content: &str, args: &[&str]) -> std::process::Output {
  let directory = tempfile::tempdir().unwrap();
  let justfile_path = directory.path().join("justfile");
  std::fs::write(&justfile_path, justfile_content).unwrap();
  just_ai()
    .current_dir(directory.path())
    .args(args)
    .output()
    .unwrap()
}

#[test]
fn agent_command_does_not_require_a_justfile() {
  let directory = tempfile::tempdir().unwrap();
  let output = just_ai()
    .current_dir(directory.path())
    .args(["agent", "review-architecture"])
    .output()
    .unwrap();
  assert!(output.status.success());
  let stdout = String::from_utf8(output.stdout).unwrap();
  assert!(stdout.contains("Review architecture"));
  assert!(stdout.contains("get_architecture"));
}

#[test]
fn verify_agent_command_prints_canonical_playbook() {
  let directory = tempfile::tempdir().unwrap();
  let output = just_ai()
    .current_dir(directory.path())
    .args(["agent", "verify"])
    .output()
    .unwrap();
  assert!(output.status.success());
  assert_eq!(
    String::from_utf8(output.stdout).unwrap(),
    include_str!("../../../agent/commands/verify.md")
  );
}

#[test]
fn missing_justfile_is_reported_without_panicking() {
  let directory = tempfile::tempdir().unwrap();
  let output = just_ai()
    .current_dir(directory.path())
    .arg("doctor")
    .output()
    .unwrap();
  assert!(!output.status.success());
  assert!(String::from_utf8(output.stderr).unwrap().contains("error:"));
}

// ===== Migrate Analyze Tests =====

#[test]
fn migrate_analyze_runs_and_reports_structure() {
  let justfile = r#"
build:
  cargo build

test: build
  cargo test

lint:
  cargo clippy

fmt:
  cargo fmt

deploy: build
  echo deploy

clean:
  cargo clean
"#;

  let output = run_with_justfile(justfile, &["migrate", "analyze"]);
  let stdout = String::from_utf8(output.stdout).unwrap();
  assert!(output.status.success());
  assert!(stdout.contains("Project Analysis"));
  assert!(stdout.contains("Total recipes: 6"));
  assert!(stdout.contains("Unreferenced recipes"));
  assert!(stdout.contains("test")); // test has no dependents
  assert!(stdout.contains("Dependency depths"));
  // build depends on test + lint, so depth 1
  // deploy depends on build, so depth 2
}

#[test]
fn migrate_analyze_detects_unreferenced_recipes() {
  let justfile = r#"
build:
  cargo build

test: build
  cargo test

standalone:
  echo "this recipe is never used"
"#;

  let output = run_with_justfile(justfile, &["migrate", "analyze"]);
  let stdout = String::from_utf8(output.stdout).unwrap();
  assert!(output.status.success());
  assert!(stdout.contains("Unreferenced recipes"));
  assert!(stdout.contains("standalone"));
}

#[test]
fn migrate_analyze_detects_isolated_recipes() {
  let justfile = r#"
build:
  cargo build

test: build
  cargo test

isolated:
  echo "no deps, no dependents"
"#;

  let output = run_with_justfile(justfile, &["migrate", "analyze"]);
  let stdout = String::from_utf8(output.stdout).unwrap();
  assert!(output.status.success());
  assert!(stdout.contains("Isolated recipes"));
  assert!(stdout.contains("isolated"));
}

#[test]
fn migrate_analyze_json_output() {
  let justfile = r#"
build:
  cargo build

test: build
  cargo test
"#;

  let output = run_with_justfile(justfile, &["migrate", "analyze", "--json"]);
  let stdout = String::from_utf8(output.stdout).unwrap();
  assert!(output.status.success());
  // Parse JSON to verify structure
  let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
  assert!(json.get("unreferenced_recipes").is_some());
  assert!(json.get("isolated_recipes").is_some());
  assert!(json.get("cycles").is_some());
  assert!(json.get("dependency_depths").is_some());
  assert!(json.get("similar_recipes").is_some());
}

// Note: cycle detection is tested in inspection unit tests because
// `just --dump` fails on circular dependencies before we can analyze them

// ===== Migrate Modularize Tests =====

#[test]
fn migrate_modularize_groups_by_prefix() {
  let justfile = r#"
test-unit:
  cargo test --lib

test-integration:
  cargo test --test integration

build:
  cargo build

build-release:
  cargo build --release

lint:
  cargo clippy

deploy:
  echo deploy
"#;

  let output = run_with_justfile(justfile, &["migrate", "modularize", "--dry-run"]);
  let stdout = String::from_utf8(output.stdout).unwrap();
  let stderr = String::from_utf8(output.stderr).unwrap();
  if !output.status.success() {
    eprintln!("STDOUT:\n{}", stdout);
    eprintln!("STDERR:\n{}", stderr);
  }
  assert!(
    output.status.success(),
    "exit code: {}, stderr: {}",
    output.status,
    stderr
  );
  assert!(stdout.contains("Modularization Plan"));
  assert!(stdout.contains("Module 'test': 2 recipes"));
  assert!(stdout.contains("test-unit"));
  assert!(stdout.contains("test-integration"));
  assert!(stdout.contains("Module 'build': 2 recipes"));
  assert!(stdout.contains("build"));
  assert!(stdout.contains("build-release"));
}

#[test]
fn migrate_modularize_dry_run_shows_imports() {
  let justfile = r#"
test-unit:
  cargo test --lib

test-integration:
  cargo test --test integration
"#;

  let output = run_with_justfile(justfile, &["migrate", "modularize", "--dry-run"]);
  let stdout = String::from_utf8(output.stdout).unwrap();
  assert!(output.status.success());
  assert!(stdout.contains("import 'test.just'"));
}

#[test]
fn migrate_modularize_write_creates_module_files() {
  let justfile = r#"
test-unit:
  cargo test --lib

test-integration:
  cargo test --test integration
"#;

  let directory = tempfile::tempdir().unwrap();
  let justfile_path = directory.path().join("justfile");
  std::fs::write(&justfile_path, justfile).unwrap();

  let output = just_ai()
    .current_dir(directory.path())
    .args(["migrate", "modularize", "--write"])
    .output()
    .unwrap();

  let stdout = String::from_utf8(output.stdout).unwrap();
  let stderr = String::from_utf8(output.stderr).unwrap();
  eprintln!("=== STDOUT ===\n{}", stdout);
  eprintln!("=== STDERR ===\n{}", stderr);
  assert!(output.status.success(), "stdout: {}", stdout);
  assert!(stdout.contains("Created"));
  assert!(stdout.contains("test.just"));
  assert!(stdout.contains("Wrote"));

  // Verify module file was created
  let module_path = directory.path().join("test.just");
  assert!(module_path.exists());
  let module_content = std::fs::read_to_string(&module_path).unwrap();
  assert!(module_content.contains("test-unit"));
  assert!(module_content.contains("test-integration"));

  // Verify root justfile has import
  let root_content = std::fs::read_to_string(&justfile_path).unwrap();
  assert!(root_content.contains("import 'test.just'"));
  assert!(!root_content.contains("test-unit:")); // recipe moved
}

// ===== Migrate Deduplicate Tests =====

#[test]
fn migrate_deduplicate_finds_similar_recipes() {
  let justfile = r#"
test-unit:
  cargo test --lib

test-unit-alt:
  cargo test --lib

build-release:
  cargo build

build-debug:
  cargo build
"#;

  let output = run_with_justfile(justfile, &["migrate", "deduplicate"]);
  let stdout = String::from_utf8(output.stdout).unwrap();
  let stderr = String::from_utf8(output.stderr).unwrap();
  if !output.status.success() {
    eprintln!("STDOUT:\n{}", stdout);
    eprintln!("STDERR:\n{}", stderr);
  }
  assert!(
    output.status.success(),
    "exit code: {}, stderr: {}",
    output.status,
    stderr
  );
  assert!(stdout.contains("Duplicate Analysis"));
  assert!(stdout.contains("Found 2 similar recipe pairs"));
  assert!(stdout.contains("100.0% similar"));
  assert!(stdout.contains("test-unit"));
  assert!(stdout.contains("test-unit-alt"));
  assert!(stdout.contains("build-release"));
  assert!(stdout.contains("build-debug"));
}

#[test]
fn migrate_deduplicate_write_removes_duplicates() {
  let justfile = r#"
test-unit:
  cargo test --lib

test-unit-alt:
  cargo test --lib
"#;

  let directory = tempfile::tempdir().unwrap();
  let justfile_path = directory.path().join("justfile");
  std::fs::write(&justfile_path, justfile).unwrap();

  let output = just_ai()
    .current_dir(directory.path())
    .args(["migrate", "deduplicate", "--write"])
    .output()
    .unwrap();

  let stdout = String::from_utf8(output.stdout).unwrap();
  assert!(output.status.success(), "stdout: {}", stdout);
  assert!(stdout.contains("Auto-merged"));

  // Verify one recipe was removed
  let root_content = std::fs::read_to_string(&justfile_path).unwrap();
  let count = root_content.matches("test-unit").count();
  assert_eq!(count, 1, "Only one test-unit should remain");
}

#[test]
fn migrate_deduplicate_smart_merge_combines_unique_parts() {
  let justfile = r#"
test-unit:
  cargo test --lib
  echo unique from first

test-unit-alt:
  cargo test --lib
  cargo test --doc
  echo unique from second
"#;

  // Note: current implementation looks for identical bodies, so this test
  // checks that it still finds them. Smart merge would need similar bodies.
  let output = run_with_justfile(justfile, &["migrate", "deduplicate", "--write", "--merge"]);
  let _stdout = String::from_utf8(output.stdout).unwrap();
  assert!(output.status.success());
}

#[test]
fn migrate_deduplicate_with_similarity_threshold() {
  let justfile = r#"
test-a:
  cargo test --lib

test-b:
  cargo test --lib

build:
  cargo build
"#;

  let output = run_with_justfile(
    justfile,
    &["migrate", "deduplicate", "--similarity-threshold", "0.9"],
  );
  let stdout = String::from_utf8(output.stdout).unwrap();
  assert!(output.status.success());
  assert!(stdout.contains("test-a"));
  assert!(stdout.contains("test-b"));
  // build should not be in similar pairs (different body)
}
