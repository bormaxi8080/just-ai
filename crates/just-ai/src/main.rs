use {
  clap::{Parser, Subcommand},
  serde::{Deserialize, Serialize},
  serde_json::Value,
  similar::{ChangeTag, TextDiff},
  std::{
    collections::BTreeMap,
    env,
    error::Error,
    fmt::{self, Display, Formatter},
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{SystemTime, UNIX_EPOCH},
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
  #[command(about = "Ask an AI provider to suggest useful missing recipes")]
  Suggest,
  #[command(about = "Ask an AI provider to explain a recipe")]
  Explain {
    #[arg(help = "Recipe name or namepath to explain")]
    recipe: String,
  },
  #[command(about = "Ask an AI provider to propose a new recipe")]
  Add {
    #[arg(help = "Natural-language task description")]
    request: String,
    #[arg(long, help = "Apply the generated recipe after validation")]
    write: bool,
  },
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
    Commands::Suggest => {
      let response = AiClient::from_env()?.complete_json::<SuggestResponse>(
        "Suggest useful missing just recipes for this project.",
        &suggest_prompt(&context)?,
      )?;
      print_suggestions(&response);
    }
    Commands::Explain { recipe } => {
      let selected = context
        .find_recipe(&recipe)
        .ok_or_else(|| format!("recipe `{recipe}` not found"))?;
      let response = AiClient::from_env()?.complete_json::<ExplainResponse>(
        "Explain a just recipe using the supplied project context.",
        &explain_prompt(&context, selected)?,
      )?;
      print_explanation(&response);
    }
    Commands::Add { request, write } => {
      let response = AiClient::from_env()?.complete_json::<AddRecipeResponse>(
        "Generate a safe just recipe proposal as strict JSON.",
        &add_prompt(&context, &request)?,
      )?;
      handle_add(&cli.just_binary, &context, &request, response, write)?;
    }
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

  fn find_recipe(&self, needle: &str) -> Option<&ContextRecipe> {
    self
      .recipes
      .iter()
      .find(|recipe| recipe.namepath == needle || recipe.name == needle)
  }

  fn has_recipe(&self, name: &str) -> bool {
    self
      .recipes
      .iter()
      .any(|recipe| recipe.name == name || recipe.namepath == name)
  }

  fn root_source(&self) -> Option<&Path> {
    self.modules.first().map(|module| module.source.as_path())
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

impl<'de> Deserialize<'de> for RiskLevel {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    let value = String::deserialize(deserializer)?;
    match value.as_str() {
      "low" => Ok(Self::Low),
      "medium" => Ok(Self::Medium),
      "high" => Ok(Self::High),
      "blocked" => Ok(Self::Blocked),
      _ => Err(serde::de::Error::custom(format!(
        "unknown risk level `{value}`"
      ))),
    }
  }
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

#[derive(Debug)]
struct AiClient {
  api_key: Option<String>,
  base_url: String,
  curl: String,
  model: String,
}

impl AiClient {
  fn from_env() -> Result<Self, Box<dyn Error>> {
    let provider = env::var("JUST_AI_PROVIDER").unwrap_or_else(|_| "openai".to_owned());
    let base_url = env::var("JUST_AI_BASE_URL").unwrap_or_else(|_| match provider.as_str() {
      "ollama" => "http://localhost:11434/v1".to_owned(),
      _ => "https://api.openai.com/v1".to_owned(),
    });
    let model = env::var("JUST_AI_MODEL").unwrap_or_else(|_| match provider.as_str() {
      "ollama" => "llama3.1".to_owned(),
      _ => "gpt-5-mini".to_owned(),
    });
    let api_key = env::var("JUST_AI_API_KEY").ok();
    let curl = env::var("JUST_AI_CURL").unwrap_or_else(|_| "curl".to_owned());

    if provider != "ollama" && api_key.is_none() {
      return Err("JUST_AI_API_KEY is required unless JUST_AI_PROVIDER=ollama is used".into());
    }

    Ok(Self {
      api_key,
      base_url,
      curl,
      model,
    })
  }

  fn complete_json<T>(&self, system: &str, user: &str) -> Result<T, Box<dyn Error>>
  where
    T: for<'de> Deserialize<'de>,
  {
    let content = self.complete(system, user)?;
    let content = strip_json_fence(content.trim());
    Ok(serde_json::from_str(content)?)
  }

  fn complete(&self, system: &str, user: &str) -> Result<String, Box<dyn Error>> {
    let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
      "model": self.model,
      "messages": [
        {
          "role": "system",
          "content": system
        },
        {
          "role": "user",
          "content": user
        }
      ],
      "response_format": {
        "type": "json_object"
      }
    });

    let mut command = Command::new(&self.curl);
    command
      .args([
        "--fail",
        "--silent",
        "--show-error",
        "--request",
        "POST",
        "--header",
        "Content-Type: application/json",
      ])
      .arg("--data")
      .arg(body.to_string())
      .arg(url);

    if let Some(api_key) = &self.api_key {
      command.args(["--header", &format!("Authorization: Bearer {api_key}")]);
    }

    let output = command.output()?;

    if !output.status.success() {
      return Err(
        AiError {
          status: output.status.to_string(),
          stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        }
        .into(),
      );
    }

    let response: ChatCompletionResponse = serde_json::from_slice(&output.stdout)?;
    response
      .choices
      .into_iter()
      .next()
      .map(|choice| choice.message.content)
      .ok_or_else(|| "AI provider returned no choices".into())
  }
}

#[derive(Debug)]
struct AiError {
  status: String,
  stderr: String,
}

impl Display for AiError {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    if self.stderr.is_empty() {
      write!(f, "AI provider request failed with {}", self.status)
    } else {
      write!(
        f,
        "AI provider request failed with {}: {}",
        self.status, self.stderr
      )
    }
  }
}

