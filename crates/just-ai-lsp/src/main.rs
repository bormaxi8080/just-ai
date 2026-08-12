use just_ai_lsp::JustLspServer;
use tokio::io::{stdin, stdout};
use tower_lsp::{LspService, Server};

#[tokio::main]
async fn main() {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .with_writer(std::io::stderr)
    .init();

  let (service, socket) = LspService::new(JustLspServer::new);
  Server::new(stdin(), stdout(), socket).serve(service).await;
}
