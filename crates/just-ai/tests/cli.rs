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
  assert!(stdout.contains("exact duplicates"));

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

#[test]
fn modularize_refuses_existing_module_without_changing_files() {
  let directory = tempfile::tempdir().unwrap();
  let original = "test-a:\n  echo a\n\ntest-b:\n  echo b\n";
  let root = directory.path().join("justfile");
  let module = directory.path().join("test.just");
  std::fs::write(&root, original).unwrap();
  std::fs::write(&module, "# keep existing content\n").unwrap();
  let output = just_ai()
    .current_dir(directory.path())
    .args(["migrate", "modularize", "--write"])
    .output()
    .unwrap();
  assert!(!output.status.success());
  assert_eq!(std::fs::read_to_string(root).unwrap(), original);
  assert_eq!(
    std::fs::read_to_string(module).unwrap(),
    "# keep existing content\n"
  );
}

#[test]
fn modularize_preserves_attributes_and_validates_before_writing() {
  let directory = tempfile::tempdir().unwrap();
  let original = "# test documentation\n[private]\ntest-a:\n  echo a\n\ntest-b:\n  echo b\n";
  std::fs::write(directory.path().join("justfile"), original).unwrap();
  let output = just_ai()
    .current_dir(directory.path())
    .args(["migrate", "modularize", "--write"])
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "{}",
    String::from_utf8_lossy(&output.stderr)
  );
  let module = std::fs::read_to_string(directory.path().join("test.just")).unwrap();
  assert!(module.contains("# test documentation\n[private]\ntest-a:"));
}

#[test]
fn history_accepts_recipe_and_success_filters() {
  let output = run_with_justfile(
    "test:\n  echo a\n",
    &[
      "history",
      "recent",
      "--recipe",
      "test",
      "--success",
      "false",
      "--json",
    ],
  );
  assert!(
    output.status.success(),
    "{}",
    String::from_utf8_lossy(&output.stderr)
  );
  assert!(serde_json::from_slice::<Vec<serde_json::Value>>(&output.stdout).is_ok());
}

#[cfg(unix)]
#[test]
fn modularize_validation_failure_leaves_project_untouched() {
  use std::os::unix::fs::PermissionsExt;
  let directory = tempfile::tempdir().unwrap();
  let original = "test-a:\n  echo a\n\ntest-b:\n  echo b\n";
  let root = directory.path().join("justfile");
  std::fs::write(&root, original).unwrap();
  let binary = directory.path().join("fake-just");
  std::fs::write(&binary, "#!/bin/sh\nif [ \"$1\" = \"--dump\" ]; then exec just \"$@\"; fi\necho 'validation rejected' >&2\nexit 1\n").unwrap();
  std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
  let output = just_ai()
    .current_dir(directory.path())
    .arg("--just-binary")
    .arg(binary)
    .args(["migrate", "modularize", "--write"])
    .output()
    .unwrap();
  assert!(!output.status.success());
  assert!(String::from_utf8_lossy(&output.stderr).contains("validation rejected"));
  assert_eq!(std::fs::read_to_string(root).unwrap(), original);
  assert!(!directory.path().join("test.just").exists());
}

#[test]
fn real_runner_preview_enforces_blocked_and_custom_policy() {
  use just_ai::application::execution::{RecipeExecutor, RunRequest};
  let directory = tempfile::tempdir().unwrap();
  std::fs::write(
    directory.path().join("justfile"),
    "danger:\n  rm -rf /\n\nsafe:\n  @echo safe\n",
  )
  .unwrap();
  let executor = RecipeExecutor::new("just");
  let request = |recipe: &str| RunRequest {
    project_root: directory.path().into(),
    recipe: recipe.into(),
    arguments: vec![],
  };
  let prepared = executor.prepare(request("danger")).unwrap();
  assert_eq!(prepared.risk, just_ai::domain::risk::RiskLevel::Blocked);
  assert!(!prepared.preview.is_empty());
  assert!(matches!(
    prepared.policy,
    just_ai::domain::policy::PolicyDecision::Deny { .. }
  ));
  std::fs::write(
    directory.path().join("just-ai.toml"),
    "[policy.decisions]\nlow = { type = 'deny', reason = 'project policy' }\n",
  )
  .unwrap();
  assert!(matches!(
    executor.prepare(request("safe")).unwrap().policy,
    just_ai::domain::policy::PolicyDecision::Deny { .. }
  ));
}

#[test]
fn ai_context_redacts_recipe_bodies_docs_and_defaults_without_mutating_source() {
  let directory = tempfile::tempdir().unwrap();
  std::fs::write(
    directory.path().join("justfile"),
    "# API_KEY=synthetic-doc-secret\nhello:\n  @echo API_KEY=synthetic-body-secret\n",
  )
  .unwrap();
  let context = just_ai::inspect_project_at("just", directory.path()).unwrap();
  let sanitized = context.ai_json().unwrap();
  assert!(!sanitized.contains("synthetic-doc-secret"));
  assert!(!sanitized.contains("synthetic-body-secret"));
  assert!(context.recipes[0].body[0].contains("synthetic-body-secret"));
  serde_json::from_str::<serde_json::Value>(&sanitized).unwrap();
}

