use crate::model::{ProviderProfile, ProviderProfileId};
use linguamesh_application::{HostSecretRequests, ProviderManager, host_secret_channel};
use linguamesh_domain::{
    CompatibilityRequirements, CoreCompatibility, ErrorKind, ModelDescriptor, SecretValue,
    TranslationError, TranslationEvent, TranslationRequest,
};
use linguamesh_engine::{CancellationHandle, TranslationOperation, core_compatibility};
use linguamesh_testkit::FakeProviderServer;
use std::error::Error;
use std::fmt;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{Receiver as CommandReceiver, Sender as CommandSender};
use tokio_util::sync::CancellationToken;

const COMMAND_CAPACITY: usize = 16;
const EVENT_CAPACITY: usize = 64;
const SECRET_REQUEST_CAPACITY: usize = 1;
const REVIEWED_CORE_VERSION: &str = "0.1.0-alpha.2";
const REVIEWED_ABI_MAJOR: u32 = 1;
const REVIEWED_PROTOCOL_VERSION: u32 = 1;
const REVIEWED_PROVIDER_CATALOG_VERSION: &str = "0.1.0";
const REQUIRED_CORE_FEATURES: [&str; 6] = [
    "cancellation_v1",
    "compatibility_negotiation_v1",
    "typed_rust_host_secret_broker_v1",
    "model_discovery_v1",
    "streaming_text_v1",
    "text_translation_v1",
];

/// 描述连接配置是否应跨进程重启保留。
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum PersistenceIntent {
    /// 配置和秘密仅在当前进程内存中存活。
    SessionOnly,
    /// 配置和秘密应使用平台安全存储保留。
    Persistent,
}

/// 描述发送给共享核心工作线程的命令。
pub enum WorkerCommand {
    /// 连接一个已验证的规范提供商配置。
    Connect {
        /// 不含秘密值的核心配置。
        profile: ProviderProfile,
        /// 只允许消费一次的会话秘密。
        secret: Option<SecretValue>,
        /// 用户明确选择的持久化行为。
        persistence: PersistenceIntent,
    },
    /// 明确选择当前提供商已经发现的模型。
    SelectModel(String),
    /// 开始翻译请求。
    Translate(TranslationRequest),
    /// 取消当前连接或翻译。
    Cancel,
    /// 停止工作线程和本地提供商。
    Shutdown,
}

/// 描述从共享核心传回原生主线程的事件。
pub enum WorkerEvent {
    /// 内建假提供商已启动，但尚未建立应用连接。
    DemoProviderReady {
        /// 当前进程专用的回环基础端点。
        endpoint: String,
    },
    /// 提供商已连接并返回模型。
    Connected {
        /// 已由核心成功连接的规范配置。
        profile: ProviderProfile,
        /// 核心发现的模型。
        models: Vec<ModelDescriptor>,
    },
    /// 工作线程已确认用户选择的模型。
    ModelSelected {
        /// 当前活动提供商配置标识。
        profile_id: ProviderProfileId,
        /// 已确认的模型标识。
        model_id: String,
    },
    /// 共享核心产生翻译事件。
    Translation(TranslationEvent),
    /// 核心事件流在没有终止事件时异常结束。
    OperationFailed(TranslationError),
    /// 翻译命令在创建核心操作之前被拒绝。
    TranslationRejected(TranslationError),
    /// 候选提供商连接被拒绝且现有会话不受影响。
    ProviderRejected {
        /// 被拒绝的候选配置标识。
        profile_id: ProviderProfileId,
        /// 不包含秘密的类型化错误。
        error: TranslationError,
    },
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

enum QueuedCommand {
    Connect {
        profile: ProviderProfile,
        secret: Option<SecretValue>,
        persistence: PersistenceIntent,
        cancellation: CancellationToken,
    },
    SelectModel(String),
    Translate(TranslationRequest),
    Cancel,
    Shutdown,
}

enum ActiveCancellation {
    Connection(CancellationToken),
    Translation(CancellationHandle),
}

impl ActiveCancellation {
    fn cancel(&self) {
        match self {
            Self::Connection(cancellation) => cancellation.cancel(),
            Self::Translation(cancellation) => cancellation.cancel(),
        }
    }
}

/// 管理不阻塞原生主线程的共享核心运行时。
pub struct CoreWorker {
    commands: CommandSender<QueuedCommand>,
    events: Receiver<WorkerEvent>,
    active_cancellation: Arc<Mutex<Option<ActiveCancellation>>>,
    shutdown_cancellation: CancellationToken,
    _thread: JoinHandle<()>,
}

impl CoreWorker {
    /// 启动独立运行时和仅供显式连接的回环假提供商。
    #[must_use]
    pub fn spawn() -> Self {
        let (commands, command_receiver) = tokio::sync::mpsc::channel(COMMAND_CAPACITY);
        let (event_sender, events) = mpsc::sync_channel(EVENT_CAPACITY);
        let startup_events = event_sender.clone();
        let active_cancellation = Arc::new(Mutex::new(None));
        let shutdown_cancellation = CancellationToken::new();
        let worker_cancellation = Arc::clone(&active_cancellation);
        let worker_shutdown = shutdown_cancellation.clone();
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
                    worker_shutdown,
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
            shutdown_cancellation,
            _thread: thread,
        }
    }

