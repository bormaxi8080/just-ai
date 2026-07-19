use std::process::Command;

fn just_ai() -> Command {
  Command::new(env!("CARGO_BIN_EXE_just-ai"))
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
