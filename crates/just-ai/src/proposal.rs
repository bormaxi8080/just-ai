use {
  crate::{
    ProjectContext,
    ai_responses::{
      AddRecipeResponse, ComposeWorkflowResponse, FixProposal, FixResponse,
      RecipeParameterProposal, RecipeProposal, TemplateProposal, TemplateResponse,
      WorkflowResponse,
    },
    application,
    bounded_file::{self, max_editable_file_bytes},
    bounded_output,
    cli::print_section,
    domain::risk::{RiskFinding, RiskLevel},
    inspection,
    just_dump::DumpError,
  },
  similar::{ChangeTag, TextDiff},
  std::{
    collections::HashMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
  },
};

/// Directory name for storing templates
const TEMPLATE_DIR: &str = ".just-ai/templates";

/// Current template format version
const TEMPLATE_VERSION: u32 = 1;

/// Get the template directory path for a project
fn get_template_dir(project_root: &Path) -> PathBuf {
  project_root.join(TEMPLATE_DIR)
}

/// Get a template file path
fn get_template_path(project_root: &Path, template_name: &str) -> PathBuf {
  get_template_dir(project_root).join(format!("{template_name}.json"))
}

/// Template metadata stored on disk
#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct StoredTemplate {
  name: String,
  description: String,
  category: String,
  parameters: Vec<TemplateParameter>,
  body: Vec<String>,
  created_at: u64,
  updated_at: u64,
  #[serde(default = "default_version")]
  version: u32,
}