impl Error for AiError {}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
  choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
  message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
  content: String,
}

fn strip_json_fence(content: &str) -> &str {
  content
    .strip_prefix("```json")
    .or_else(|| content.strip_prefix("```"))
    .and_then(|content| content.strip_suffix("```"))
    .map(str::trim)
    .unwrap_or(content)
}

#[derive(Debug, Deserialize)]
struct SuggestResponse {
  recommendations: Vec<SuggestRecommendation>,
  summary: String,
}

#[derive(Debug, Deserialize)]
struct SuggestRecommendation {
  body: Vec<String>,
  name: String,
  rationale: String,
  risk: RiskLevel,
}

#[derive(Debug, Deserialize)]
struct ExplainResponse {
  dependencies: Vec<String>,
  explanation: String,
  parameters: Vec<String>,
  risks: Vec<String>,
  summary: String,
}

#[derive(Debug, Deserialize)]
struct AddRecipeResponse {
  rationale: Vec<String>,
  recipe: RecipeProposal,
  summary: String,
}

#[derive(Debug, Deserialize)]
struct RecipeProposal {
  body: Vec<String>,
  #[serde(default)]
  dependencies: Vec<String>,
  doc: Option<String>,
  name: String,
  #[serde(default)]
  parameters: Vec<RecipeParameterProposal>,
}

#[derive(Debug, Deserialize)]
struct RecipeParameterProposal {
  #[serde(default)]
  default: Option<String>,
  name: String,
}

fn suggest_prompt(context: &ProjectContext) -> Result<String, Box<dyn Error>> {
  Ok(format!(
    "\
Return strict JSON with this exact shape:
{{
  \"summary\": \"short summary\",
  \"recommendations\": [
    {{
      \"name\": \"recipe-name\",
      \"body\": [\"command line\"],
      \"rationale\": \"why this recipe is useful\",
      \"risk\": \"low|medium|high|blocked\"
    }}
  ]
}}

Recommend at most five missing just recipes. Prefer practical repository
workflows such as test, lint, fmt, build, ci, dev, clean, coverage, or docs.
Do not recommend recipes that already exist. Do not include markdown.

Project context:
{}",
    serde_json::to_string_pretty(context)?
  ))
}

