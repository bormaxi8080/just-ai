pub mod ai_responses;
pub mod application;
pub mod bounded_file;
pub mod cli;
pub mod config;
pub mod domain;
pub mod inspection;
pub mod prompts;
pub mod proposal;
pub mod provider;

mod bounded_output;
mod just_dump;

pub use ai_responses::{AddRecipeResponse, FixResponse};
pub use bounded_file::{ensure_text_limit, max_editable_file_bytes};
pub use cli::AiClient;
pub use inspection::{
  ContextModule, ContextParameter, ContextRecipe, ProjectContext, inspect_project,
  inspect_project_at,
};
pub use proposal::{
  append_recipe, handle_add, handle_fix, insert_recipe_at, insert_recipe_grouped,
  render_fix_recipe, render_recipe, replace_recipe, unified_diff, validate_justfile,
};

/// Run the command-line adapter using process arguments.
pub fn run() -> std::process::ExitCode {
  cli::run()
}