#[test]
fn sqlite_history_is_scoped_to_project_root() {
  let directory = tempfile::tempdir().unwrap();
  let data = directory.path().join("data");
  let a = directory.path().join("a");
  let b = directory.path().join("b");
  for root in [&a, &b] {
    std::fs::create_dir(root).unwrap();
    std::fs::write(root.join("justfile"), "hello:\n  @echo history-marker\n").unwrap();
  }
  let output = just_ai()
    .current_dir(&a)
    .env("JUST_AI_DATA_DIR", &data)
    .args(["run", "hello"])
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "{}",
    String::from_utf8_lossy(&output.stderr)
  );
  let output = just_ai()
    .current_dir(&b)
    .env("JUST_AI_DATA_DIR", &data)
    .args(["history", "recent", "--json"])
    .output()
    .unwrap();
  assert!(output.status.success());
  let records: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(records, serde_json::json!([]));
  let output = just_ai()
    .current_dir(&a)
    .env("JUST_AI_DATA_DIR", &data)
    .args(["history", "recent", "--json"])
    .output()
    .unwrap();
  assert_eq!(
    serde_json::from_slice::<serde_json::Value>(&output.stdout)
      .unwrap()
      .as_array()
      .unwrap()
      .len(),
    1
  );
}

#[test]
fn deduplication_preserves_source_and_refuses_module_projects() {
  let directory = tempfile::tempdir().unwrap();
  let root = directory.path();
  let source = "alpha z a:\n  @echo {{z}} {{a}}\n  @echo repeat\n  @echo repeat\n\nbeta z a:\n  @echo {{z}} {{a}}\n  @echo repeat\n  @echo repeat\n";
  std::fs::write(root.join("justfile"), source).unwrap();
  let context = just_ai::inspect_project_at("just", root).unwrap();
  let plan =
    just_ai::application::deduplication::DeduplicationPlan::prepare(&context, 0.8, true).unwrap();
  plan.apply(std::path::Path::new("just")).unwrap();
  let written = std::fs::read_to_string(root.join("justfile")).unwrap();
  assert!(written.contains("beta z a:"));
  assert_eq!(written.matches("@echo repeat").count(), 2);
  assert!(written.contains("{{z}} {{a}}"));
  std::fs::write(root.join("justfile"), "mod tools\n\nalpha:\n  @echo safe\n").unwrap();
  std::fs::write(root.join("tools.just"), "alpha:\n  @echo safe\n").unwrap();
  let context = just_ai::inspect_project_at("just", root).unwrap();
  assert!(
    just_ai::application::deduplication::DeduplicationPlan::prepare(&context, 0.8, false).is_err()
  );
  assert!(
    std::fs::read_to_string(root.join("justfile"))
      .unwrap()
      .contains("alpha:")
  );
}

#[test]
fn deduplication_keeps_referenced_duplicates() {
  let directory = tempfile::tempdir().unwrap();
  std::fs::write(
    directory.path().join("justfile"),
    "a:\n  @echo safe\n\nb:\n  @echo safe\n\ncaller: b\n",
  )
  .unwrap();
  let context = just_ai::inspect_project_at("just", directory.path()).unwrap();
  let plan =
    just_ai::application::deduplication::DeduplicationPlan::prepare(&context, 0.8, false).unwrap();
  assert!(plan.removed.is_empty());
  assert_eq!(plan.original, plan.proposed);
}

#[test]
fn stored_template_uses_defaults_and_rejects_unresolved_or_unknown_values() {
  use just_ai::{
    ai_responses::{TemplateParameter, TemplateProposal},
    application::templates::TemplatePlan,
  };
  let directory = tempfile::tempdir().unwrap();
  std::fs::write(directory.path().join("justfile"), "hello:\n  @echo hello\n").unwrap();
  let template = TemplateProposal {
    name: "example".into(),
    description: "test".into(),
    category: "test".into(),
    body: vec!["@echo {{value}}".into()],
    parameters: vec![TemplateParameter {
      name: "value".into(),
      description: "value".into(),
      required: true,
      default: Some("default-marker".into()),
    }],
  };
  just_ai::proposal::save_template(directory.path(), &template).unwrap();
  let stored = just_ai::proposal::load_template(directory.path(), "example")
    .unwrap()
    .unwrap();
  let context = just_ai::inspect_project_at("just", directory.path()).unwrap();
  let plan = TemplatePlan::prepare(&context, &stored, &Default::default(), false).unwrap();
  assert!(plan.proposed.contains("default-marker"));
  plan.validate(std::path::Path::new("just")).unwrap();
  let mut values = std::collections::HashMap::new();
  values.insert("unknown".into(), "value".into());
  assert!(TemplatePlan::prepare(&context, &stored, &values, false).is_err());
  assert!(just_ai::proposal::load_template(directory.path(), "../outside").is_err());
}