fn explain_prompt(
  context: &ProjectContext,
  recipe: &ContextRecipe,
) -> Result<String, Box<dyn Error>> {
  Ok(format!(
    "\
Return strict JSON with this exact shape:
{{
  \"summary\": \"one sentence\",
  \"explanation\": \"clear explanation\",
  \"parameters\": [\"parameter explanation\"],
  \"dependencies\": [\"dependency explanation\"],
  \"risks\": [\"risk explanation\"]
}}

Explain only the selected recipe. Do not include markdown.

Selected recipe:
{}

Project context:
{}",
    serde_json::to_string_pretty(recipe)?,
    serde_json::to_string_pretty(context)?
  ))
}

fn add_prompt(context: &ProjectContext, request: &str) -> Result<String, Box<dyn Error>> {
  Ok(format!(
    "\
Return strict JSON with this exact shape:
{{
  \"summary\": \"short summary\",
  \"recipe\": {{
    \"name\": \"recipe-name\",
    \"doc\": \"short doc comment or null\",
    \"parameters\": [
      {{
        \"name\": \"PARAMETER\",
        \"default\": \"optional default or null\"
      }}
    ],
    \"dependencies\": [\"existing dependency recipe name\"],
    \"body\": [\"command line\"]
  }},
  \"rationale\": [\"reason\"]
}}

Generate exactly one just recipe for the user request. Use plain just syntax
concepts only. Do not include markdown. Prefer low-risk commands. Reuse
existing recipes as dependencies only when it is clearly useful.

User request:
{request}

Project context:
{}",
    serde_json::to_string_pretty(context)?
  ))
}

fn print_suggestions(response: &SuggestResponse) {
  println!("{}", response.summary);

  for recommendation in &response.recommendations {
    println!();
    println!("{} [{}]", recommendation.name, recommendation.risk);
    println!("  {}", recommendation.rationale);
    for line in &recommendation.body {
      println!("  > {line}");
    }
  }
}

fn print_explanation(response: &ExplainResponse) {
  println!("{}", response.summary);
  println!();
  println!("{}", response.explanation);

  print_section("Parameters", &response.parameters);
  print_section("Dependencies", &response.dependencies);
  print_section("Risks", &response.risks);
}

fn print_section(heading: &str, items: &[String]) {
  if items.is_empty() {
    return;
  }

  println!();
  println!("{heading}:");
  for item in items {
    println!("  - {item}");
  }
}

