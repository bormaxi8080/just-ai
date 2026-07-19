pub mod application;
pub mod domain;
pub mod prompts;
pub mod provider;

mod ai_responses;
mod cli;
mod inspection;
mod proposal;

pub use inspection::{
  ContextModule, ContextParameter, ContextRecipe, ProjectContext, inspect_project,
  inspect_project_at,
};

/// Run the command-line adapter using process arguments.
pub fn run() -> std::process::ExitCode {
  cli::run()
}
