use crate::model::{LOCAL_FAKE_PROVIDER_ENDPOINT, ProviderProfile};
use linguamesh_domain::{
    ErrorKind, ModelDescriptor, TranslationError, TranslationEvent, TranslationRequest,
};
use linguamesh_engine::{CancellationHandle, TranslationEngine, TranslationOperation};
use linguamesh_provider_openai::{OpenAiCompatibleProvider, OpenAiConfig};
use linguamesh_testkit::FakeProviderServer;
use std::error::Error;
use std::fmt;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{Receiver as CommandReceiver, Sender as CommandSender};

const COMMAND_CAPACITY: usize = 16;
const EVENT_CAPACITY: usize = 64;

/// 描述发送给共享核心工作线程的命令。
pub enum WorkerCommand {
    /// 连接新的会话内提供商。
    Connect(ProviderProfile),
    /// 开始翻译请求。
    Translate(TranslationRequest),
    /// 取消当前翻译。
    Cancel,
    /// 停止工作线程和本地提供商。
    Shutdown,
}

/// 描述从共享核心传回原生主线程的事件。
pub enum WorkerEvent {
    /// 本地提供商已连接并返回模型。
    Connected(Vec<ModelDescriptor>),
    /// 共享核心产生翻译事件。
    Translation(TranslationEvent),
    /// 核心事件流在没有终止事件时异常结束。
    OperationFailed(TranslationError),
    /// 新提供商连接被拒绝且现有操作不受影响。
    ProviderRejected(TranslationError),
    /// 工作线程拒绝命令或无法启动。
    Rejected(TranslationError),
    /// 工作线程已干净停止。
    Stopped,
}

/// 表示界面无法非阻塞提交命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkerSendError;

impl fmt::Display for WorkerSendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("The core command queue is unavailable or full.")
    }
}

impl Error for WorkerSendError {}

/// 管理不阻塞原生主线程的共享核心运行时。
pub struct CoreWorker {
    commands: CommandSender<WorkerCommand>,
    events: Receiver<WorkerEvent>,
    active_cancellation: Arc<Mutex<Option<CancellationHandle>>>,
    _thread: JoinHandle<()>,
}

impl CoreWorker {
    /// 启动独立运行时和回环假提供商。
    #[must_use]
    pub fn spawn() -> Self {
        let (commands, command_receiver) = tokio::sync::mpsc::channel(COMMAND_CAPACITY);
        let (event_sender, events) = mpsc::sync_channel(EVENT_CAPACITY);
        let startup_events = event_sender.clone();
        let active_cancellation = Arc::new(Mutex::new(None));
        let worker_cancellation = Arc::clone(&active_cancellation);
        let thread = std::thread::spawn(move || {
            let runtime = Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build();
            match runtime {
                Ok(runtime) => runtime.block_on(run_worker(
                    command_receiver,
                    event_sender,
                    worker_cancellation,
                )),
                Err(error) => {
                    let _ = startup_events.send(WorkerEvent::Rejected(TranslationError::new(
                        ErrorKind::Internal,
                        format!("Failed to start the core runtime: {error}"),
                    )));
                    let _ = startup_events.send(WorkerEvent::Stopped);
                }
            }
        });
        Self {
            commands,
            events,
            active_cancellation,
            _thread: thread,
        }
    }

    /// 非阻塞提交界面命令。
    pub fn try_send(&self, command: WorkerCommand) -> Result<(), WorkerSendError> {
        if let WorkerCommand::Cancel = &command
            && let Some(cancellation) = self
                .active_cancellation
                .lock()
                .ok()
                .and_then(|active| active.clone())
        {
            cancellation.cancel();
            return Ok(());
        }
        self.commands.try_send(command).map_err(|_| WorkerSendError)
    }

    /// 非阻塞接收下一条核心事件。
    pub fn try_recv(&self) -> Result<WorkerEvent, TryRecvError> {
        self.events.try_recv()
    }
}

impl Drop for CoreWorker {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active_cancellation.lock()
            && let Some(cancellation) = active.take()
        {
            cancellation.cancel();
        }
        let _ = self.commands.try_send(WorkerCommand::Shutdown);
    }
}

enum ActiveStep {
    Command(Option<WorkerCommand>),
    Event(Option<TranslationEvent>),
}

