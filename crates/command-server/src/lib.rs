//! Loopback command bus.
//!
//! The same `POST /api/invoke { cmd, args }` envelope the cloud server exposes,
//! but generic over the host state so the mobile core and the headless host can
//! each register their own command set against it.
//!
//! Deliberately transport-only: no business logic, no dependency on a concrete
//! host. Command names are the `app__snake_case` identifiers the desktop
//! `bindings.ts` already uses, so one frontend contract serves every transport.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// How a failed command reports itself. Mirrors the cloud server's codes so the
/// frontend can branch on `code` instead of parsing messages.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: CommandErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandErrorCode {
    /// The command name is not registered on this transport.
    Unsupported,
    /// The command exists but rejected its arguments.
    BadRequest,
    /// Anything raised by the command implementation.
    CommandFailed,
}

impl CommandError {
    pub fn unsupported(command: impl Into<String>) -> Self {
        let command = command.into();
        Self {
            code: CommandErrorCode::Unsupported,
            message: format!("`{command}` is not available on this transport"),
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            code: CommandErrorCode::CommandFailed,
            message: message.into(),
        }
    }
}

impl IntoResponse for CommandError {
    fn into_response(self) -> Response {
        (axum::http::StatusCode::OK, Json(json!({ "ok": false, "error": self }))).into_response()
    }
}

pub type CommandFuture = Pin<Box<dyn Future<Output = Result<Value, CommandError>> + Send>>;
pub type CommandHandler<S> = Arc<dyn Fn(Arc<S>, Value) -> CommandFuture + Send + Sync>;

/// Command name → handler, generic over the host state type `S`.
pub struct CommandRegistry<S> {
    handlers: HashMap<&'static str, CommandHandler<S>>,
}

impl<S> Default for CommandRegistry<S> {
    fn default() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }
}

impl<S> CommandRegistry<S>
where
    S: Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F, Fut>(&mut self, name: &'static str, handler: F)
    where
        F: Fn(Arc<S>, Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Value, CommandError>> + Send + 'static,
    {
        self.handlers
            .insert(name, Arc::new(move |state, args| Box::pin(handler(state, args))));
    }

    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    pub fn dispatch(
        &self,
        state: Arc<S>,
        name: &str,
        args: Value,
    ) -> Result<CommandFuture, CommandError> {
        match self.handlers.get(name) {
            Some(handler) => Ok(handler(state, args)),
            None => Err(CommandError::unsupported(name)),
        }
    }
}

#[derive(Deserialize)]
struct InvokeRequest {
    cmd: String,
    #[serde(default)]
    args: Value,
}

/// Shared router state: the registry plus the host it dispatches against.
pub struct CommandBus<S> {
    pub registry: Arc<CommandRegistry<S>>,
    pub state: Arc<S>,
}

async fn invoke_endpoint<S>(
    State(bus): State<Arc<CommandBus<S>>>,
    Json(request): Json<InvokeRequest>,
) -> Response
where
    S: Send + Sync + 'static,
{
    let future = match bus.registry.dispatch(Arc::clone(&bus.state), &request.cmd, request.args) {
        Ok(future) => future,
        Err(error) => {
            tracing::debug!(cmd = %request.cmd, "invoke: unsupported");
            return error.into_response();
        }
    };

    match future.await {
        Ok(value) => Json(json!({ "ok": true, "result": value })).into_response(),
        Err(error) => {
            tracing::warn!(cmd = %request.cmd, message = %error.message, "invoke: failed");
            error.into_response()
        }
    }
}

/// Router serving `POST /api/invoke`.
pub fn router<S>(bus: Arc<CommandBus<S>>) -> Router
where
    S: Send + Sync + 'static,
{
    Router::new()
        .route("/api/invoke", post(invoke_endpoint::<S>))
        .with_state(bus)
}

/// Binds an ephemeral loopback port and serves the bus on it.
///
/// Returns the bound address so the caller can hand `127.0.0.1:<port>` to the
/// UI layer. Ephemeral by design: the port must not be guessable from outside.
pub async fn serve_loopback<S>(bus: Arc<CommandBus<S>>) -> std::io::Result<(std::net::SocketAddr, tokio::task::JoinHandle<()>)>
where
    S: Send + Sync + 'static,
{
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    let app = router(bus);
    let handle = tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, app).await {
            tracing::error!(error = %error, "command bus stopped");
        }
    });
    Ok((address, handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeHost {
        greeting: String,
    }

    async fn echo(state: Arc<FakeHost>, args: Value) -> Result<Value, CommandError> {
        let name = args
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("world");
        Ok(json!({ "message": format!("{}, {name}", state.greeting) }))
    }

    fn bus() -> Arc<CommandBus<FakeHost>> {
        let mut registry = CommandRegistry::new();
        registry.register("app__echo", echo);
        Arc::new(CommandBus {
            registry: Arc::new(registry),
            state: Arc::new(FakeHost {
                greeting: "hello".into(),
            }),
        })
    }

    #[test]
    fn registers_and_dispatches() {
        let registry = {
            let mut registry = CommandRegistry::new();
            registry.register("app__echo", echo);
            registry
        };
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("app__echo"));
    }

    #[test]
    fn unknown_command_is_rejected() {
        let bus = bus();
        let error = bus
            .registry
            .dispatch(Arc::clone(&bus.state), "app__missing", Value::Null)
            .err()
            .expect("unknown command must fail");
        assert!(matches!(error.code, CommandErrorCode::Unsupported));
    }

    #[tokio::test]
    async fn serves_invoke_over_http() {
        let (address, handle) = serve_loopback(bus()).await.expect("bind loopback");

        let client = reqwest_lite_post(address, json!({ "cmd": "app__echo", "args": { "name": "vrcx" } })).await;
        assert!(client.contains("\"ok\":true"), "unexpected body: {client}");
        assert!(client.contains("hello, vrcx"), "unexpected body: {client}");

        handle.abort();
    }

    /// Minimal POST helper so the test does not pull in an HTTP client crate.
    async fn reqwest_lite_post(address: std::net::SocketAddr, payload: Value) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let body = payload.to_string();
        let mut stream = tokio::net::TcpStream::connect(address)
            .await
            .expect("connect");
        let request = format!(
            "POST /api/invoke HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.expect("write");
        let mut response = String::new();
        stream.read_to_string(&mut response).await.expect("read");
        response
    }
}