    /// 非阻塞提交界面命令。
    pub fn try_send(&self, command: WorkerCommand) -> Result<(), WorkerSendError> {
        match command {
            WorkerCommand::Cancel => {
                if let Ok(active) = self.active_cancellation.lock()
                    && let Some(cancellation) = active.as_ref()
                {
                    cancellation.cancel();
                    return Ok(());
                }
                self.commands
                    .try_send(QueuedCommand::Cancel)
                    .map_err(|_| WorkerSendError)
            }
            WorkerCommand::Connect {
                profile,
                secret,
                persistence,
            } => {
                let cancellation = self.shutdown_cancellation.child_token();
                let installed = install_connection_cancellation_if_idle(
                    &self.active_cancellation,
                    cancellation.clone(),
                );
                let result = self.commands.try_send(QueuedCommand::Connect {
                    profile,
                    secret,
                    persistence,
                    cancellation,
                });
                if result.is_err() && installed {
                    clear_active_cancellation(&self.active_cancellation);
                }
                result.map_err(|_| WorkerSendError)
            }
            WorkerCommand::SelectModel(model_id) => self
                .commands
                .try_send(QueuedCommand::SelectModel(model_id))
                .map_err(|_| WorkerSendError),
            WorkerCommand::Translate(request) => self
                .commands
                .try_send(QueuedCommand::Translate(request))
                .map_err(|_| WorkerSendError),
            WorkerCommand::Shutdown => {
                self.shutdown_cancellation.cancel();
                cancel_active(&self.active_cancellation);
                self.commands
                    .try_send(QueuedCommand::Shutdown)
                    .map_err(|_| WorkerSendError)
            }
        }
    }

    /// 非阻塞接收下一条核心事件。
    pub fn try_recv(&self) -> Result<WorkerEvent, TryRecvError> {
        self.events.try_recv()
    }
}

impl Drop for CoreWorker {
    fn drop(&mut self) {
        self.shutdown_cancellation.cancel();
        if let Ok(mut active) = self.active_cancellation.lock()
            && let Some(cancellation) = active.take()
        {
            cancellation.cancel();
        }
        let _ = self.commands.try_send(QueuedCommand::Shutdown);
    }
}

enum ActiveStep {
    Shutdown,
    Command(Option<QueuedCommand>),
    Event(Option<TranslationEvent>),
}