async fn run_worker(
    mut commands: CommandReceiver<WorkerCommand>,
    events: SyncSender<WorkerEvent>,
    active_cancellation: Arc<Mutex<Option<CancellationHandle>>>,
) {
    let (server, mut engine, models) = match start_default_provider().await {
        Ok(started) => started,
        Err(error) => {
            let _ = events.send(WorkerEvent::Rejected(error));
            let _ = events.send(WorkerEvent::Stopped);
            return;
        }
    };
    if events.send(WorkerEvent::Connected(models)).is_err() {
        server.shutdown().await;
        return;
    }

    let mut active: Option<TranslationOperation> = None;
    loop {
        if let Some(operation) = active.as_mut() {
            let step = tokio::select! {
                biased;
                command = commands.recv() => ActiveStep::Command(command),
                event = operation.next_event() => ActiveStep::Event(event),
            };
            match step {
                ActiveStep::Command(Some(WorkerCommand::Cancel)) => operation.cancel(),
                ActiveStep::Command(Some(WorkerCommand::Connect(_))) => {
                    let _ = events.send(WorkerEvent::ProviderRejected(TranslationError::new(
                        ErrorKind::Internal,
                        "A provider cannot be changed while a translation is running.",
                    )));
                }
                ActiveStep::Command(Some(WorkerCommand::Translate(_))) => {
                    let _ = events.send(WorkerEvent::Rejected(TranslationError::new(
                        ErrorKind::Internal,
                        "A translation is already running.",
                    )));
                }
                ActiveStep::Command(Some(WorkerCommand::Shutdown) | None) => break,
                ActiveStep::Event(Some(event)) => {
                    let terminal = event.is_terminal();
                    if events.send(WorkerEvent::Translation(event)).is_err() {
                        break;
                    }
                    if terminal {
                        set_active_cancellation(&active_cancellation, None);
                        active = None;
                    }
                }
                ActiveStep::Event(None) => {
                    set_active_cancellation(&active_cancellation, None);
                    active = None;
                    let _ = events.send(WorkerEvent::OperationFailed(TranslationError::new(
                        ErrorKind::Internal,
                        "The core event stream ended without a terminal event.",
                    )));
                }
            }
        } else {
            match commands.recv().await {
                Some(WorkerCommand::Connect(profile)) => match connect_profile(
                    &profile,
                    if profile.endpoint() == LOCAL_FAKE_PROVIDER_ENDPOINT {
                        Some(server.base_url())
                    } else {
                        None
                    },
                )
                .await
                {
                    Ok((next_engine, models)) => {
                        engine = next_engine;
                        if events.send(WorkerEvent::Connected(models)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        if events.send(WorkerEvent::ProviderRejected(error)).is_err() {
                            break;
                        }
                    }
                },
                Some(WorkerCommand::Translate(request)) => {
                    let operation = engine.translate(request);
                    set_active_cancellation(
                        &active_cancellation,
                        Some(operation.cancellation_handle()),
                    );
                    active = Some(operation);
                }
                Some(WorkerCommand::Cancel) => {}
                Some(WorkerCommand::Shutdown) | None => break,
            }
        }
    }
    if let Some(operation) = active {
        operation.cancel();
    }
    set_active_cancellation(&active_cancellation, None);
    server.shutdown().await;
    let _ = events.send(WorkerEvent::Stopped);
}

async fn start_default_provider()
-> Result<(FakeProviderServer, TranslationEngine, Vec<ModelDescriptor>), TranslationError> {
    let server = FakeProviderServer::start().await.map_err(|error| {
        TranslationError::new(
            ErrorKind::Network,
            format!("Failed to start the loopback provider: {error}"),
        )
    })?;
    match connect_endpoint(&server.base_url()).await {
        Ok((engine, models)) => Ok((server, engine, models)),
        Err(error) => {
            server.shutdown().await;
            Err(error)
        }
    }
}

async fn connect_profile(
    profile: &ProviderProfile,
    endpoint_override: Option<String>,
) -> Result<(TranslationEngine, Vec<ModelDescriptor>), TranslationError> {
    let endpoint = endpoint_override
        .as_deref()
        .unwrap_or_else(|| profile.endpoint());
    connect_endpoint(endpoint).await
}

async fn connect_endpoint(
    endpoint: &str,
) -> Result<(TranslationEngine, Vec<ModelDescriptor>), TranslationError> {
    let provider = OpenAiCompatibleProvider::new(OpenAiConfig::without_credential(endpoint))?;
    let engine = TranslationEngine::new(std::sync::Arc::new(provider));
    let models = engine.list_models().await?;
    if models.is_empty() {
        return Err(TranslationError::new(
            ErrorKind::ModelUnavailable,
            "The provider returned no models.",
        ));
    }
    Ok((engine, models))
}

fn set_active_cancellation(
    active_cancellation: &Mutex<Option<CancellationHandle>>,
    cancellation: Option<CancellationHandle>,
) {
    if let Ok(mut active) = active_cancellation.lock() {
        *active = cancellation;
    }
}

#[cfg(test)]
mod tests {
    use super::{CoreWorker, WorkerCommand, WorkerEvent};
    use crate::model::ProviderProfile;
    use linguamesh_domain::{ErrorKind, TranslationEvent, TranslationRequest};
    use linguamesh_testkit::FakeProviderServer;
    use std::sync::mpsc;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};
    use tokio::runtime::Builder;
    use tokio::sync::oneshot;

    struct ExternalFakeProvider {
        endpoint: String,
        shutdown: Option<oneshot::Sender<()>>,
        thread: Option<JoinHandle<()>>,
    }

    impl ExternalFakeProvider {
        fn start() -> Self {
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            let (shutdown, shutdown_receiver) = oneshot::channel();
            let thread = std::thread::spawn(move || {
                let runtime = Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("external provider runtime");
                runtime.block_on(async move {
                    let server = FakeProviderServer::start()
                        .await
                        .expect("external fake provider");
                    ready_sender
                        .send(server.base_url())
                        .expect("provider endpoint");
                    let _ = shutdown_receiver.await;
                    server.shutdown().await;
                });
            });
            let endpoint = ready_receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("provider startup");
            Self {
                endpoint,
                shutdown: Some(shutdown),
                thread: Some(thread),
            }
        }
    }

    impl Drop for ExternalFakeProvider {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
            if let Some(thread) = self.thread.take() {
                thread.join().expect("provider shutdown");
            }
        }
    }

    fn connected_worker() -> (CoreWorker, Vec<linguamesh_domain::ModelDescriptor>) {
        let worker = CoreWorker::spawn();
        let event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("connection event");
        let WorkerEvent::Connected(models) = event else {
            panic!("expected connection event");
        };
        (worker, models)
    }

    #[test]
    fn real_core_stream_completes_through_worker() {
        let (worker, models) = connected_worker();
        assert!(models.iter().any(|model| model.id == "fake-translator"));
        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-translator",
            )))
            .expect("submit");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = String::new();
        let mut terminal = None;
        while Instant::now() < deadline && terminal.is_none() {
            let event = worker
                .events
                .recv_timeout(Duration::from_millis(500))
                .expect("translation event");
            if let WorkerEvent::Translation(event) = event {
                match event {
                    TranslationEvent::TextDelta { text, .. } => output.push_str(&text),
                    event if event.is_terminal() => terminal = Some(event),
                    _ => {}
                }
            }
        }
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, Some(TranslationEvent::Completed { .. })));
    }

    #[test]
    fn embedded_provider_can_be_reconnected_and_translated() {
        let (worker, _) = connected_worker();
        worker
            .try_send(WorkerCommand::Connect(ProviderProfile::local_fake()))
            .expect("connect");
        let event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("connection result");
        let WorkerEvent::Connected(models) = event else {
            panic!("expected connection success");
        };
        assert!(models.iter().any(|model| model.id == "fake-translator"));

        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-translator",
            )))
            .expect("translate");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = String::new();
        let mut terminal = None;
        while Instant::now() < deadline && terminal.is_none() {
            let event = worker
                .events
                .recv_timeout(Duration::from_millis(500))
                .expect("translation event");
            if let WorkerEvent::Translation(event) = event {
                match event {
                    TranslationEvent::TextDelta { text, .. } => output.push_str(&text),
                    event if event.is_terminal() => terminal = Some(event),
                    _ => {}
                }
            }
        }
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, Some(TranslationEvent::Completed { .. })));
    }

    #[test]
    fn worker_cancellation_retains_partial_output() {
        let (worker, _) = connected_worker();
        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-slow-translator",
            )))
            .expect("submit");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = String::new();
        let mut terminal = None;
        while Instant::now() < deadline && terminal.is_none() {
            let event = worker
                .events
                .recv_timeout(Duration::from_millis(500))
                .expect("translation event");
            if let WorkerEvent::Translation(event) = event {
                match event {
                    TranslationEvent::TextDelta { text, .. } => {
                        output.push_str(&text);
                        worker.try_send(WorkerCommand::Cancel).expect("cancel");
                    }
                    event if event.is_terminal() => terminal = Some(event),
                    _ => {}
                }
            }
        }
        assert!(!output.is_empty());
        assert!(matches!(terminal, Some(TranslationEvent::Cancelled { .. })));
    }

    #[test]
    fn cancellation_after_terminal_event_is_idempotent() {
        let (worker, _) = connected_worker();
        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-translator",
            )))
            .expect("submit");
        loop {
            let event = worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("translation event");
            if matches!(
                event,
                WorkerEvent::Translation(TranslationEvent::Completed { .. })
            ) {
                break;
            }
        }

        worker.try_send(WorkerCommand::Cancel).expect("cancel");

        assert!(matches!(
            worker.events.recv_timeout(Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
    }

    #[test]
    fn connection_switches_to_discovered_loopback_provider() {
        let external = ExternalFakeProvider::start();
        let (worker, _) = connected_worker();
        worker
            .try_send(WorkerCommand::Connect(ProviderProfile::new(
                "external-loopback",
                "External loopback provider",
                external.endpoint.clone(),
            )))
            .expect("connect");

        let event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("connection result");
        let WorkerEvent::Connected(models) = event else {
            panic!("expected connection success");
        };
        assert!(models.iter().any(|model| model.id == "fake-translator"));

        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-translator",
            )))
            .expect("translate");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = String::new();
        let mut terminal = None;
        while Instant::now() < deadline && terminal.is_none() {
            let event = worker
                .events
                .recv_timeout(Duration::from_millis(500))
                .expect("translation event");
            if let WorkerEvent::Translation(event) = event {
                match event {
                    TranslationEvent::TextDelta { text, .. } => output.push_str(&text),
                    event if event.is_terminal() => terminal = Some(event),
                    _ => {}
                }
            }
        }
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, Some(TranslationEvent::Completed { .. })));
    }

    #[test]
    fn failed_connection_keeps_previous_provider_usable() {
        let (worker, _) = connected_worker();
        worker
            .try_send(WorkerCommand::Connect(ProviderProfile::new(
                "invalid-provider",
                "Invalid provider",
                "not a valid endpoint",
            )))
            .expect("connect");

        let event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("connection rejection");
        let WorkerEvent::ProviderRejected(error) = event else {
            panic!("expected connection rejection");
        };
        assert_eq!(error.kind, ErrorKind::InvalidEndpoint);

        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-translator",
            )))
            .expect("translate with previous provider");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = String::new();
        let mut terminal = None;
        while Instant::now() < deadline && terminal.is_none() {
            let event = worker
                .events
                .recv_timeout(Duration::from_millis(500))
                .expect("translation event");
            if let WorkerEvent::Translation(event) = event {
                match event {
                    TranslationEvent::TextDelta { text, .. } => output.push_str(&text),
                    event if event.is_terminal() => terminal = Some(event),
                    _ => {}
                }
            }
        }
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, Some(TranslationEvent::Completed { .. })));
    }

    #[test]
    fn active_translation_rejects_provider_change() {
        let (worker, _) = connected_worker();
        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-slow-translator",
            )))
            .expect("translate");

        loop {
            let event = worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("translation event");
            if matches!(
                event,
                WorkerEvent::Translation(TranslationEvent::TextDelta { .. })
            ) {
                break;
            }
        }

        worker
            .try_send(WorkerCommand::Connect(ProviderProfile::local_fake()))
            .expect("connect command");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut rejected = false;
        while Instant::now() < deadline && !rejected {
            let event = worker
                .events
                .recv_timeout(Duration::from_millis(500))
                .expect("worker event");
            if let WorkerEvent::ProviderRejected(error) = event {
                assert_eq!(error.kind, ErrorKind::Internal);
                rejected = true;
            }
        }
        assert!(rejected);

        let completed = loop {
            let event = worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("terminal event");
            if matches!(
                event,
                WorkerEvent::Translation(TranslationEvent::Completed { .. })
            ) {
                break true;
            }
        };
        assert!(completed);
    }
}