fn default_version() -> u32 {
  1
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct TemplateParameter {
  name: String,
  description: String,
  required: bool,
  default: Option<String>,
}

/// Save a template to disk
pub fn save_template(
  project_root: &Path,
  template: &TemplateProposal,
) -> Result<(), Box<dyn Error>> {
  let dir = get_template_dir(project_root);
  fs::create_dir_all(&dir)?;

  let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

  // Load existing template to preserve created_at and version
  let path = get_template_path(project_root, &template.name);
  let existing = if path.exists() {
    let content = fs::read_to_string(&path)?;
    serde_json::from_str::<StoredTemplate>(&content).ok()
  } else {
    None
  };

  let created_at = existing.as_ref().map(|e| e.created_at).unwrap_or(now);
  let version = existing
    .as_ref()
    .map(|e| e.version + 1)
    .unwrap_or(TEMPLATE_VERSION);

  let stored = StoredTemplate {
    name: template.name.clone(),
    description: template.description.clone(),
    category: template.category.clone(),
    parameters: template
      .parameters
      .iter()
      .map(|p| TemplateParameter {
        name: p.name.clone(),
        description: p.description.clone(),
        required: p.required,
        default: p.default.clone(),
      })
      .collect(),
    body: template.body.clone(),
    created_at,
    updated_at: now,
    version,
  };

  let json = serde_json::to_string_pretty(&stored)?;
  fs::write(&path, json)?;

  Ok(())
}

/// Load a template from disk
pub fn load_template(
  project_root: &Path,
  template_name: &str,
) -> Result<Option<TemplateProposal>, Box<dyn Error>> {
  let path = get_template_path(project_root, template_name);
  if !path.exists() {
    return Ok(None);
  }

  let content = fs::read_to_string(path)?;
  let stored: StoredTemplate = serde_json::from_str(&content)?;

  // Handle migration from older versions (pre-version)
  let _version = stored.version; // Currently on version 1, reserved for future migrations

  Ok(Some(TemplateProposal {
    name: stored.name,
    description: stored.description,
    category: stored.category,
    parameters: stored
      .parameters
      .into_iter()
      .map(|p| crate::ai_responses::TemplateParameter {
        name: p.name,
        description: p.description,
        required: p.required,
        default: p.default,
      })
      .collect(),
    body: stored.body,
  }))
}

/// List all stored templates
pub fn list_templates(project_root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
  let dir = get_template_dir(project_root);
  if !dir.exists() {
    return Ok(Vec::new());
  }

  let mut templates = Vec::new();
  for entry in fs::read_dir(dir)? {
    let entry = entry?;
    let path = entry.path();
    if path.extension().and_then(|s| s.to_str()) == Some("json")
      && let Some(name) = path.file_stem().and_then(|s| s.to_str())
    {
      templates.push(name.to_string());
    }
  }
  templates.sort();
  Ok(templates)
}

/// Delete a template
pub fn delete_template(project_root: &Path, template_name: &str) -> Result<bool, Box<dyn Error>> {
  let path = get_template_path(project_root, template_name);
  if path.exists() {
    fs::remove_file(path)?;
    Ok(true)
  } else {
    Ok(false)
  }
}

/// Built-in templates shipped with just-ai
pub fn builtin_templates() -> Vec<TemplateProposal> {
  vec![
    // CI/CD Pipeline Template
    TemplateProposal {
      name: "ci-pipeline".to_string(),
      description: "Complete CI/CD pipeline with build, test, lint, and deploy stages".to_string(),
      category: "ci".to_string(),
      parameters: vec![
        crate::ai_responses::TemplateParameter {
          name: "language".to_string(),
          description: "Programming language (rust, node, python, go, java)".to_string(),
          required: true,
          default: Some("rust".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "registry".to_string(),
          description: "Container registry for docker images".to_string(),
          required: false,
          default: Some("ghcr.io".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "deploy_target".to_string(),
          description: "Deployment target (kubernetes, serverless, vm)".to_string(),
          required: false,
          default: Some("kubernetes".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "deploy_script".to_string(),
          description: "Deploy command script".to_string(),
          required: false,
          default: Some("kubectl apply -f k8s/".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "project_name".to_string(),
          description: "Project name (for Docker image)".to_string(),
          required: false,
          default: Some("app".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "version".to_string(),
          description: "Version tag (for Docker image)".to_string(),
          required: false,
          default: Some("latest".to_string()),
        },
      ],
      body: vec![
        "Build stage".to_string(),
        "build:".to_string(),
        "cargo build --release".to_string(),
        "".to_string(),
        "Test stage".to_string(),
        "test:".to_string(),
        "cargo test --all".to_string(),
        "".to_string(),
        "Lint stage".to_string(),
        "lint:".to_string(),
        "cargo clippy -- -D warnings".to_string(),
        "cargo fmt --all -- --check".to_string(),
        "".to_string(),
        "Security audit".to_string(),
        "audit:".to_string(),
        "cargo audit".to_string(),
        "".to_string(),
        "Docker build".to_string(),
        "docker-build:".to_string(),
        "docker build -t {{registry}}/{{project_name}}:{{version}} .".to_string(),
        "".to_string(),
        "Deploy stage".to_string(),
        "deploy: docker-build".to_string(),
        "{{deploy_script}}".to_string(),
      ],
    },
    // Test Matrix Template
    TemplateProposal {
      name: "test-matrix".to_string(),
      description: "Run tests across multiple versions and platforms".to_string(),
      category: "test".to_string(),
      parameters: vec![
        crate::ai_responses::TemplateParameter {
          name: "language".to_string(),
          description: "Programming language".to_string(),
          required: true,
          default: Some("rust".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "versions".to_string(),
          description: "Comma-separated version list (e.g., 1.70,1.71,stable)".to_string(),
          required: true,
          default: Some("stable,beta,nightly".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "platforms".to_string(),
          description: "Comma-separated platform list (e.g., ubuntu,macos,windows)".to_string(),
          required: false,
          default: Some("ubuntu-latest,macos-latest,windows-latest".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "test_command".to_string(),
          description: "Test command to run".to_string(),
          required: false,
          default: Some("cargo test".to_string()),
        },
      ],
      body: vec![
        "Test matrix for {{language}}".to_string(),
        "test-matrix:".to_string(),
        "  echo \"Testing {{language}} {{versions}} on {{platforms}}\"".to_string(),
        "  {{test_command}}".to_string(),
      ],
    },
    // Release Template
    TemplateProposal {
      name: "release".to_string(),
      description: "Automated release workflow with version bump, changelog, and tagging"
        .to_string(),
      category: "release".to_string(),
      parameters: vec![
        crate::ai_responses::TemplateParameter {
          name: "version_type".to_string(),
          description: "Version bump type (patch, minor, major)".to_string(),
          required: true,
          default: Some("patch".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "changelog_tool".to_string(),
          description: "Tool to generate changelog (git-cliff, conventional-changelog)".to_string(),
          required: false,
          default: Some("git-cliff".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "publish".to_string(),
          description: "Whether to publish to package registry".to_string(),
          required: false,
          default: Some("true".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "version".to_string(),
          description: "Version after bump (e.g., 1.2.3)".to_string(),
          required: false,
          default: Some("1.0.0".to_string()),
        },
      ],
      body: vec![
        "# Version bump".to_string(),
        "version-bump:".to_string(),
        "cargo set-version --bump {{version_type}}".to_string(),
        "".to_string(),
        "# Generate changelog".to_string(),
        "changelog:".to_string(),
        "{{changelog_tool}} --output CHANGELOG.md".to_string(),
        "".to_string(),
        "# Create git tag".to_string(),
        "tag: version-bump changelog".to_string(),
        "git add Cargo.toml CHANGELOG.md".to_string(),
        "git commit -m \"chore: release v{{version}}\"".to_string(),
        "git tag -a v{{version}} -m \"Release v{{version}}\"".to_string(),
        "git push origin main --tags".to_string(),
        "".to_string(),
        "# Publish to registry".to_string(),
        "publish: tag".to_string(),
        "@if {{publish}} == \"true\"".to_string(),
        "cargo publish".to_string(),
        "".to_string(),
        "# Full release".to_string(),
        "release: publish".to_string(),
        "echo \"Release v{{version}} complete!\"".to_string(),
      ],
    },
    // Docker Template
    TemplateProposal {
      name: "docker".to_string(),
      description: "Multi-stage Dockerfile with build, test, and production stages".to_string(),
      category: "docker".to_string(),
      parameters: vec![
        crate::ai_responses::TemplateParameter {
          name: "base_image".to_string(),
          description: "Base Docker image".to_string(),
          required: true,
          default: Some("rust:1.75".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "app_name".to_string(),
          description: "Application name".to_string(),
          required: true,
          default: Some("app".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "port".to_string(),
          description: "Exposed port".to_string(),
          required: false,
          default: Some("8080".to_string()),
        },
      ],
      body: vec![
        "# Build stage".to_string(),
        "docker-build:".to_string(),
        "docker build --target build -t {{app_name}}:build .".to_string(),
        "".to_string(),
        "# Test stage".to_string(),
        "docker-test: docker-build".to_string(),
        "docker run --rm {{app_name}}:build cargo test".to_string(),
        "".to_string(),
        "# Production stage".to_string(),
        "docker-prod: docker-build".to_string(),
        "docker build --target prod -t {{app_name}}:latest .".to_string(),
        "".to_string(),
        "# Run container".to_string(),
        "docker-run: docker-prod".to_string(),
        "docker run -d -p {{port}}:{{port}} --name {{app_name}} {{app_name}}:latest".to_string(),
        "".to_string(),
        "# Clean up".to_string(),
        "docker-clean:".to_string(),
        "docker rmi {{app_name}}:build {{app_name}}:latest || true".to_string(),
      ],
    },
    // Code Quality Template
    TemplateProposal {
      name: "code-quality".to_string(),
      description: "Comprehensive code quality checks: format, lint, audit, security".to_string(),
      category: "quality".to_string(),
      parameters: vec![
        crate::ai_responses::TemplateParameter {
          name: "language".to_string(),
          description: "Programming language".to_string(),
          required: true,
          default: Some("rust".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "deny_warnings".to_string(),
          description: "Deny warnings in clippy".to_string(),
          required: false,
          default: Some("true".to_string()),
        },
      ],
      body: vec![
        "# Format check".to_string(),
        "fmt-check:".to_string(),
        "cargo fmt --all -- --check".to_string(),
        "".to_string(),
        "# Clippy linting".to_string(),
        "clippy:".to_string(),
        "@if {{deny_warnings}} == \"true\"".to_string(),
        "cargo clippy -- -D warnings".to_string(),
        "@else".to_string(),
        "cargo clippy".to_string(),
        "".to_string(),
        "# Security audit".to_string(),
        "audit:".to_string(),
        "cargo audit".to_string(),
        "".to_string(),
        "# Dependency check".to_string(),
        "deps-check:".to_string(),
        "cargo outdated".to_string(),
        "cargo deny check".to_string(),
        "".to_string(),
        "# All quality checks".to_string(),
        "quality: fmt-check clippy audit deps-check".to_string(),
        "echo \"All quality checks passed!\"".to_string(),
      ],
    },
    // Benchmark Template
    TemplateProposal {
      name: "benchmark".to_string(),
      description: "Run benchmarks and compare with baseline".to_string(),
      category: "bench".to_string(),
      parameters: vec![
        crate::ai_responses::TemplateParameter {
          name: "bench_name".to_string(),
          description: "Benchmark name or pattern".to_string(),
          required: false,
          default: Some("*".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "baseline_branch".to_string(),
          description: "Git branch to compare against".to_string(),
          required: false,
          default: Some("main".to_string()),
        },
      ],
      body: vec![
        "# Run benchmarks".to_string(),
        "bench:".to_string(),
        "cargo bench --bench {{bench_name}}".to_string(),
        "".to_string(),
        "# Save baseline".to_string(),
        "bench-save: bench".to_string(),
        "cargo bench --bench {{bench_name}} -- --save-baseline main".to_string(),
        "".to_string(),
        "# Compare with baseline".to_string(),
        "bench-compare: bench".to_string(),
        "cargo bench --bench {{bench_name}} -- --baseline {{baseline_branch}}".to_string(),
        "".to_string(),
        "# Generate benchmark report".to_string(),
        "bench-report: bench".to_string(),
        "cargo bench --bench {{bench_name}} -- --output-format bencher | tee bench-results.txt"
          .to_string(),
      ],
    },
    // Documentation Template
    TemplateProposal {
      name: "docs".to_string(),
      description: "Generate and deploy documentation".to_string(),
      category: "docs".to_string(),
      parameters: vec![
        crate::ai_responses::TemplateParameter {
          name: "host".to_string(),
          description: "Documentation host (github-pages, gitlab-pages, netlify)".to_string(),
          required: false,
          default: Some("github-pages".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "include_private".to_string(),
          description: "Include private items in docs".to_string(),
          required: false,
          default: Some("false".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "project_name".to_string(),
          description: "Project name (for documentation path)".to_string(),
          required: false,
          default: Some("app".to_string()),
        },
      ],
      body: vec![
        "# Build documentation".to_string(),
        "docs-build:".to_string(),
        "@if {{include_private}} == \"true\"".to_string(),
        "cargo doc --document-private-items --no-deps".to_string(),
        "@else".to_string(),
        "cargo doc --no-deps".to_string(),
        "".to_string(),
        "# Test documentation examples".to_string(),
        "docs-test:".to_string(),
        "cargo test --doc".to_string(),
        "".to_string(),
        "# Deploy to GitHub Pages".to_string(),
        "docs-deploy-gh: docs-build".to_string(),
        "gh-pages -d target/doc".to_string(),
        "".to_string(),
        "# Open docs locally".to_string(),
        "docs-open: docs-build".to_string(),
        "open target/doc/{{project_name}}/index.html".to_string(),
      ],
    },
    // Database Migration Template
    TemplateProposal {
      name: "db-migrate".to_string(),
      description: "Database migration management with up/down scripts".to_string(),
      category: "database".to_string(),
      parameters: vec![
        crate::ai_responses::TemplateParameter {
          name: "migration_tool".to_string(),
          description: "Migration tool (sqlx, diesel, sea-orm)".to_string(),
          required: true,
          default: Some("sqlx".to_string()),
        },
        crate::ai_responses::TemplateParameter {
          name: "db_url_env".to_string(),
          description: "Environment variable for database URL".to_string(),
          required: false,
          default: Some("DATABASE_URL".to_string()),
        },
      ],
      body: vec![
        "# Create new migration".to_string(),
        "db-migrate-create:".to_string(),
        "{{migration_tool}} migrate add {{name}}".to_string(),
        "".to_string(),
        "# Run migrations up".to_string(),
        "db-migrate-up:".to_string(),
        "{{db_url_env}}={{DATABASE_URL}} {{migration_tool}} migrate run".to_string(),
        "".to_string(),
        "# Revert last migration".to_string(),
        "db-migrate-down:".to_string(),
        "{{db_url_env}}={{DATABASE_URL}} {{migration_tool}} migrate revert".to_string(),
        "".to_string(),
        "# Show migration status".to_string(),
        "db-migrate-status:".to_string(),
        "{{db_url_env}}={{DATABASE_URL}} {{migration_tool}} migrate info".to_string(),
        "".to_string(),
        "# Reset and re-run all migrations".to_string(),
        "db-migrate-reset: db-migrate-down".to_string(),
        "{{db_url_env}}={{DATABASE_URL}} {{migration_tool}} migrate run".to_string(),
      ],
    },
    // Security Scan Template
    TemplateProposal {
      name: "security-scan".to_string(),
      description: "Comprehensive security scanning: dependencies, code, secrets, containers"
        .to_string(),
      category: "security".to_string(),
      parameters: vec![crate::ai_responses::TemplateParameter {
        name: "scan_type".to_string(),
        description: "Type of scan (all, deps, code, secrets, container)".to_string(),
        required: false,
        default: Some("all".to_string()),
      }],
      body: vec![
        "# Dependency vulnerability scan".to_string(),
        "security-deps:".to_string(),
        "cargo audit".to_string(),
        "cargo deny check advisories".to_string(),
        "".to_string(),
        "# Static code analysis".to_string(),
        "security-code:".to_string(),
        "cargo geiger".to_string(),
        "cargo rustsec".to_string(),
        "".to_string(),
        "# Secret detection".to_string(),
        "security-secrets:".to_string(),
        "git-secrets --scan".to_string(),
        "truffle3hog filesystem . --no-git".to_string(),
        "".to_string(),
        "# Container scanning".to_string(),
        "security-container:".to_string(),
        "trivy image {{image_name}}".to_string(),
        "docker scout cves {{image_name}}".to_string(),
        "".to_string(),
        "# Full security scan".to_string(),
        "security-all: security-deps security-code security-secrets security-container".to_string(),
        "echo \"Security scan complete!\"".to_string(),
      ],
    },
  ]
}

/// Install built-in templates to project
pub fn install_builtin_templates(
  project_root: &Path,
  template_names: Option<Vec<String>>,
) -> Result<Vec<String>, Box<dyn Error>> {
  let builtin = builtin_templates();
  let mut installed = Vec::new();

  for template in builtin {
    if let Some(ref names) = template_names
      && !names.contains(&template.name)
    {
      continue;
    }
    save_template(project_root, &template)?;
    installed.push(template.name);
  }

  Ok(installed)
}

/// List available built-in template names
pub fn list_builtin_template_names() -> Vec<String> {
  builtin_templates().into_iter().map(|t| t.name).collect()
}

pub fn handle_add(
  just_binary: &Path,
  context: &ProjectContext,
  request: &str,
  response: AddRecipeResponse,
  write: bool,
) -> Result<(), Box<dyn Error>> {
  validate_proposal(context, &response.recipe, None)?;

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = bounded_file::read_utf8(source, max_editable_file_bytes())?;
  let recipe = render_recipe(&response.recipe);
  let proposed = insert_recipe_grouped(
    &original,
    &recipe,
    context,
    &response.recipe.dependencies,
    &response.recipe.name,
  );
  bounded_file::ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
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
    application::patches::apply_reviewed_change(source, &original, &proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this recipe.");
  }

  Ok(())
}

pub fn handle_fix(
  just_binary: &Path,
  context: &ProjectContext,
  recipe_name: &str,
  response: FixResponse,
  write: bool,
) -> Result<(), Box<dyn Error>> {
  validate_fix_proposal(context, &response.recipe, recipe_name)?;

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = bounded_file::read_utf8(source, max_editable_file_bytes())?;
  let recipe = render_fix_recipe(&response.recipe);
  let proposed = replace_recipe(&original, recipe_name, &recipe);
  bounded_file::ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
  validate_justfile(just_binary, source, &proposed)?;

  let risks = RiskFinding::scan_lines(&response.recipe.body);
  let risk = RiskLevel::highest(&risks);
  if risk == RiskLevel::Blocked {
    return Err("generated fix has blocked risk and will not be written".into());
  }

  println!("{}", response.summary);
  println!();
  println!("Recipe to fix: {recipe_name}");
  println!("Fixed recipe: {} [{}]", response.recipe.name, risk);

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
    application::patches::apply_reviewed_change(source, &original, &proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this fix.");
  }

  Ok(())
}

/// Handle workflow proposal - validates and inserts multiple recipes
pub fn handle_workflow(
  just_binary: &Path,
  context: &ProjectContext,
  request: &str,
  response: WorkflowResponse,
  write: bool,
) -> Result<(), Box<dyn Error>> {
  // Validate all recipes in the workflow
  for recipe in &response.recipes {
    validate_workflow_recipe(context, recipe, &response.recipes)?;
  }

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = bounded_file::read_utf8(source, max_editable_file_bytes())?;

  // Build a map of recipe name to rendered recipe
  let mut rendered_recipes: HashMap<String, String> = HashMap::new();
  for recipe in &response.recipes {
    rendered_recipes.insert(recipe.name.clone(), render_recipe(recipe));
  }

  // Insert recipes in execution order
  let mut proposed = original.clone();
  for recipe_name in &response.execution_order {
    let recipe = rendered_recipes
      .get(recipe_name)
      .ok_or_else(|| format!("recipe `{recipe_name}` not found in workflow"))?;

    // Find the recipe proposal to get dependencies
    let recipe_proposal = response
      .recipes
      .iter()
      .find(|r| r.name == *recipe_name)
      .ok_or_else(|| format!("recipe proposal for `{recipe_name}` not found"))?;

    proposed = insert_recipe_grouped(
      &proposed,
      recipe,
      context,
      &recipe_proposal.dependencies,
      recipe_name,
    );
  }

  bounded_file::ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
  validate_justfile(just_binary, source, &proposed)?;

  // Check risk for all recipes
  let mut all_risks = Vec::new();
  for recipe in &response.recipes {
    let risks = RiskFinding::scan_lines(&recipe.body);
    all_risks.extend(risks);
  }
  let risk = RiskLevel::highest(&all_risks);
  if risk == RiskLevel::Blocked {
    return Err("generated workflow has blocked risk and will not be written".into());
  }

  println!("{}", response.summary);
  println!();
  println!("Workflow request: {request}");
  println!(
    "Recipes: {}",
    response
      .recipes
      .iter()
      .map(|r| r.name.as_str())
      .collect::<Vec<_>>()
      .join(", ")
  );
  println!("Execution order: {}", response.execution_order.join(" -> "));

  print_section("Rationale", &response.rationale);

  if !all_risks.is_empty() {
    println!();
    println!("Risk findings:");
    for finding in &all_risks {
      println!("  - {}: `{}`", finding.reason, finding.line);
    }
  }

  println!();
  println!("{}", unified_diff(source, &original, &proposed));

  if write {
    application::patches::apply_reviewed_change(source, &original, &proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this workflow.");
  }

  Ok(())
}

fn validate_workflow_recipe(
  context: &ProjectContext,
  proposal: &RecipeProposal,
  all_recipes: &[RecipeProposal],
) -> Result<(), Box<dyn Error>> {
  if proposal.name.is_empty() {
    return Err("generated recipe name is empty".into());
  }

  if proposal.body.is_empty() {
    return Err("generated recipe body is empty".into());
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

  // Check for duplicate names within the workflow
  let name_count = all_recipes
    .iter()
    .filter(|r| r.name == proposal.name)
    .count();
  if name_count > 1 {
    return Err(format!("duplicate recipe name `{}` in workflow", proposal.name).into());
  }

  // Check if recipe already exists in project (but allow if it's being replaced by this workflow)
  if context.has_recipe(&proposal.name) {
    // Check if this recipe is being replaced by another recipe in the workflow
    let is_replaced = all_recipes
      .iter()
      .any(|r| r.name == proposal.name && r.body != proposal.body);

    // Also check if the existing recipe in the project is functionally equivalent
    let existing_equivalent = if let Some(existing) = context.find_recipe(&proposal.name) {
      // Compare normalized bodies (ignoring whitespace differences)
      let existing_body: String = existing
        .body
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ");
      let proposed_body: String = proposal
        .body
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ");
      existing_body == proposed_body
    } else {
      false
    };

    if !is_replaced && !existing_equivalent {
      return Err(format!("recipe `{}` already exists", proposal.name).into());
    }
  }

  for dependency in &proposal.dependencies {
    // Check if dependency exists in project or in this workflow
    let in_workflow = all_recipes.iter().any(|r| r.name == *dependency);
    if !context.has_recipe(dependency) && !in_workflow {
      return Err(format!("dependency recipe `{dependency}` does not exist").into());
    }
  }

  Ok(())
}

pub fn validate_fix_proposal(
  context: &ProjectContext,
  proposal: &FixProposal,
  original_recipe_name: &str,
) -> Result<(), Box<dyn Error>> {
  if proposal.name.is_empty() {
    return Err("generated recipe name is empty".into());
  }

  if proposal.body.is_empty() {
    return Err("generated recipe body is empty".into());
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

  // The fix can replace the original recipe, so allow same name
  if proposal.name != original_recipe_name && context.has_recipe(&proposal.name) {
    return Err(format!("recipe `{}` already exists", proposal.name).into());
  }

  Ok(())
}

pub fn render_fix_recipe(proposal: &FixProposal) -> String {
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

pub fn replace_recipe(original: &str, recipe_name: &str, new_recipe: &str) -> String {
  let lines: Vec<&str> = original.lines().collect();
  let mut result = Vec::new();
  let mut i = 0;
  let mut found = false;

  while i < lines.len() {
    let line = lines[i];
    // Check if this line starts a recipe definition matching recipe_name
    // Recipe definition: name [params] [: deps] :
    let trimmed = line.trim_start();
    let is_recipe_def = trimmed.starts_with(&format!("{recipe_name} "))
      || trimmed == recipe_name
      || trimmed.starts_with(&format!("{recipe_name}:"));
    if !found && is_recipe_def {
      // Found the recipe - skip until we hit a non-indented line (next recipe or EOF)
      found = true;
      result.push(new_recipe);
      // Skip the original recipe body (indented lines)
      i += 1;
      while i < lines.len()
        && (lines[i].starts_with(' ') || lines[i].starts_with('\t') || lines[i].trim().is_empty())
      {
        i += 1;
      }
      continue;
    }
    result.push(line);
    i += 1;
  }

  // If recipe wasn't found, append at end (fallback to append behavior)
  if !found {
    result.push("");
    result.push(new_recipe.trim_end());
  }

  result.join("\n")
}

fn validate_proposal(
  context: &ProjectContext,
  proposal: &RecipeProposal,
  batch_recipes: Option<&[RecipeProposal]>,
) -> Result<(), Box<dyn Error>> {
  if proposal.name.is_empty() {
    return Err("generated recipe name is empty".into());
  }

  if proposal.body.is_empty() {
    return Err("generated recipe body is empty".into());
  }

  if context.has_recipe(&proposal.name) {
    // Check if existing recipe is functionally equivalent (ignoring whitespace)
    // Compare both body AND dependencies
    let existing_equivalent = if let Some(existing) = context.find_recipe(&proposal.name) {
      let existing_body: String = existing
        .body
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ");
      let proposed_body: String = proposal
        .body
        .iter()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join(" ");
      // Also compare dependencies (sorted for consistent comparison)
      let existing_deps = {
        let mut deps = existing.dependencies.clone();
        deps.sort();
        deps
      };
      let proposed_deps = {
        let mut deps = proposal.dependencies.clone();
        deps.sort();
        deps
      };
      existing_body == proposed_body && existing_deps == proposed_deps
    } else {
      false
    };
    if !existing_equivalent {
      return Err(format!("recipe `{}` already exists", proposal.name).into());
    }
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
    let in_context = context.has_recipe(dependency);
    let in_batch = batch_recipes
      .map(|recipes| recipes.iter().any(|r| r.name == *dependency))
      .unwrap_or(false);
    if !in_context && !in_batch {
      return Err(format!("dependency recipe `{dependency}` does not exist").into());
    }
  }

  Ok(())
}

pub fn render_recipe(proposal: &RecipeProposal) -> String {
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

pub fn append_recipe(original: &str, recipe: &str) -> String {
  let mut proposed = original.to_owned();

  if !proposed.ends_with('\n') {
    proposed.push('\n');
  }

  proposed.push('\n');
  proposed.push_str(recipe);
  proposed
}

/// Insert a recipe at a specific line number in the original content.
pub fn insert_recipe_at(original: &str, recipe: &str, insert_line: usize) -> String {
  let lines: Vec<&str> = original.lines().collect();
  let mut result: Vec<&str> = Vec::new();

  for (i, line) in lines.iter().enumerate() {
    if i == insert_line {
      // Ensure proper spacing before the inserted recipe
      if !result.is_empty() && !result.last().unwrap().trim().is_empty() {
        result.push("");
      }
      result.push(recipe.trim_end());
      if i < lines.len() && !lines[i].trim().is_empty() {
        result.push("");
      }
    }
    result.push(line);
  }

  // If insert_line is at the end (past all lines), append
  if insert_line >= lines.len() {
    if !result.is_empty() && !result.last().unwrap().trim().is_empty() {
      result.push("");
    }
    result.push(recipe.trim_end());
  }

  result.join("\n")
}

/// Insert a recipe grouped with related recipes based on dependencies and naming patterns.
pub fn insert_recipe_grouped(
  original: &str,
  recipe: &str,
  context: &ProjectContext,
  dependencies: &[String],
  recipe_name: &str,
) -> String {
  // Try to find the best insertion point using the context
  if let Some(insert_line) =
    inspection::ProjectContext::find_insertion_point(context, recipe_name, dependencies)
  {
    insert_recipe_at(original, recipe, insert_line)
  } else {
    // Fallback to append behavior
    append_recipe(original, recipe)
  }
}

pub fn validate_justfile(
  just_binary: &Path,
  source: &Path,
  proposed: &str,
) -> Result<(), Box<dyn Error>> {
  let temp_path = temporary_justfile_path(source)?;
  fs::write(&temp_path, proposed)?;

  let mut command = Command::new(just_binary);
  command
    .arg("--justfile")
    .arg(&temp_path)
    .args(["--dump", "--dump-format", "json"]);
  let output = bounded_output::capture(&mut command);

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

pub fn unified_diff(path: &Path, original: &str, proposed: &str) -> String {
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

/// Handle template proposal - stores the template for later instantiation
pub fn handle_template(
  context: &ProjectContext,
  request: &str,
  response: TemplateResponse,
) -> Result<(), Box<dyn Error>> {
  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let _original = bounded_file::read_utf8(source, max_editable_file_bytes())?;

  // Save template to disk for persistence
  let project_root = source.parent().unwrap_or_else(|| Path::new("."));
  save_template(project_root, &response.template)?;

  println!("{}", response.summary);
  println!();
  println!("Template request: {request}");
  println!("Template name: {}", response.template.name);
  println!("Category: {}", response.template.category);
  println!("Parameters:");
  for param in &response.template.parameters {
    let req = if param.required { " (required)" } else { "" };
    let def = param.default.as_deref().unwrap_or("none");
    println!(
      "  - {}: {}{} [default: {def}]",
      param.name, param.description, req
    );
  }

  print_section("Rationale", &[format!("Template created for: {request}")]);

  println!();
  println!("Template body:");
  for line in &response.template.body {
    println!("  {line}");
  }

  println!();
  println!(
    "Template saved to .just-ai/templates/{}.json",
    response.template.name
  );
  println!(
    "To instantiate this template, run: just-ai instantiate-template {} <param=value>...",
    response.template.name
  );

  Ok(())
}

/// Handle template instantiation - creates a recipe from a template with provided values
pub fn handle_instantiate_template(
  just_binary: &Path,
  context: &ProjectContext,
  template: &TemplateProposal,
  values: &std::collections::HashMap<String, String>,
  write: bool,
  force: bool,
) -> Result<(), Box<dyn Error>> {
  // Substitute template parameters in the body
  let mut substituted_body = Vec::new();
  for line in &template.body {
    let mut substituted = line.clone();
    for (key, value) in values {
      substituted = substituted.replace(&format!("{{{{{key}}}}}"), value);
    }
    substituted_body.push(substituted);
  }

  // Parse the template body into multiple recipes
  let recipes = parse_template_body(
    &substituted_body,
    &template.name,
    &template.description,
    &template.parameters,
  )?;

  // Validate all recipes, skipping existing non-equivalent ones if force is true
  // Also skip recipes that are functionally equivalent to existing ones
  let mut recipes_to_instantiate = Vec::new();
  let mut skipped_recipes = Vec::new();

  for recipe in &recipes {
    // First check if recipe is functionally equivalent to existing
    let is_equivalent = if context.has_recipe(&recipe.name) {
      if let Some(existing) = context.find_recipe(&recipe.name) {
        let existing_body: String = existing
          .body
          .iter()
          .map(|l| l.trim())
          .collect::<Vec<_>>()
          .join(" ");
        let proposed_body: String = recipe
          .body
          .iter()
          .map(|l| l.trim())
          .collect::<Vec<_>>()
          .join(" ");
        let existing_deps = {
          let mut deps = existing.dependencies.clone();
          deps.sort();
          deps
        };
        let proposed_deps = {
          let mut deps = recipe.dependencies.clone();
          deps.sort();
          deps
        };
        existing_body == proposed_body && existing_deps == proposed_deps
      } else {
        false
      }
    } else {
      false
    };

    if is_equivalent {
      // Skip equivalent recipes automatically
      skipped_recipes.push(recipe.name.clone());
      println!(
        "Skipping equivalent recipe: {} (already exists with same implementation)",
        recipe.name
      );
      continue;
    }

    let validation_result = validate_proposal(context, recipe, Some(&recipes));
    match validation_result {
      Ok(()) => {
        recipes_to_instantiate.push(recipe.clone());
      }
      Err(e) => {
        let err_msg = e.to_string();
        if force && err_msg.contains("already exists") {
          // Skip this recipe - it already exists with a different implementation
          skipped_recipes.push(recipe.name.clone());
          println!(
            "Skipping existing recipe: {} (use --force to override behavior)",
            recipe.name
          );
        } else {
          return Err(e);
        }
      }
    }
  }

  if recipes_to_instantiate.is_empty() {
    println!(
      "All recipes from template '{}' already exist. Nothing to instantiate.",
      template.name
    );
    if !skipped_recipes.is_empty() {
      println!("Skipped recipes: {}", skipped_recipes.join(", "));
    }
    return Ok(());
  }

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = bounded_file::read_utf8(source, max_editable_file_bytes())?;

  // Insert all recipes one by one
  let mut proposed = original.clone();
  let mut all_risks = Vec::new();

  for recipe in &recipes_to_instantiate {
    let rendered = render_recipe(recipe);
    proposed = insert_recipe_grouped(
      &proposed,
      &rendered,
      context,
      &recipe.dependencies,
      &recipe.name,
    );

    let risks = RiskFinding::scan_lines(&recipe.body);
    let max_risk = RiskLevel::highest(&risks);
    if max_risk == RiskLevel::Blocked {
      return Err(
        format!(
          "recipe `{}` has blocked risk and will not be written",
          recipe.name
        )
        .into(),
      );
    }
    all_risks.extend(risks);
  }

  bounded_file::ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
  validate_justfile(just_binary, source, &proposed)?;

  let highest_risk = RiskLevel::highest(&all_risks);

  println!("Template instantiated: {}", template.name);
  println!();
  println!(
    "Recipes: {}",
    recipes_to_instantiate
      .iter()
      .map(|r| r.name.as_str())
      .collect::<Vec<_>>()
      .join(", ")
  );
  if !skipped_recipes.is_empty() {
    println!("Skipped (already exist): {}", skipped_recipes.join(", "));
  }
  println!("Highest risk: {}", highest_risk);

  println!();
  println!("{}", unified_diff(source, &original, &proposed));

  if write {
    application::patches::apply_reviewed_change(source, &original, &proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this template.");
  }

  Ok(())
}

/// Parse template body (which contains multiple just recipes) into individual RecipeProposals
fn parse_template_body(
  body: &[String],
  template_name: &str,
  template_description: &str,
  template_parameters: &[crate::ai_responses::TemplateParameter],
) -> Result<Vec<RecipeProposal>, Box<dyn Error>> {
  let mut recipes = Vec::new();
  let mut current_recipe: Option<RecipeProposal> = None;
  let mut current_body: Vec<String> = Vec::new();
  let mut pending_doc: Option<String> = None; // Section header comment before a recipe

  for line in body {
    let trimmed = line.trim_start();
    let leading_ws_len = line.len() - trimmed.len();
    let starts_with_ws = leading_ws_len > 0;
    let is_comment = trimmed.starts_with('#');
    let is_empty = trimmed.is_empty();

    // Check if this line starts a new recipe definition (name: or name params:)
    // Recipe definition must be at column 0 (no leading whitespace)
    let is_new_recipe = if starts_with_ws {
      false
    } else if let Some(colon_pos) = trimmed.find(':') {
      let name_part = &trimmed[..colon_pos];
      // Recipe name has no leading whitespace and doesn't start with special chars
      !name_part.is_empty()
        && name_part.chars().all(|c| {
          c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ' || c == '=' || c == '\''
        })
    } else {
      false
    };

    if is_new_recipe {
      // Finish previous recipe if any
      if let Some(mut recipe) = current_recipe.take() {
        // Use pending doc if available, otherwise use template description
        if let Some(doc) = pending_doc.take() {
          recipe.doc = Some(doc);
        }
        recipe.body = current_body.clone();
        recipes.push(recipe);
      }

      // Parse recipe definition
      let def = trimmed;
      let (name_part, deps_part) = def.split_once(':').unwrap_or((def, ""));
      let name_parts: Vec<&str> = name_part.split_whitespace().collect();
      let recipe_name = name_parts.first().map_or(template_name, |v| v).to_string();

      // Parse dependencies - support both parenthesized (dep1) (dep2) and space-separated dep1 dep2 formats
      let deps = if !deps_part.trim().is_empty() {
        deps_part
          .split_whitespace()
          .filter_map(|d| {
            let d = d.trim();
            if d.starts_with('(') && d.ends_with(')') {
              Some(d[1..d.len() - 1].to_string())
            } else if !d.is_empty() && !d.starts_with('#') {
              // Space-separated dependency format (justfile standard)
              Some(d.to_string())
            } else {
              None
            }
          })
          .collect()
      } else {
        Vec::new()
      };

      // Parse parameters (name=default form)
      let params = name_parts[1..]
        .iter()
        .map(|p| {
          if let Some(eq_pos) = p.find('=') {
            let name = p[..eq_pos].to_string();
            let default = p[eq_pos + 1..].trim_matches('\'').to_string();
            RecipeParameterProposal {
              name,
              default: Some(default),
            }
          } else {
            RecipeParameterProposal {
              name: p.to_string(),
              default: None,
            }
          }
        })
        .collect();

      current_recipe = Some(RecipeProposal {
        name: recipe_name,
        doc: None, // Will be set from pending_doc when recipe is finalized
        parameters: params,
        dependencies: deps,
        body: Vec::new(),
      });

      // Reset body for new recipe
      current_body.clear();
    } else if starts_with_ws || is_empty {
      // Indented line or empty line - part of current recipe body
      if current_recipe.is_some() {
        current_body.push(line.clone());
      }
    } else if is_comment && !starts_with_ws {
      // Top-level comment (section header) - save as pending doc for next recipe
      pending_doc = Some(line.trim().to_string());
    } else {
      // Other lines at column 0 (like @for, @if directives) - part of body if we have a recipe
      if current_recipe.is_some() {
        current_body.push(line.clone());
      }
    }
  }

  // Don't forget the last recipe
  if let Some(mut recipe) = current_recipe {
    if let Some(doc) = pending_doc.take() {
      recipe.doc = Some(doc);
    }
    recipe.body = current_body;
    recipes.push(recipe);
  }

  // If no recipes were parsed, fall back to treating entire body as single recipe
  if recipes.is_empty() {
    recipes.push(RecipeProposal {
      name: template_name.to_string(),
      doc: Some(template_description.to_string()),
      parameters: template_parameters
        .iter()
        .map(|p| RecipeParameterProposal {
          name: p.name.clone(),
          default: p.default.clone(),
        })
        .collect(),
      dependencies: vec![],
      body: body.to_vec(),
    });
  }

  Ok(recipes)
}

/// Handle compose workflow proposal - validates and inserts/modifies recipes based on composition
pub fn handle_compose_workflow(
  just_binary: &Path,
  context: &ProjectContext,
  request: &str,
  response: ComposeWorkflowResponse,
  write: bool,
) -> Result<(), Box<dyn Error>> {
  // Validate all recipes in the workflow
  let existing_recipe_names: std::collections::HashSet<_> =
    context.recipes.iter().map(|r| r.namepath.clone()).collect();

  for recipe in &response.recipes {
    if recipe.name.is_empty() {
      return Err("generated recipe name is empty".into());
    }

    if recipe.body.is_empty() {
      return Err("generated recipe body is empty".into());
    }

    if !recipe
      .name
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
      return Err(
        format!(
          "recipe name `{}` contains unsupported characters",
          recipe.name
        )
        .into(),
      );
    }

    // Check source type matches reality
    let exists = existing_recipe_names.contains(&recipe.name);
    match recipe.source.as_str() {
      "existing" => {
        if !exists {
          return Err(
            format!(
              "recipe `{}` marked as existing but not found in project",
              recipe.name
            )
            .into(),
          );
        }
      }
      "modified" => {
        if !exists {
          return Err(
            format!(
              "recipe `{}` marked as modified but not found in project",
              recipe.name
            )
            .into(),
          );
        }
      }
      "new" => {
        if exists {
          return Err(
            format!(
              "recipe `{}` marked as new but already exists in project",
              recipe.name
            )
            .into(),
          );
        }
      }
      _ => {
        return Err(
          format!(
            "invalid source type `{}`, must be existing|new|modified",
            recipe.source
          )
          .into(),
        );
      }
    }

    for dependency in &recipe.dependencies {
      let in_workflow = response.recipes.iter().any(|r| r.name == *dependency);
      if !context.has_recipe(dependency) && !in_workflow {
        return Err(format!("dependency recipe `{dependency}` does not exist").into());
      }
    }
  }

  let source = context
    .root_source()
    .ok_or("project context does not contain a root justfile source")?;
  let original = bounded_file::read_utf8(source, max_editable_file_bytes())?;

  // Build a map of recipe name to rendered recipe
  let mut rendered_recipes: HashMap<String, String> = HashMap::new();
  for recipe in &response.recipes {
    let proposal = RecipeProposal {
      name: recipe.name.clone(),
      doc: recipe.doc.clone(),
      parameters: recipe.parameters.clone(),
      dependencies: recipe.dependencies.clone(),
      body: recipe.body.clone(),
    };
    rendered_recipes.insert(recipe.name.clone(), render_recipe(&proposal));
  }

  // Apply recipes in execution order
  let mut proposed = original.clone();
  for recipe_name in &response.execution_order {
    let recipe = rendered_recipes
      .get(recipe_name)
      .ok_or_else(|| format!("recipe `{recipe_name}` not found in workflow"))?;

    let recipe_proposal = response
      .recipes
      .iter()
      .find(|r| r.name == *recipe_name)
      .ok_or_else(|| format!("recipe proposal for `{recipe_name}` not found"))?;

    if recipe_proposal.source == "existing" {
      // Skip existing recipes - they're already in the justfile
      continue;
    }

    proposed = insert_recipe_grouped(
      &proposed,
      recipe,
      context,
      &recipe_proposal.dependencies,
      recipe_name,
    );
  }

  bounded_file::ensure_text_limit(&proposed, "proposed justfile", max_editable_file_bytes())?;
  validate_justfile(just_binary, source, &proposed)?;

  // Check risk for all recipes
  let mut all_risks = Vec::new();
  for recipe in &response.recipes {
    let risks = RiskFinding::scan_lines(&recipe.body);
    all_risks.extend(risks);
  }
  let risk = RiskLevel::highest(&all_risks);
  if risk == RiskLevel::Blocked {
    return Err("composed workflow has blocked risk and will not be written".into());
  }

  println!("{}", response.summary);
  println!();
  println!("Workflow request: {request}");
  println!(
    "Recipes: {}",
    response
      .recipes
      .iter()
      .map(|r| format!("{} ({})", r.name, r.source))
      .collect::<Vec<_>>()
      .join(", ")
  );
  println!("Execution order: {}", response.execution_order.join(" -> "));

  print_section("Rationale", &response.rationale);

  if !all_risks.is_empty() {
    println!();
    println!("Risk findings:");
    for finding in &all_risks {
      println!("  - {}: `{}`", finding.reason, finding.line);
    }
  }

  println!();
  println!("{}", unified_diff(source, &original, &proposed));

  if write {
    application::patches::apply_reviewed_change(source, &original, &proposed)?;
    println!("Wrote {}", source.display());
  } else {
    println!("Dry run only. Re-run with --write to apply this workflow.");
  }

  Ok(())
}