// 集中保留命令与事件优先级，避免拆分后破坏单一活动操作约束。
#[allow(clippy::too_many_lines)]
async fn run_worker(
    mut commands: CommandReceiver<QueuedCommand>,
    events: SyncSender<WorkerEvent>,
    active_cancellation: Arc<Mutex<Option<ActiveCancellation>>>,
    shutdown_cancellation: CancellationToken,
) {
    if let Err(error) = validate_current_core_contract() {
        let _ = events.send(WorkerEvent::Rejected(error));
        let _ = events.send(WorkerEvent::Stopped);
        return;
    }
    let (secret_broker, mut secret_requests) = match host_secret_channel(SECRET_REQUEST_CAPACITY) {
        Ok(channel) => channel,
        Err(error) => {
            let _ = events.send(WorkerEvent::Rejected(error));
            let _ = events.send(WorkerEvent::Stopped);
            return;
        }
    };
    let server = match FakeProviderServer::start().await {
        Ok(server) => server,
        Err(error) => {
            let _ = events.send(WorkerEvent::Rejected(TranslationError::new(
                ErrorKind::Network,
                format!("Failed to start the loopback provider: {error}"),
            )));
            let _ = events.send(WorkerEvent::Stopped);
            return;
        }
    };
    if events
        .send(WorkerEvent::DemoProviderReady {
            endpoint: server.base_url(),
        })
        .is_err()
    {
        server.shutdown().await;
        return;
    }

    let mut manager = ProviderManager::new(secret_broker.clone());
    let mut selected_model: Option<String> = None;
    let mut active: Option<TranslationOperation> = None;
    let mut shutting_down = false;
    let mut stop_after_active = false;
    while !shutting_down {
        if let Some(operation) = active.as_mut() {
            let step = tokio::select! {
                biased;
                () = shutdown_cancellation.cancelled(), if !stop_after_active => ActiveStep::Shutdown,
                command = commands.recv(), if !stop_after_active => ActiveStep::Command(command),
                event = operation.next_event() => ActiveStep::Event(event),
            };
            match step {
                ActiveStep::Command(Some(QueuedCommand::Cancel)) => operation.cancel(),
                ActiveStep::Command(Some(QueuedCommand::Connect { profile, .. })) => {
                    let _ = events.send(WorkerEvent::ProviderRejected {
                        profile_id: profile.id().clone(),
                        error: TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "A provider cannot be changed while a translation is running.",
                        ),
                    });
                }
                ActiveStep::Command(Some(QueuedCommand::SelectModel(_))) => {
                    let _ = events.send(WorkerEvent::Rejected(TranslationError::new(
                        ErrorKind::InvalidConfiguration,
                        "A model cannot be changed while a translation is running.",
                    )));
                }
                ActiveStep::Command(Some(QueuedCommand::Translate(_))) => {
                    let _ = events.send(WorkerEvent::Rejected(TranslationError::new(
                        ErrorKind::InvalidConfiguration,
                        "A translation is already running.",
                    )));
                }
                ActiveStep::Shutdown
                | ActiveStep::Command(Some(QueuedCommand::Shutdown) | None) => {
                    operation.cancel();
                    stop_after_active = true;
                }
                ActiveStep::Event(Some(event)) => {
                    let terminal = event.is_terminal();
                    if events.send(WorkerEvent::Translation(event)).is_err() {
                        shutting_down = true;
                    }
                    if terminal {
                        clear_active_cancellation(&active_cancellation);
                        active = None;
                        if stop_after_active {
                            shutting_down = true;
                        }
                    }
                }
                ActiveStep::Event(None) => {
                    clear_active_cancellation(&active_cancellation);
                    active = None;
                    let _ = events.send(WorkerEvent::OperationFailed(TranslationError::new(
                        ErrorKind::Internal,
                        "The core event stream ended without a terminal event.",
                    )));
                    if stop_after_active {
                        shutting_down = true;
                    }
                }
            }
            continue;
        }

        let command = tokio::select! {
            biased;
            () = shutdown_cancellation.cancelled() => None,
            command = commands.recv() => command,
        };
        match command {
            Some(QueuedCommand::Connect {
                profile,
                secret,
                persistence,
                cancellation,
            }) => {
                set_active_cancellation(
                    &active_cancellation,
                    ActiveCancellation::Connection(cancellation.clone()),
                );
                let profile_id = profile.id().clone();
                let mut candidate = ProviderManager::new(secret_broker.clone());
                let result = connect_candidate(
                    &mut candidate,
                    &profile,
                    secret,
                    persistence,
                    &cancellation,
                    &mut secret_requests,
                )
                .await;
                clear_active_cancellation(&active_cancellation);
                match result {
                    Ok(models) => {
                        selected_model = profile
                            .selected_model()
                            .filter(|selected| models.iter().any(|model| model.id == *selected))
                            .map(str::to_owned);
                        manager = candidate;
                        if events
                            .send(WorkerEvent::Connected { profile, models })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::ProviderRejected { profile_id, error })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::SelectModel(model_id)) => {
                let result = select_model(&manager, &model_id);
                match result {
                    Ok(profile_id) => {
                        selected_model = Some(model_id.clone());
                        if events
                            .send(WorkerEvent::ModelSelected {
                                profile_id,
                                model_id,
                            })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events.send(WorkerEvent::Rejected(error)).is_err() {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::Translate(request)) => {
                match begin_translation(&manager, selected_model.as_deref(), request) {
                    Ok(operation) => {
                        set_active_cancellation(
                            &active_cancellation,
                            ActiveCancellation::Translation(operation.cancellation_handle()),
                        );
                        active = Some(operation);
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::Cancel) => {}
            Some(QueuedCommand::Shutdown) | None => shutting_down = true,
        }
    }

    if let Some(operation) = active {
        operation.cancel();
    }
    reject_queued_commands_for_shutdown(&mut commands, &events);
    clear_active_cancellation(&active_cancellation);
    manager.disconnect();
    server.shutdown().await;
    let _ = events.send(WorkerEvent::Stopped);
}

fn reject_queued_commands_for_shutdown(
    commands: &mut CommandReceiver<QueuedCommand>,
    events: &SyncSender<WorkerEvent>,
) {
    while let Ok(command) = commands.try_recv() {
        let result = match command {
            QueuedCommand::Connect { profile, .. } => events.send(WorkerEvent::ProviderRejected {
                profile_id: profile.id().clone(),
                error: TranslationError::cancelled(),
            }),
            QueuedCommand::SelectModel(_) => {
                events.send(WorkerEvent::Rejected(TranslationError::cancelled()))
            }
            QueuedCommand::Translate(_) => events.send(WorkerEvent::TranslationRejected(
                TranslationError::cancelled(),
            )),
            QueuedCommand::Cancel | QueuedCommand::Shutdown => continue,
        };
        if result.is_err() {
            break;
        }
    }
}

fn validate_current_core_contract() -> Result<(), TranslationError> {
    let actual = core_compatibility().map_err(|error| {
        TranslationError::new(
            ErrorKind::ProtocolIncompatible,
            format!("Core compatibility could not be read: {}", error.message),
        )
    })?;
    validate_core_contract(&actual)
}

fn validate_core_contract(actual: &CoreCompatibility) -> Result<(), TranslationError> {
    let requirements = CompatibilityRequirements {
        core_version: REVIEWED_CORE_VERSION.to_owned(),
        abi_major: REVIEWED_ABI_MAJOR,
        protocol_version: REVIEWED_PROTOCOL_VERSION,
        provider_catalog_version: REVIEWED_PROVIDER_CATALOG_VERSION.to_owned(),
        required_features: REQUIRED_CORE_FEATURES
            .iter()
            .map(|feature| (*feature).to_owned())
            .collect(),
    };
    requirements.validate(actual).map_err(|error| {
        TranslationError::new(
            ErrorKind::ProtocolIncompatible,
            format!("The shared Core contract is incompatible: {error}"),
        )
    })
}

async fn connect_candidate(
    manager: &mut ProviderManager,
    profile: &ProviderProfile,
    mut session_secret: Option<SecretValue>,
    persistence: PersistenceIntent,
    cancellation: &CancellationToken,
    requests: &mut HostSecretRequests,
) -> Result<Vec<ModelDescriptor>, TranslationError> {
    if profile
        .secret_ref()
        .is_some_and(linguamesh_domain::SecretRef::is_persistent)
    {
        return Err(TranslationError::new(
            ErrorKind::SecureStorageUnavailable,
            "Secure credential storage is unavailable.",
        ));
    }
    if persistence == PersistenceIntent::Persistent {
        return Err(TranslationError::new(
            ErrorKind::Persistence,
            "Persistent Linux provider profiles are not available in this checkpoint.",
        ));
    }
    if session_secret.is_some() && profile.secret_ref().is_none() {
        return Err(TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "A session credential requires an explicit session secret reference.",
        ));
    }

    let connection = manager.connect(profile, cancellation);
    tokio::pin!(connection);
    loop {
        tokio::select! {
            biased;
            () = cancellation.cancelled() => return Err(TranslationError::cancelled()),
            result = &mut connection => return result,
            request = requests.recv() => {
                let Some(request) = request else {
                    return Err(TranslationError::new(
                        ErrorKind::SecretUnavailable,
                        "The host secret request channel closed.",
                    ));
                };
                let required_ref = request.required().secret_ref.clone();
                let response = if required_ref.is_persistent() {
                    request.reject_secure_storage_unavailable()
                } else if profile.secret_ref() == Some(&required_ref) {
                    match session_secret.take() {
                        Some(secret) => request.provide_secret(secret),
                        None => request.reject_unavailable(),
                    }
                } else {
                    request.reject_unavailable()
                };
                if response.is_err() && cancellation.is_cancelled() {
                    return Err(TranslationError::cancelled());
                }
                if response.is_err() {
                    return Err(TranslationError::new(
                        ErrorKind::SecretUnavailable,
                        "The Core secret request was no longer active.",
                    ));
                }
            }
        }
    }
}

fn select_model(
    manager: &ProviderManager,
    model_id: &str,
) -> Result<ProviderProfileId, TranslationError> {
    let profile_id = manager.active_profile_id().ok_or_else(|| {
        TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "Connect a provider before selecting a model.",
        )
    })?;
    if !manager.models().iter().any(|model| model.id == model_id) {
        return Err(TranslationError::new(
            ErrorKind::ModelUnavailable,
            "The selected model is not available from the active provider.",
        ));
    }
    Ok(profile_id.clone())
}

fn begin_translation(
    manager: &ProviderManager,
    selected_model: Option<&str>,
    request: TranslationRequest,
) -> Result<TranslationOperation, TranslationError> {
    let engine = manager.active_engine().ok_or_else(|| {
        TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "Connect a provider before translating.",
        )
    })?;
    let selected_model = selected_model.ok_or_else(|| {
        TranslationError::new(
            ErrorKind::ModelUnavailable,
            "Select a model before translating.",
        )
    })?;
    if request.model_id != selected_model {
        return Err(TranslationError::new(
            ErrorKind::ModelUnavailable,
            "The translation request does not use the confirmed model selection.",
        ));
    }
    Ok(engine.translate(request))
}

fn install_connection_cancellation_if_idle(
    active_cancellation: &Mutex<Option<ActiveCancellation>>,
    cancellation: CancellationToken,
) -> bool {
    let Ok(mut active) = active_cancellation.lock() else {
        return false;
    };
    if active.is_some() {
        return false;
    }
    *active = Some(ActiveCancellation::Connection(cancellation));
    true
}

fn set_active_cancellation(
    active_cancellation: &Mutex<Option<ActiveCancellation>>,
    cancellation: ActiveCancellation,
) {
    if let Ok(mut active) = active_cancellation.lock() {
        *active = Some(cancellation);
    }
}

fn clear_active_cancellation(active_cancellation: &Mutex<Option<ActiveCancellation>>) {
    if let Ok(mut active) = active_cancellation.lock() {
        *active = None;
    }
}

fn cancel_active(active_cancellation: &Mutex<Option<ActiveCancellation>>) {
    if let Ok(active) = active_cancellation.lock()
        && let Some(cancellation) = active.as_ref()
    {
        cancellation.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        COMMAND_CAPACITY, CoreWorker, PersistenceIntent, QueuedCommand, REQUIRED_CORE_FEATURES,
        WorkerCommand, WorkerEvent, validate_core_contract,
    };
    use crate::model::{ProviderProfile, ProviderProfileId};
    use linguamesh_domain::{
        CoreCompatibility, ErrorKind, SecretRef, SecretRefNamespace, SecretValue, TranslationError,
        TranslationEvent, TranslationRequest,
    };
    use linguamesh_engine::core_compatibility;
    use linguamesh_testkit::FakeProviderServer;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};
    use tokio::runtime::Builder;
    use tokio::sync::oneshot;

    enum FakeMode {
        Standard,
        Authenticated,
        Delayed(Duration),
    }

    struct ExternalFakeProvider {
        endpoint: String,
        model_requests: Arc<AtomicUsize>,
        shutdown: Option<oneshot::Sender<()>>,
        thread: Option<JoinHandle<()>>,
    }

    impl ExternalFakeProvider {
        fn start(mode: FakeMode) -> Self {
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            let (shutdown, shutdown_receiver) = oneshot::channel();
            let thread = std::thread::spawn(move || {
                let runtime = Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("external provider runtime");
                runtime.block_on(async move {
                    let server = match mode {
                        FakeMode::Standard => FakeProviderServer::start().await,
                        FakeMode::Authenticated => {
                            FakeProviderServer::start_requiring_bearer_token(SecretValue::new(
                                "fake-session-secret",
                            ))
                            .await
                        }
                        FakeMode::Delayed(delay) => {
                            FakeProviderServer::start_with_model_delay(delay).await
                        }
                    }
                    .expect("external fake provider");
                    ready_sender
                        .send((server.base_url(), server.model_request_counter()))
                        .expect("provider endpoint");
                    let _ = shutdown_receiver.await;
                    server.shutdown().await;
                });
            });
            let (endpoint, model_requests) = ready_receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("provider startup");
            Self {
                endpoint,
                model_requests,
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

    fn profile(
        id: &str,
        endpoint: &str,
        secret_ref: Option<SecretRef>,
        selected_model: Option<&str>,
    ) -> ProviderProfile {
        ProviderProfile::new(
            ProviderProfileId::parse(id).expect("profile ID"),
            format!("{id} display name"),
            "custom-openai-compatible",
            "openai_chat_completions",
            endpoint,
            secret_ref,
        )
        .expect("profile")
        .with_selected_model(selected_model.map(str::to_owned))
        .expect("selected model")
    }

    fn started_worker() -> (CoreWorker, String) {
        let worker = CoreWorker::spawn();
        let event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("demo provider event");
        let WorkerEvent::DemoProviderReady { endpoint } = event else {
            panic!("expected demo provider readiness");
        };
        (worker, endpoint)
    }

    fn connect(
        worker: &CoreWorker,
        profile: ProviderProfile,
        secret: Option<SecretValue>,
        persistence: PersistenceIntent,
    ) -> Result<Vec<linguamesh_domain::ModelDescriptor>, TranslationError> {
        worker
            .try_send(WorkerCommand::Connect {
                profile,
                secret,
                persistence,
            })
            .expect("connect command");
        match worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("connection result")
        {
            WorkerEvent::Connected { models, .. } => Ok(models),
            WorkerEvent::ProviderRejected { error, .. } => Err(error),
            _ => panic!("unexpected connection event"),
        }
    }

    fn select(worker: &CoreWorker, model_id: &str) {
        worker
            .try_send(WorkerCommand::SelectModel(model_id.to_owned()))
            .expect("model command");
        let event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("model event");
        assert!(matches!(
            event,
            WorkerEvent::ModelSelected { model_id: selected, .. } if selected == model_id
        ));
    }

    fn translate(worker: &CoreWorker, model_id: &str) -> (String, TranslationEvent) {
        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello", "zh-CN", model_id,
            )))
            .expect("translation command");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = String::new();
        while Instant::now() < deadline {
            let event = worker
                .events
                .recv_timeout(Duration::from_millis(500))
                .expect("translation event");
            if let WorkerEvent::Translation(event) = event {
                match event {
                    TranslationEvent::TextDelta { text, .. } => output.push_str(&text),
                    event if event.is_terminal() => return (output, event),
                    _ => {}
                }
            }
        }
        panic!("translation did not terminate");
    }

    #[test]
    fn reviewed_core_contract_is_required_exactly() {
        let actual = core_compatibility().expect("compatibility");
        validate_core_contract(&actual).expect("reviewed contract");
        let mut incompatible = actual;
        incompatible.abi_major += 1;
        let error = validate_core_contract(&incompatible).expect_err("ABI rejection");
        assert_eq!(error.kind, ErrorKind::ProtocolIncompatible);

        let missing_feature = CoreCompatibility {
            core_version: "0.1.0-alpha.2".to_owned(),
            abi_major: 1,
            protocol_version: 1,
            provider_catalog_version: "0.1.0".to_owned(),
            enabled_features: REQUIRED_CORE_FEATURES
                .iter()
                .skip(1)
                .map(|feature| (*feature).to_owned())
                .collect(),
        };
        let error = validate_core_contract(&missing_feature).expect_err("feature rejection");
        assert_eq!(error.kind, ErrorKind::ProtocolIncompatible);
    }

    #[test]
    fn startup_requires_explicit_connection_and_model_selection() {
        let (worker, endpoint) = started_worker();
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_millis(100)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        let models = connect(
            &worker,
            profile("explicit-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        assert!(models.iter().any(|model| model.id == "fake-translator"));

        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-translator",
            )))
            .expect("translation command");
        let event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("selection rejection");
        assert!(matches!(
            event,
            WorkerEvent::TranslationRejected(error) if error.kind == ErrorKind::ModelUnavailable
        ));

        select(&worker, "fake-translator");
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
    }

    #[test]
    fn authenticated_session_secret_is_consumed_once_and_old_session_survives_failure() {
        let external = ExternalFakeProvider::start(FakeMode::Authenticated);
        let (worker, _) = started_worker();
        let secret_ref = SecretRef::new(SecretRefNamespace::Session);
        let authenticated = profile(
            "authenticated-provider",
            &external.endpoint,
            Some(secret_ref.clone()),
            None,
        );
        let error = connect(
            &worker,
            authenticated.clone(),
            Some(SecretValue::new("SESSION_SECRET_CANARY")),
            PersistenceIntent::SessionOnly,
        )
        .expect_err("invalid credential");
        assert_eq!(error.kind, ErrorKind::Authentication);
        assert!(!error.message.contains("SESSION_SECRET_CANARY"));

        connect(
            &worker,
            authenticated.clone(),
            Some(SecretValue::new("fake-session-secret")),
            PersistenceIntent::SessionOnly,
        )
        .expect("authenticated connection");
        select(&worker, "fake-translator");

        let error = connect(&worker, authenticated, None, PersistenceIntent::SessionOnly)
            .expect_err("missing one-shot secret");
        assert_eq!(error.kind, ErrorKind::SecretUnavailable);

        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
    }

    #[test]
    fn unavailable_secret_backends_fail_closed() {
        let external = ExternalFakeProvider::start(FakeMode::Standard);
        let (worker, _) = started_worker();
        let missing_session = profile(
            "missing-session-secret",
            &external.endpoint,
            Some(SecretRef::new(SecretRefNamespace::Session)),
            None,
        );
        let error = connect(
            &worker,
            missing_session,
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect_err("missing secret");
        assert_eq!(error.kind, ErrorKind::SecretUnavailable);

        let persistent_ref = profile(
            "persistent-secret",
            &external.endpoint,
            Some(SecretRef::new(SecretRefNamespace::SecretService)),
            None,
        );
        let error = connect(
            &worker,
            persistent_ref.clone(),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect_err("secure storage unavailable");
        assert_eq!(error.kind, ErrorKind::SecureStorageUnavailable);

        let error = connect(&worker, persistent_ref, None, PersistenceIntent::Persistent)
            .expect_err("persistent secret takes precedence");
        assert_eq!(error.kind, ErrorKind::SecureStorageUnavailable);

        let persistent_profile = profile("persistent-profile", &external.endpoint, None, None);
        let error = connect(
            &worker,
            persistent_profile,
            None,
            PersistenceIntent::Persistent,
        )
        .expect_err("profile persistence unavailable");
        assert_eq!(error.kind, ErrorKind::Persistence);
    }

    #[test]
    fn delayed_connection_can_be_cancelled_immediately() {
        let external = ExternalFakeProvider::start(FakeMode::Delayed(Duration::from_secs(2)));
        let (worker, _) = started_worker();
        let candidate = profile("delayed-provider", &external.endpoint, None, None);
        worker
            .try_send(WorkerCommand::Connect {
                profile: candidate,
                secret: None,
                persistence: PersistenceIntent::SessionOnly,
            })
            .expect("connect command");
        let deadline = Instant::now() + Duration::from_secs(5);
        while external.model_requests.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(external.model_requests.load(Ordering::SeqCst), 1);
        let started = Instant::now();
        worker.try_send(WorkerCommand::Cancel).expect("cancel");
        let event = worker
            .events
            .recv_timeout(Duration::from_secs(1))
            .expect("cancelled connection");
        assert!(matches!(
            event,
            WorkerEvent::ProviderRejected { error, .. } if error.kind == ErrorKind::Cancelled
        ));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn shutdown_interrupts_delayed_connection() {
        let external = ExternalFakeProvider::start(FakeMode::Delayed(Duration::from_secs(2)));
        let (worker, _) = started_worker();
        worker
            .try_send(WorkerCommand::Connect {
                profile: profile("shutdown-provider", &external.endpoint, None, None),
                secret: None,
                persistence: PersistenceIntent::SessionOnly,
            })
            .expect("connect command");
        let deadline = Instant::now() + Duration::from_secs(5);
        while external.model_requests.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(external.model_requests.load(Ordering::SeqCst), 1);

        worker.try_send(WorkerCommand::Shutdown).expect("shutdown");

        let rejected = worker
            .events
            .recv_timeout(Duration::from_secs(1))
            .expect("connection cancellation");
        assert!(matches!(
            rejected,
            WorkerEvent::ProviderRejected { error, .. } if error.kind == ErrorKind::Cancelled
        ));
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(1)),
            Ok(WorkerEvent::Stopped)
        ));
    }

    #[test]
    fn shutdown_cancels_active_and_queued_connections() {
        let external = ExternalFakeProvider::start(FakeMode::Delayed(Duration::from_secs(2)));
        let (worker, _) = started_worker();
        for profile_id in ["first-delayed-provider", "second-delayed-provider"] {
            worker
                .try_send(WorkerCommand::Connect {
                    profile: profile(profile_id, &external.endpoint, None, None),
                    secret: None,
                    persistence: PersistenceIntent::SessionOnly,
                })
                .expect("connect command");
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while external.model_requests.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(external.model_requests.load(Ordering::SeqCst), 1);

        worker.try_send(WorkerCommand::Shutdown).expect("shutdown");

        for _ in 0..2 {
            let event = worker
                .events
                .recv_timeout(Duration::from_secs(1))
                .expect("cancelled connection");
            assert!(matches!(
                event,
                WorkerEvent::ProviderRejected { error, .. }
                    if error.kind == ErrorKind::Cancelled
            ));
        }
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(1)),
            Ok(WorkerEvent::Stopped)
        ));
        assert_eq!(external.model_requests.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn shutdown_signal_stops_worker_when_command_queue_is_full() {
        let external = ExternalFakeProvider::start(FakeMode::Delayed(Duration::from_secs(2)));
        let (worker, _) = started_worker();
        worker
            .try_send(WorkerCommand::Connect {
                profile: profile("full-queue-provider", &external.endpoint, None, None),
                secret: None,
                persistence: PersistenceIntent::SessionOnly,
            })
            .expect("connect command");
        let deadline = Instant::now() + Duration::from_secs(5);
        while external.model_requests.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(external.model_requests.load(Ordering::SeqCst), 1);

        for index in 0..COMMAND_CAPACITY {
            worker
                .commands
                .try_send(QueuedCommand::SelectModel(format!("queued-model-{index}")))
                .expect("queued command");
        }
        let active = worker
            .active_cancellation
            .lock()
            .expect("active cancellation lock");
        worker.shutdown_cancellation.cancel();
        assert!(matches!(
            worker.commands.try_send(QueuedCommand::Shutdown),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_))
        ));
        drop(active);

        let rejected = worker
            .events
            .recv_timeout(Duration::from_secs(1))
            .expect("connection cancellation");
        assert!(matches!(
            rejected,
            WorkerEvent::ProviderRejected { error, .. } if error.kind == ErrorKind::Cancelled
        ));
        for _ in 0..COMMAND_CAPACITY {
            let rejected = worker
                .events
                .recv_timeout(Duration::from_secs(1))
                .expect("queued command cancellation");
            assert!(matches!(
                rejected,
                WorkerEvent::Rejected(error) if error.kind == ErrorKind::Cancelled
            ));
        }
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(1)),
            Ok(WorkerEvent::Stopped)
        ));
    }

    #[test]
    fn failed_switch_preserves_confirmed_provider_and_model() {
        let (worker, endpoint) = started_worker();
        connect(
            &worker,
            profile("working-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("working connection");
        select(&worker, "fake-translator");

        let error = connect(
            &worker,
            profile("unavailable-provider", "http://127.0.0.1:1/v1/", None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect_err("unavailable provider");
        assert_eq!(error.kind, ErrorKind::Network);

        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
    }

    #[test]
    fn saved_selection_is_restored_only_when_still_available() {
        let (worker, endpoint) = started_worker();
        connect(
            &worker,
            profile(
                "saved-provider",
                &endpoint,
                None,
                Some("fake-slow-translator"),
            ),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("saved selection connection");
        let (_, terminal) = translate(&worker, "fake-slow-translator");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));

        connect(
            &worker,
            profile(
                "stale-saved-provider",
                &endpoint,
                None,
                Some("removed-model"),
            ),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("stale selection connection");
        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-translator",
            )))
            .expect("translation command");
        let event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("selection rejection");
        assert!(matches!(
            event,
            WorkerEvent::TranslationRejected(error) if error.kind == ErrorKind::ModelUnavailable
        ));
    }

    #[test]
    fn worker_cancellation_retains_partial_output() {
        let (worker, endpoint) = started_worker();
        connect(
            &worker,
            profile("slow-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        select(&worker, "fake-slow-translator");
        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-slow-translator",
            )))
            .expect("translation command");
        let mut output = String::new();
        let terminal = loop {
            let event = worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("translation event");
            if let WorkerEvent::Translation(event) = event {
                match event {
                    TranslationEvent::TextDelta { text, .. } => {
                        output.push_str(&text);
                        worker.try_send(WorkerCommand::Cancel).expect("cancel");
                    }
                    event if event.is_terminal() => break event,
                    _ => {}
                }
            }
        };
        assert!(!output.is_empty());
        assert!(matches!(terminal, TranslationEvent::Cancelled { .. }));
    }

    #[test]
    fn shutdown_forwards_translation_terminal_before_stopping() {
        let (worker, endpoint) = started_worker();
        connect(
            &worker,
            profile("shutdown-translation-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        select(&worker, "fake-slow-translator");
        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-slow-translator",
            )))
            .expect("translation command");
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

        worker.try_send(WorkerCommand::Shutdown).expect("shutdown");

        let terminal = worker
            .events
            .recv_timeout(Duration::from_secs(1))
            .expect("terminal event");
        assert!(matches!(
            terminal,
            WorkerEvent::Translation(TranslationEvent::Cancelled { .. })
        ));
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(1)),
            Ok(WorkerEvent::Stopped)
        ));
    }
}
