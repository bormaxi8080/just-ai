use std::{
  path::PathBuf,
  sync::Mutex,
  time::{Instant, SystemTime, UNIX_EPOCH},
};

use just_ai::application::{
  execution::{CancellationToken, PreparedRun, RecipeExecutor, RunConfirmation, RunRequest},
  history::{JsonLineHistory, RunHistory, RunRecord, project_history_path},
};
use tauri::Emitter;

#[tauri::command]
fn inspect_project(project_root: PathBuf) -> Result<just_ai::ProjectContext, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }

  just_ai::inspect_project_at("just", project_root).map_err(|error| error.to_string())
}

#[tauri::command]
async fn prepare_run(request: RunRequest) -> Result<PreparedRun, String> {
  tauri::async_runtime::spawn_blocking(move || RecipeExecutor::new("just").prepare(request))
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn recent_runs(project_root: PathBuf, limit: usize) -> Result<Vec<RunRecord>, String> {
  if !project_root.is_dir() {
    return Err(format!(
      "project root is not a directory: {}",
      project_root.display()
    ));
  }
  JsonLineHistory::new(project_history_path(&project_root), 500)
    .recent(limit.min(100))
    .map_err(|error| error.to_string())
}

#[derive(serde::Serialize)]
struct RunResult {
  success: bool,
  exit_code: Option<i32>,
  stdout: String,
  stderr: String,
}

#[derive(Default)]
struct ActiveRun(Mutex<Option<CancellationToken>>);

#[tauri::command]
async fn execute_run(
  app: tauri::AppHandle,
  active_run: tauri::State<'_, ActiveRun>,
  prepared: PreparedRun,
  confirmation: RunConfirmation,
) -> Result<RunResult, String> {
  let cancellation = CancellationToken::default();
  {
    let mut active = active_run.0.lock().map_err(|error| error.to_string())?;
    if active.is_some() {
      return Err("another recipe is already running".into());
    }
    *active = Some(cancellation.clone());
  }
  let result = tauri::async_runtime::spawn_blocking(move || -> Result<RunResult, String> {
    let started_at_ms = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_err(|error| error.to_string())?
      .as_millis();
    let started = Instant::now();
    let project_root = prepared.request.project_root.clone();
    let recipe = prepared.request.recipe.clone();
    let completed = RecipeExecutor::new("just")
      .execute_streaming(&prepared, &confirmation, &cancellation, |event| {
        let _ = app.emit("run-event", event);
      })
      .map_err(|error| error.to_string())?;
    let record = RunRecord::completed(
      recipe,
      started_at_ms,
      started.elapsed().as_millis(),
      completed.status.code(),
      completed.status.success(),
      &completed.stdout,
      &completed.stderr,
    );
    JsonLineHistory::new(project_history_path(&project_root), 500)
      .append(&record)
      .map_err(|error| error.to_string())?;
    Ok(RunResult {
      success: completed.status.success(),
      exit_code: completed.status.code(),
      stdout: String::from_utf8_lossy(&completed.stdout).into_owned(),
      stderr: String::from_utf8_lossy(&completed.stderr).into_owned(),
    })
  })
  .await;
  *active_run.0.lock().map_err(|error| error.to_string())? = None;
  result.map_err(|error| error.to_string())?
}

#[tauri::command]
fn cancel_run(active_run: tauri::State<'_, ActiveRun>) -> Result<bool, String> {
  let active = active_run.0.lock().map_err(|error| error.to_string())?;
  if let Some(cancellation) = active.as_ref() {
    cancellation.cancel();
    Ok(true)
  } else {
    Ok(false)
  }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(ActiveRun::default())
    .invoke_handler(tauri::generate_handler![
      inspect_project,
      prepare_run,
      recent_runs,
      execute_run,
      cancel_run
    ])
    .run(tauri::generate_context!())
    .expect("failed to run just-ai desktop application");
}
