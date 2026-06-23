use {
  clap::{Parser, Subcommand},
  serde::{Deserialize, Serialize},
  serde_json::Value,
  std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
    path::PathBuf,
    process::{Command, ExitCode},
  },
};

#[derive(Debug, Parser)]
#[command(
  name = "just-ai",
  about = "AI-oriented companion utilities for justfiles"
)]
struct Cli {
  #[arg(
    long,
    env = "JUST_AI_JUST_BINARY",
    default_value = "just",
    global = true,
    help = "Path to the just binary used for justfile discovery"
  )]
  just_binary: PathBuf,
  #[command(subcommand)]
  command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
  #[command(about = "Export a compact machine-readable context for AI tools")]
  ExportContext {
    #[arg(long, help = "Pretty-print JSON output")]
    pretty: bool,
  },
  #[command(about = "Analyze recipes for risky command patterns")]
  Doctor {
    #[arg(long, help = "Emit JSON instead of human-readable output")]
    json: bool,
  },
}

fn main() -> ExitCode {
  match try_main() {
    Ok(()) => ExitCode::SUCCESS,
    Err(err) => {
      eprintln!("error: {err}");
      ExitCode::FAILURE
    }
  }
}

fn try_main() -> Result<(), Box<dyn Error>> {
  let cli = Cli::parse();
  let dump = load_dump(&cli.just_binary)?;
  let context = ProjectContext::from_dump(dump);

  match cli.command {
    Commands::ExportContext { pretty } => {
      if pretty {
        println!("{}", serde_json::to_string_pretty(&context)?);
      } else {
        println!("{}", serde_json::to_string(&context)?);
      }
    }
    Commands::Doctor { json } => {
      let report = DoctorReport::from_context(&context);

      if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
      } else {
        print_doctor_report(&report);
      }

      if report.highest_risk == RiskLevel::Blocked {
        return Err("blocked-risk recipes found".into());
      }
    }
  }

  Ok(())
}

fn load_dump(just_binary: &PathBuf) -> Result<DumpModule, Box<dyn Error>> {
  let output = Command::new(just_binary)
    .args(["--dump", "--dump-format", "json"])
    .output()?;

  if !output.status.success() {
    return Err(
      DumpError {
        status: output.status.to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
      }
      .into(),
    );
  }

  Ok(serde_json::from_slice(&output.stdout)?)
}

#[derive(Debug)]
struct DumpError {
  status: String,
  stderr: String,
}

impl Display for DumpError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    if self.stderr.is_empty() {
      write!(f, "just dump failed with {}", self.status)
    } else {
      write!(f, "just dump failed with {}: {}", self.status, self.stderr)
    }
  }
}

impl Error for DumpError {}

#[derive(Debug, Deserialize)]
struct DumpModule {
  #[serde(default)]
  modules: BTreeMap<String, DumpModule>,
  #[serde(default)]
  recipes: BTreeMap<String, DumpRecipe>,
  source: PathBuf,
  #[serde(default)]
  warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DumpRecipe {
  #[serde(default)]
  body: Vec<Value>,
  #[serde(default)]
  dependencies: Vec<DumpDependency>,
  doc: Option<String>,
  name: String,
  namepath: String,
  #[serde(default)]
  parameters: Vec<DumpParameter>,
  #[serde(default)]
  private: bool,
  #[serde(default)]
  quiet: bool,
  #[serde(default)]
  shebang: bool,
}

#[derive(Debug, Deserialize)]
struct DumpDependency {
  recipe: String,
}

#[derive(Debug, Deserialize)]
struct DumpParameter {
  name: String,
  kind: String,
  #[serde(default)]
  default: Option<Value>,
}

#[derive(Debug, Serialize)]
struct ProjectContext {
  modules: Vec<ContextModule>,
  recipes: Vec<ContextRecipe>,
  warnings: Vec<String>,
}

impl ProjectContext {
  fn from_dump(dump: DumpModule) -> Self {
    let mut context = Self {
      modules: Vec::new(),
      recipes: Vec::new(),
      warnings: Vec::new(),
    };

    context.collect_module("", dump);
    context
  }