fn handle_add(
  just_binary: &Path,
  context: &ProjectContext,
  request: &str,
  response: AddRecipeResponse,
  write: bool,
) -> Result<(), Box<dyn Error>> {
  validate_proposal(context, &response.recipe)?;

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = fs::read_to_string(source)?;
  let recipe = render_recipe(&response.recipe);
  let proposed = append_recipe(&original, &recipe);
  validate_justfile(just_binary, source, &proposed)?;

  let risks = RiskFinding::scan_lines(&response.recipe.body);
  let risk = RiskLevel::highest(&risks);
  if risk == RiskLevel::Blocked {
    return Err("generated recipe has blocked risk and will not be written".into());
  }

  println!("{}", response.summary);
  println!();
  println!("Request: {request}");
  println!("Recipe: {} [{}]", response.recipe.name, risk);

  print_section("Rationale", &response.rationale);

  if !risks.is_empty() {
    println!();
    println!("Risk findings:");
    for finding in &risks {
      println!("  - {}: `{}`", finding.reason, finding.line);
    }
  }

  println!();
  println!("{}", unified_diff(source, &original, &proposed));

  if write {
    fs::write(source, proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this recipe.");
  }

  Ok(())
}

fn validate_proposal(
  context: &ProjectContext,
  proposal: &RecipeProposal,
) -> Result<(), Box<dyn Error>> {
  if proposal.name.is_empty() {
    return Err("generated recipe name is empty".into());
  }

  if proposal.body.is_empty() {
    return Err("generated recipe body is empty".into());
  }

  if context.has_recipe(&proposal.name) {
    return Err(format!("recipe `{}` already exists", proposal.name).into());
  }

  if !proposal
    .name
    .chars()
    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
  {
    return Err(
      format!(
        "recipe name `{}` contains unsupported characters",
        proposal.name
      )
      .into(),
    );
  }

  for dependency in &proposal.dependencies {
    if !context.has_recipe(dependency) {
      return Err(format!("dependency recipe `{dependency}` does not exist").into());
    }
  }

  Ok(())
}

fn render_recipe(proposal: &RecipeProposal) -> String {
  let mut rendered = String::new();

  if let Some(doc) = &proposal.doc {
    rendered.push_str("# ");
    rendered.push_str(doc.trim());
    rendered.push('\n');
  }

  rendered.push_str(&proposal.name);

  for parameter in &proposal.parameters {
    rendered.push(' ');
    rendered.push_str(&parameter.name);
    if let Some(default) = &parameter.default {
      rendered.push_str("='");
      rendered.push_str(&default.replace('\'', "\\'"));
      rendered.push('\'');
    }
  }

  if !proposal.dependencies.is_empty() {
    rendered.push_str(": ");
    rendered.push_str(
      &proposal
        .dependencies
        .iter()
        .map(|dependency| format!("({dependency})"))
        .collect::<Vec<_>>()
        .join(" "),
    );
  } else {
    rendered.push(':');
  }

  rendered.push('\n');

  for line in &proposal.body {
    rendered.push_str("  ");
    rendered.push_str(line);
    rendered.push('\n');
  }

  rendered
}

fn append_recipe(original: &str, recipe: &str) -> String {
  let mut proposed = original.to_owned();

  if !proposed.ends_with('\n') {
    proposed.push('\n');
  }

  proposed.push('\n');
  proposed.push_str(recipe);
  proposed
}

fn validate_justfile(
  just_binary: &Path,
  source: &Path,
  proposed: &str,
) -> Result<(), Box<dyn Error>> {
  let temp_path = temporary_justfile_path(source)?;
  fs::write(&temp_path, proposed)?;

  let output = Command::new(just_binary)
    .arg("--justfile")
    .arg(&temp_path)
    .args(["--dump", "--dump-format", "json"])
    .output();

  let remove_result = fs::remove_file(&temp_path);
  let output = output?;

  if let Err(err) = remove_result {
    return Err(format!("failed to remove temporary justfile: {err}").into());
  }

  if !output.status.success() {
    return Err(
      DumpError {
        status: output.status.to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
      }
      .into(),
    );
  }

  Ok(())
}

fn temporary_justfile_path(source: &Path) -> Result<PathBuf, Box<dyn Error>> {
  let directory = source.parent().unwrap_or_else(|| Path::new("."));
  let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
  Ok(directory.join(format!(".just-ai-{nanos}.justfile")))
}

fn unified_diff(path: &Path, original: &str, proposed: &str) -> String {
  let diff = TextDiff::from_lines(original, proposed);
  let mut rendered = String::new();

  rendered.push_str(&format!("--- {}\n", path.display()));
  rendered.push_str(&format!("+++ {}\n", path.display()));

  for change in diff.iter_all_changes() {
    let sign = match change.tag() {
      ChangeTag::Delete => "-",
      ChangeTag::Insert => "+",
      ChangeTag::Equal => " ",
    };
    rendered.push_str(sign);
    rendered.push_str(change.value());
  }

  rendered
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

  #[test]
  fn renders_recipe_proposal() {
    let recipe = render_recipe(&RecipeProposal {
      body: vec!["cargo test".into()],
      dependencies: Vec::new(),
      doc: Some("Run tests".into()),
      name: "test-all".into(),
      parameters: vec![RecipeParameterProposal {
        default: Some("all".into()),
        name: "SCOPE".into(),
      }],
    });

    assert_eq!(recipe, "# Run tests\ntest-all SCOPE='all':\n  cargo test\n");
  }

  #[test]
  fn strips_json_fence() {
    assert_eq!(
      strip_json_fence("```json\n{\"ok\":true}\n```"),
      "{\"ok\":true}"
    );
  }
}
