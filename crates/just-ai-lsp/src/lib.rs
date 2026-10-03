//! Language Server Protocol implementation for justfiles
//!
//! Provides:
//! - Diagnostics: syntax errors, undefined recipes, circular dependencies, unused variables
//! - Completion: recipe names, parameters, variables, built-in functions, settings
//! - Hover: recipe documentation, parameter types, variable values
//! - Go to Definition: recipe references, dependencies, variables, imports
//! - Document Symbols: recipes, variables, imports, modules
//! - Code Actions: add missing recipe, fix undefined recipe, add import

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use lsp_types::*;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::{Client, LanguageServer};
use tracing::{debug, info, warn};
use url::Url;

mod analysis;
mod completion;
mod diagnostics;
mod hover;
mod symbols;

use analysis::{analyze_project, ProjectAnalysis};
use completion::provide_completions;
use diagnostics::compute_diagnostics;
use hover::provide_hover;
use symbols::provide_document_symbols;

/// LSP Server state
#[derive(Clone)]
pub struct JustLspServer {
  client: Client,
  /// Open documents: uri -> (version, text)
  documents: Arc<Mutex<HashMap<Url, (i32, String)>>>,
  /// Workspace folders
  workspace_folders: Arc<Mutex<Vec<WorkspaceFolder>>>,
}

impl JustLspServer {
  pub fn new(client: Client) -> Self {
    Self {
      client,
      documents: Arc::new(Mutex::new(HashMap::new())),
      workspace_folders: Arc::new(Mutex::new(Vec::new())),
    }
  }

  /// Get or compute project analysis for a document
  async fn get_analysis(&self, uri: &Url) -> Result<ProjectAnalysis> {
    let project_root = self.find_project_root(uri).await?;

    // Refresh from disk on every request. An indefinitely cached dump silently
    // loses saved edits and changes to imported files.
    analyze_project(&project_root).await
  }

  /// Find the project root (directory containing justfile) for a URI
  async fn find_project_root(&self, uri: &Url) -> Result<PathBuf> {
    let path = uri
      .to_file_path()
      .map_err(|_| anyhow::anyhow!("Invalid file URI"))?;
    let mut current = path.parent().unwrap_or(&path);

    loop {
      let justfile = current.join("justfile");
      let justfile_cap = current.join("Justfile");
      let dot_just = current.join(".just");

      if justfile.exists() || justfile_cap.exists() || dot_just.exists() {
        return Ok(current.to_path_buf());
      }

      let parent = current.parent();
      if parent.is_none() || parent == Some(current) {
        break;
      }
      current = parent.unwrap();
    }

    // Fall back to workspace folder or file's parent
    let folders = self.workspace_folders.lock().unwrap();
    if let Some(folder) = folders.first() {
      return folder
        .uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("Invalid workspace URI"));
    }

    Ok(path.parent().unwrap_or(&path).to_path_buf())
  }

  /// Update document content
  fn update_document(&self, uri: Url, version: i32, text: String) {
    self.documents.lock().unwrap().insert(uri, (version, text));
  }

  /// Remove document
  fn remove_document(&self, uri: &Url) {
    self.documents.lock().unwrap().remove(uri);
  }

  /// Get document text
  fn get_document(&self, uri: &Url) -> Option<String> {
    self
      .documents
      .lock()
      .unwrap()
      .get(uri)
      .map(|(_, text)| text.clone())
  }

  /// Publish diagnostics for a document
  async fn publish_diagnostics(&self, uri: Url) {
    let text = match self.get_document(&uri) {
      Some(t) => t,
      None => return,
    };

    let analysis = match self.get_analysis(&uri).await {
      Ok(a) => a,
      Err(e) => {
        warn!("Failed to get analysis for diagnostics: {}", e);
        return;
      }
    };

    let diagnostics = compute_diagnostics(&uri, &text, &analysis);
    self
      .client
      .publish_diagnostics(uri, diagnostics, None)
      .await;
  }
}

#[tower_lsp::async_trait]
impl LanguageServer for JustLspServer {
  async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
    info!("Initializing just-ai LSP server");

    if let Some(folders) = params.workspace_folders {
      *self.workspace_folders.lock().unwrap() = folders;
    }