  fn collect_module(&mut self, module_path: &str, module: DumpModule) {
    self.modules.push(ContextModule {
      module_path: module_path.to_owned(),
      source: module.source,
      recipe_count: module.recipes.len(),
    });

    self.warnings.extend(module.warnings);

    for recipe in module.recipes.into_values() {
      let body = recipe.body.iter().map(render_body_line).collect::<Vec<_>>();
      let risks = RiskFinding::scan_lines(&body);
      let risk = RiskLevel::highest(&risks);

      self.recipes.push(ContextRecipe {
        body,
        dependencies: recipe
          .dependencies
          .into_iter()
          .map(|dependency| dependency.recipe)
          .collect(),
        doc: recipe.doc,
        module_path: module_path.to_owned(),
        name: recipe.name,
        namepath: recipe.namepath,
        parameters: recipe
          .parameters
          .into_iter()
          .map(|parameter| ContextParameter {
            default: parameter.default.map(|default| render_value(&default)),
            kind: parameter.kind,
            name: parameter.name,
          })
          .collect(),
        private: recipe.private,
        quiet: recipe.quiet,
        risk,
        risks,
        shebang: recipe.shebang,
      });
    }

    for (name, child) in module.modules {
      let child_path = if module_path.is_empty() {
        name
      } else {
        format!("{module_path}:{name}")
      };
      self.collect_module(&child_path, child);
    }
  }
}

#[derive(Debug, Serialize)]
struct ContextModule {
  module_path: String,
  recipe_count: usize,
  source: PathBuf,
}

#[derive(Debug, Serialize)]
struct ContextRecipe {
  body: Vec<String>,
  dependencies: Vec<String>,
  doc: Option<String>,
  module_path: String,
  name: String,
  namepath: String,
  parameters: Vec<ContextParameter>,
  private: bool,
  quiet: bool,
  risk: RiskLevel,
  risks: Vec<RiskFinding>,
  shebang: bool,
}

#[derive(Debug, Serialize)]
struct ContextParameter {
  default: Option<String>,
  kind: String,
  name: String,
}

fn render_body_line(value: &Value) -> String {
  match value {
    Value::Array(parts) => parts
      .iter()
      .map(render_fragment)
      .collect::<Vec<_>>()
      .join(""),
    Value::String(string) => string.clone(),
    other => other.to_string(),
  }
}

fn render_value(value: &Value) -> String {
  match value {
    Value::Array(_) => render_fragment(value),
    Value::String(string) => string.clone(),
    other => other.to_string(),
  }
}

fn render_fragment(value: &Value) -> String {
  match value {
    Value::String(string) => string.clone(),
    Value::Array(parts) => {
      let head = parts
        .first()
        .and_then(Value::as_str)
        .unwrap_or("expression");
      format!("{{{{{head}:...}}}}")
    }
    other => other.to_string(),
  }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
enum RiskLevel {
  Low,
  Medium,
  High,
  Blocked,
}

impl RiskLevel {
  fn highest(findings: &[RiskFinding]) -> Self {
    findings
      .iter()
      .map(|finding| finding.level)
      .max()
      .unwrap_or(Self::Low)
  }
}

impl Display for RiskLevel {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::Low => write!(f, "low"),
      Self::Medium => write!(f, "medium"),
      Self::High => write!(f, "high"),
      Self::Blocked => write!(f, "blocked"),
    }
  }
}

#[derive(Clone, Debug, Serialize)]
struct RiskFinding {
  level: RiskLevel,
  line: String,
  reason: String,
}

impl RiskFinding {
  fn scan_lines(lines: &[String]) -> Vec<Self> {
    lines
      .iter()
      .flat_map(|line| Self::scan_line(line))
      .collect()
  }

  fn scan_line(line: &str) -> Vec<Self> {
    let normalized = normalize_command(line);
    let mut findings = Vec::new();

    for (needle, reason) in [
      ("rm -rf /", "recursively removes from the filesystem root"),
      ("rm -fr /", "recursively removes from the filesystem root"),
    ] {
      if normalized.contains(needle) {
        findings.push(Self::new(RiskLevel::Blocked, line, reason));
      }
    }

    if downloads_to_shell(&normalized) {
      findings.push(Self::new(
        RiskLevel::Blocked,
        line,
        "pipes downloaded content to a shell",
      ));
    }

    for (needle, reason) in [
      ("rm -rf", "recursively removes files"),
      ("rm -fr", "recursively removes files"),
      ("sudo ", "requires elevated privileges"),
      ("docker system prune", "removes Docker data"),
      ("git clean -fd", "removes untracked files"),
      ("chmod -r", "recursively changes permissions"),
      ("chown -r", "recursively changes ownership"),
      ("mkfs", "formats a filesystem"),
      ("dd if=", "performs raw disk or byte copying"),
      ("> /dev/", "writes to a device file"),
    ] {
      if normalized.contains(needle) {
        findings.push(Self::new(RiskLevel::High, line, reason));
      }
    }

    for (needle, reason) in [
      ("cargo install", "installs executable dependencies"),
      ("npm install", "installs package dependencies"),
      ("pnpm install", "installs package dependencies"),
      ("yarn add", "installs package dependencies"),
      ("pip install", "installs package dependencies"),
      ("brew install", "installs system packages"),
      ("curl ", "downloads content from the network"),
      ("wget ", "downloads content from the network"),
      ("git push", "changes remote git state"),
      ("git pull", "changes local git state from a remote"),
      ("docker build", "builds container images"),
      ("docker compose", "runs Docker Compose"),
    ] {
      if normalized.contains(needle) {
        findings.push(Self::new(RiskLevel::Medium, line, reason));
      }
    }

    findings
  }

