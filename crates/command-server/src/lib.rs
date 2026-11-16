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

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::broadcast;

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

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: CommandErrorCode::BadRequest,
            message: message.into(),
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

    /// Registers a command whose implementation takes and returns typed values.
    ///
    /// This exists so a command is written **once** as an ordinary async
    /// function over `(host, input) -> Result<Output, _>`; the JSON seam
    /// (`from_value` / `to_value`) is generated here instead of being hand
    /// written per command. That is what keeps the transport from growing a
    /// second, drifting copy of the command surface the way a per-command
    /// adapter crate does.
    pub fn register_typed<In, Out, F, Fut>(&mut self, name: &'static str, handler: F)
    where
        In: serde::de::DeserializeOwned + Send + 'static,
        Out: Serialize + 'static,
        F: Fn(Arc<S>, In) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<Out, CommandError>> + Send + 'static,
    {
        let handler = Arc::new(handler);
        self.register(name, move |state, args| {
            let handler = Arc::clone(&handler);
            async move {
                let input: In = serde_json::from_value(args)
                    .map_err(|error| CommandError::bad_request(error.to_string()))?;
                let output = handler(state, input).await?;
                serde_json::to_value(output)
                    .map_err(|error| CommandError::failed(error.to_string()))
            }
        });
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

/// Broadcast channel for the push half of the contract.
///
/// The desktop uses Tauri's `listen`; the cloud server uses `GET /api/events`.
/// This is the same idea: the host publishes `(eventName, payload)` and every
/// connected UI receives it. Event names are the `EVENT_NAME` constants from
/// `vrcx-0-runtime-event`, so the two transports stay in sync.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<(String, Value)>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn publish(&self, event: impl Into<String>, payload: Value) {
        // A send error only means nobody is listening right now.
        let _ = self.sender.send((event.into(), payload));
    }

    pub fn subscribe(&self) -> broadcast::Receiver<(String, Value)> {
        self.sender.subscribe()
    }
}

async fn events_endpoint<S>(ws: WebSocketUpgrade, State(bus): State<Arc<CommandBus<S>>>) -> Response
where
    S: Send + Sync + 'static,
{
    let events = bus.events.clone();
    ws.on_upgrade(move |socket| forward_events(socket, events.subscribe()))
}

async fn forward_events(mut socket: WebSocket, mut receiver: broadcast::Receiver<(String, Value)>) {
    loop {
        match receiver.recv().await {
            Ok((event, payload)) => {
                let frame = json!({ "event": event, "payload": payload }).to_string();
                if socket.send(Message::Text(frame.into())).await.is_err() {
                    return;
                }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "event subscriber lagged");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// Shared router state: the registry plus the host it dispatches against.
pub struct CommandBus<S> {
    pub registry: Arc<CommandRegistry<S>>,
    pub state: Arc<S>,
    pub events: EventBus,
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
        .route("/api/events", get(events_endpoint::<S>))
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
            events: EventBus::new(64),
            state: Arc::new(FakeHost {
                greeting: "hello".into(),
            }),
        })
    }

    #[tokio::test]
    async fn events_are_broadcast_to_subscribers() {
        let bus = EventBus::new(8);
        let mut first = bus.subscribe();
        let mut second = bus.subscribe();

        bus.publish("authenticatedSessionProjection", json!({ "phase": "ready" }));

        for receiver in [&mut first, &mut second] {
            let (event, payload) = receiver.recv().await.expect("event");
            assert_eq!(event, "authenticatedSessionProjection");
            assert_eq!(payload["phase"], "ready");
        }
    }

    #[tokio::test]
    async fn typed_registration_maps_arguments_and_results() {
        #[derive(serde::Deserialize)]
        struct Input {
            name: String,
        }
        #[derive(Serialize)]
        struct Output {
            greeting: String,
        }

        async fn greet(
            state: Arc<FakeHost>,
            input: Input,
        ) -> Result<Output, CommandError> {
            Ok(Output {
                greeting: format!("{}, {}", state.greeting, input.name),
            })
        }

        let mut registry = CommandRegistry::new();
        registry.register_typed::<Input, Output, _, _>("app__greet", greet);

        let future = registry
            .dispatch(
                Arc::new(FakeHost {
                    greeting: "hi".into(),
                }),
                "app__greet",
                json!({ "name": "vrcx" }),
            )
            .expect("registered");
        let value = future.await.expect("ok");
        assert_eq!(value["greeting"], "hi, vrcx");
    }

    #[tokio::test]
    async fn typed_registration_rejects_bad_arguments() {
        #[derive(serde::Deserialize)]
        struct Input {
            #[allow(dead_code)]
            name: String,
        }

        async fn noop(
            _state: Arc<FakeHost>,
            _input: Input,
        ) -> Result<Value, CommandError> {
            Ok(Value::Null)
        }

        let mut registry = CommandRegistry::new();
        registry.register_typed::<Input, Value, _, _>("app__noop", noop);

        let future = registry
            .dispatch(
                Arc::new(FakeHost {
                    greeting: "hi".into(),
                }),
                "app__noop",
                json!({ "wrong": 1 }),
            )
            .expect("registered");
        let error = future.await.expect_err("must reject");
        assert!(matches!(error.code, CommandErrorCode::BadRequest));
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