    Ok(InitializeResult {
      capabilities: ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
          TextDocumentSyncKind::INCREMENTAL,
        )),
        completion_provider: Some(CompletionOptions {
          resolve_provider: Some(false),
          trigger_characters: Some(vec![
            ":".into(),
            "=".into(),
            " ".into(),
            "\t".into(),
            "$".into(),
            "(".into(),
            "[".into(),
            "{".into(),
            ".".into(),
            "/".into(),
          ]),
          all_commit_characters: None,
          work_done_progress_options: Default::default(),
          completion_item: None,
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        workspace: Some(WorkspaceServerCapabilities {
          workspace_folders: Some(WorkspaceFoldersServerCapabilities {
            supported: Some(true),
            change_notifications: Some(OneOf::Left(true)),
          }),
          file_operations: None,
        }),
        ..Default::default()
      },
      server_info: Some(ServerInfo {
        name: "just-ai-lsp".to_string(),
        version: Some("0.1.0".to_string()),
      }),
    })
  }

  async fn initialized(&self, _: InitializedParams) {
    info!("LSP server initialized");
  }

  async fn shutdown(&self) -> LspResult<()> {
    info!("Shutting down LSP server");
    Ok(())
  }

  async fn did_open(&self, params: DidOpenTextDocumentParams) {
    debug!("Document opened: {}", params.text_document.uri);
    let uri = params.text_document.uri.clone();
    self.update_document(
      uri.clone(),
      params.text_document.version,
      params.text_document.text,
    );
    self.publish_diagnostics(uri).await;
  }

  async fn did_change(&self, params: DidChangeTextDocumentParams) {
    debug!("Document changed: {}", params.text_document.uri);
    let uri = params.text_document.uri.clone();
    for change in params.content_changes {
      // In lsp-types 0.94, TextDocumentContentChangeEvent is a struct with optional range
      if let Some(range) = change.range {
        // Incremental change
        if let Some((version, current_text)) = self.documents.lock().unwrap().get_mut(&uri) {
          *version = params.text_document.version;
          let start = position_to_offset(current_text, range.start);
          let end = position_to_offset(current_text, range.end);
          if start <= end {
            current_text.replace_range(start..end, &change.text);
          }
        }
      } else {
        // Full document change
        self.update_document(uri.clone(), params.text_document.version, change.text);
      }
    }
    self.publish_diagnostics(uri).await;
  }

  async fn did_close(&self, params: DidCloseTextDocumentParams) {
    debug!("Document closed: {}", params.text_document.uri);
    self.remove_document(&params.text_document.uri);
  }

  async fn did_save(&self, params: DidSaveTextDocumentParams) {
    debug!("Document saved: {}", params.text_document.uri);
    self.publish_diagnostics(params.text_document.uri).await;
  }

  async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
    debug!("Completion request at {:?}", params.text_document_position);
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let text = self.get_document(&uri).unwrap_or_default();
    let analysis = self.get_analysis(&uri).await.ok();

    let completions = provide_completions(&text, position, analysis.as_ref());
    Ok(Some(CompletionResponse::Array(completions)))
  }

  async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
    debug!(
      "Hover request at {:?}",
      params.text_document_position_params
    );
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let text = self.get_document(&uri).unwrap_or_default();
    let analysis = self.get_analysis(&uri).await.ok();

    let hover = provide_hover(&text, position, analysis.as_ref());
    Ok(hover)
  }

  async fn goto_definition(
    &self,
    params: GotoDefinitionParams,
  ) -> LspResult<Option<GotoDefinitionResponse>> {
    debug!(
      "Go to definition request at {:?}",
      params.text_document_position_params
    );
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let text = self.get_document(&uri).unwrap_or_default();
    let analysis = self.get_analysis(&uri).await.ok();

    let locations = analysis::find_definition(&text, position, analysis.as_ref(), &uri);
    Ok(locations.map(|locs| {
      if locs.len() == 1 {
        GotoDefinitionResponse::Scalar(locs[0].clone())
      } else {
        GotoDefinitionResponse::Array(locs)
      }
    }))
  }

  async fn document_symbol(
    &self,
    params: DocumentSymbolParams,
  ) -> LspResult<Option<DocumentSymbolResponse>> {
    debug!("Document symbols request for {}", params.text_document.uri);
    let uri = params.text_document.uri;
    let text = self.get_document(&uri).unwrap_or_default();
    let analysis = self.get_analysis(&uri).await.ok();

    let symbols = provide_document_symbols(&text, analysis.as_ref());
    Ok(Some(DocumentSymbolResponse::Nested(symbols)))
  }

  async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
    debug!("Code action request for {}", params.text_document.uri);
    let uri = params.text_document.uri;
    let range = params.range;
    let context = params.context;

    let text = self.get_document(&uri).unwrap_or_default();
    let analysis = self.get_analysis(&uri).await.ok();

    let actions =
      analysis::provide_code_actions(&text, range, &context.diagnostics, analysis.as_ref(), &uri);
    Ok(Some(actions))
  }

  async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
    let mut folders = self.workspace_folders.lock().unwrap();
    for folder in params.event.removed {
      folders.retain(|f| f.uri != folder.uri);
    }
    for folder in params.event.added {
      folders.push(folder);
    }
  }
}

/// Convert UTF-16 LSP positions into valid UTF-8 byte boundaries.
fn position_to_offset(text: &str, position: Position) -> usize {
  let mut offset = 0;
  for (i, line) in text.split_inclusive('\n').enumerate() {
    if i as u32 == position.line {
      let content = line.trim_end_matches('\n').trim_end_matches('\r');
      let characters = utf16_character_index(content, position.character);
      return offset
        + content
          .char_indices()
          .nth(characters)
          .map_or(content.len(), |(index, _)| index);
    }
    offset += line.len();
  }
  text.len()
}

fn utf16_character_index(text: &str, position: u32) -> usize {
  let mut units = 0;
  for (index, character) in text.chars().enumerate() {
    if units + character.len_utf16() > position as usize {
      return index;
    }
    units += character.len_utf16();
  }
  text.chars().count()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn utf16_offsets_handle_multibyte_text_emoji_crlf_and_end_of_document() {
    let text = "é😀x\r\nnext\n";
    assert_eq!(position_to_offset(text, Position::new(0, 1)), 2);
    assert_eq!(position_to_offset(text, Position::new(0, 2)), 2);
    assert_eq!(position_to_offset(text, Position::new(0, 3)), 6);
    assert_eq!(position_to_offset(text, Position::new(0, 99)), 7);
    assert_eq!(position_to_offset(text, Position::new(1, 1)), 10);
    assert_eq!(position_to_offset(text, Position::new(2, 0)), text.len());
    assert_eq!(position_to_offset(text, Position::new(99, 0)), text.len());
    let mut edited = text.to_owned();
    let start = position_to_offset(text, Position::new(0, 1));
    let end = position_to_offset(text, Position::new(0, 3));
    edited.replace_range(start..end, "a");
    assert_eq!(edited, "éax\r\nnext\n");
  }
}