  fn new(level: RiskLevel, line: &str, reason: &str) -> Self {
    Self {
      level,
      line: line.to_owned(),
      reason: reason.to_owned(),
    }
  }
}

fn normalize_command(line: &str) -> String {
  line
    .to_ascii_lowercase()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
}

fn downloads_to_shell(command: &str) -> bool {
  (command.contains("curl ") || command.contains("wget "))
    && (command.contains("| sh") || command.contains("| bash"))
}

#[derive(Debug, Serialize)]
struct DoctorReport {
  blocked: usize,
  high: usize,
  highest_risk: RiskLevel,
  low: usize,
  medium: usize,
  recipes: Vec<DoctorRecipe>,
  total_recipes: usize,
}

impl DoctorReport {
  fn from_context(context: &ProjectContext) -> Self {
    let recipes = context
      .recipes
      .iter()
      .map(|recipe| DoctorRecipe {
        namepath: recipe.namepath.clone(),
        risk: recipe.risk,
        risks: recipe.risks.clone(),
      })
      .collect::<Vec<_>>();

    let total_recipes = recipes.len();
    let low = recipes
      .iter()
      .filter(|recipe| recipe.risk == RiskLevel::Low)
      .count();
    let medium = recipes
      .iter()
      .filter(|recipe| recipe.risk == RiskLevel::Medium)
      .count();
    let high = recipes
      .iter()
      .filter(|recipe| recipe.risk == RiskLevel::High)
      .count();
    let blocked = recipes
      .iter()
      .filter(|recipe| recipe.risk == RiskLevel::Blocked)
      .count();
    let highest_risk = recipes
      .iter()
      .map(|recipe| recipe.risk)
      .max()
      .unwrap_or(RiskLevel::Low);

    Self {
      blocked,
      high,
      highest_risk,
      low,
      medium,
      recipes,
      total_recipes,
    }
  }
}

#[derive(Debug, Serialize)]
struct DoctorRecipe {
  namepath: String,
  risk: RiskLevel,
  risks: Vec<RiskFinding>,
}

fn print_doctor_report(report: &DoctorReport) {
  println!(
    "Analyzed {} recipes: {} low, {} medium, {} high, {} blocked.",
    report.total_recipes, report.low, report.medium, report.high, report.blocked
  );

  for recipe in &report.recipes {
    if recipe.risk == RiskLevel::Low {
      continue;
    }

    println!();
    println!("{} [{}]", recipe.namepath, recipe.risk);
    for finding in &recipe.risks {
      println!("  - {}: `{}`", finding.reason, finding.line);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn renders_body_lines() {
    let line = serde_json::json!(["echo ", ["variable", "name"], " done"]);

    assert_eq!(render_body_line(&line), "echo {{variable:...}} done");
  }

  #[test]
  fn detects_blocked_downloaded_shell() {
    let findings = RiskFinding::scan_line("curl https://example.com/install.sh | sh");

    assert_eq!(RiskLevel::highest(&findings), RiskLevel::Blocked);
  }

  #[test]
  fn detects_high_recursive_remove() {
    let findings = RiskFinding::scan_line("rm -rf tmp/release");

    assert_eq!(RiskLevel::highest(&findings), RiskLevel::High);
  }

  #[test]
  fn detects_medium_package_install() {
    let findings = RiskFinding::scan_line("cargo install cargo-watch");

    assert_eq!(RiskLevel::highest(&findings), RiskLevel::Medium);
  }
}
