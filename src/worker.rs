use crate::file_import;
use crate::model::{ProviderProfile, ProviderProfileId};
use crate::ocr::{OcrPlugin, TesseractOcr};
use linguamesh_application::{
    HostSecretBroker, HostSecretRequests, ProviderManager, host_secret_channel,
};
use linguamesh_document::{DocumentError, DocumentJob, DocumentJobState};
use linguamesh_domain::{
    CompatibilityRequirements, CoreCompatibility, ErrorKind, Glossary, ModelDescriptor,
    SecretValue, TranslationError, TranslationEvent, TranslationPrivacyMode, TranslationRequest,
};
use linguamesh_engine::{CancellationHandle, TranslationOperation, core_compatibility};
use linguamesh_storage::{
    DocumentJobOptions, DocumentJobSnapshot, MAX_DOCUMENT_JOBS, MAX_TRANSLATION_HISTORY_ENTRIES,
    MAX_TRANSLATION_MEMORY_ENTRIES, Storage, TranslationHistoryEntry, TranslationMemoryEntry,
};
use linguamesh_testkit::FakeProviderServer;
use std::error::Error;
use std::fmt;
use std::fs::{self, DirBuilder, OpenOptions};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
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
const REQUIRED_CORE_FEATURES: [&str; 9] = [
    "cancellation_v1",
    "compatibility_negotiation_v1",
    "typed_rust_host_secret_broker_v1",
    "model_discovery_v1",
    "protected_spans_v1",
    "long_text_chunking_v1",
    "bounded_text_document_v1",
    "streaming_text_v1",
    "text_translation_v1",
];

/// 描述连接配置是否应跨进程重启保留。
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum PersistenceIntent {
    /// 配置和秘密仅在当前进程内存中存活。
    SessionOnly,
    /// 仅跨进程保留非秘密配置和模型；会话秘密仍不落盘。
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
    SelectModel {
        /// 发起选择时的活动提供商配置标识。
        profile_id: ProviderProfileId,
        /// 用户明确选择的模型标识。
        model_id: String,
    },
    /// 删除一个已保存的非秘密配置，不中断当前运行时连接。
    DeleteSavedProfile {
        /// 要删除的稳定配置标识。
        profile_id: ProviderProfileId,
    },
    /// 清空本地翻译历史。
    ClearTranslationHistory,
    /// 请求按最新顺序读取本地翻译历史。
    ListTranslationHistory,
    /// 删除指定操作标识对应的一条本地翻译历史。
    DeleteTranslationHistory {
        /// 要删除的稳定操作标识。
        operation_id: String,
    },
    /// 设置是否允许新的标准请求写入本地翻译历史。
    SetTranslationHistoryEnabled {
        /// 新的本地历史写入策略。
        enabled: bool,
    },
    /// 设置是否允许新的标准请求读写本地翻译记忆。
    SetTranslationMemoryEnabled {
        /// 新的本地翻译记忆策略。
        enabled: bool,
    },
    /// 请求按最新顺序读取本地翻译记忆。
    ListTranslationMemory,
    /// 删除指定缓存键对应的一条本地翻译记忆。
    DeleteTranslationMemory {
        /// 要删除的稳定缓存键。
        cache_key: String,
    },
    /// 清空本地翻译记忆。
    ClearTranslationMemory,
    /// 创建或替换一个本地文档任务快照。
    CreateDocumentJob {
        /// 不包含路径的稳定任务标识。
        job_id: String,
        /// 受限文档快照。
        job: DocumentJob,
    },
    /// 使用用户主动启用的可选 OCR 插件导入 image-only PDF。
    OcrDocumentJob {
        /// 不包含路径的稳定任务标识。
        job_id: String,
        /// 仅用于报告的源文件名。
        source_name: String,
        /// 受限 PDF 字节，不写入日志或诊断。
        contents: Vec<u8>,
    },
    /// 顺序翻译一个已持久化的文档任务。
    TranslateDocumentJob {
        /// 文档任务标识。
        job_id: String,
        /// 可选的源语言标签。
        source_locale: Option<String>,
        /// 目标语言标签。
        target_locale: String,
        /// 请求级词汇表。
        glossary: Option<Glossary>,
        /// 本地隐私策略；隐身模式禁止写入文档任务状态。
        privacy_mode: TranslationPrivacyMode,
    },
    /// 按最近更新时间读取本地文档任务。
    ListDocumentJobs,
    /// 从已持久化的文档任务重建二进制输出。
    ExportDocumentJob {
        /// 文档任务标识。
        job_id: String,
    },
    /// 更新文档任务中的一个可翻译段。
    UpdateDocumentSegment {
        /// 文档任务标识。
        job_id: String,
        /// 段稳定顺序号。
        index: usize,
        /// 已完成的译文。
        translated_text: String,
    },
    /// 将一个可恢复文档任务标记为继续处理。
    ResumeDocumentJob {
        /// 文档任务标识。
        job_id: String,
    },
    /// 重试失败或取消的文档任务。
    RetryDocumentJob {
        /// 文档任务标识。
        job_id: String,
    },
    /// 在当前段边界暂停文档任务。
    PauseDocumentJob {
        /// 文档任务标识。
        job_id: String,
    },
    /// 取消一个文档任务并保留其快照。
    CancelDocumentJob {
        /// 文档任务标识。
        job_id: String,
    },
    /// 开始翻译请求。
    Translate(TranslationRequest),
    /// 仅在普通文本请求中使用一个已批准的保存配置进行显式回退。
    TranslateWithFallback {
        /// 主提供商请求。
        request: TranslationRequest,
        /// 用户明确批准的保存配置标识。
        fallback_profile_id: ProviderProfileId,
    },
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
    /// 已恢复全部非秘密配置，但尚未建立网络连接。
    ProfilesRestored {
        /// 按稳定顺序返回的全部持久化配置及模型偏好。
        profiles: Vec<ProviderProfile>,
        /// 上次明确激活的持久化配置标识。
        active_profile_id: Option<ProviderProfileId>,
    },
    /// 配置数据库不可用，但会话模式仍可继续使用。
    ProfileStorageUnavailable(TranslationError),
    /// 本地历史记录数量已恢复。
    TranslationHistoryRestored {
        /// 当前数据库中的历史记录数量。
        count: usize,
    },
    /// 本地翻译历史已写入并返回最新数量。
    TranslationHistoryUpdated {
        /// 当前数据库中的历史记录数量。
        count: usize,
    },
    /// 本地翻译历史已清空。
    TranslationHistoryCleared,
    /// 已读取本地翻译历史及当前数量。
    TranslationHistoryListed {
        /// 按最新写入顺序排列的有限历史条目。
        entries: Vec<TranslationHistoryEntry>,
        /// 当前数据库中的历史记录数量。
        count: usize,
    },
    /// 已恢复本地翻译历史写入策略。
    TranslationHistoryPolicyRestored {
        /// 是否允许新的标准请求写入本地翻译历史。
        enabled: bool,
    },
    /// 本地翻译历史写入策略已更新。
    TranslationHistoryPolicyUpdated {
        /// 是否允许新的标准请求写入本地翻译历史。
        enabled: bool,
    },
    /// 本地翻译历史写入策略更新被拒绝。
    TranslationHistoryPolicyRejected(TranslationError),
    /// 读取或删除本地翻译历史被拒绝。
    TranslationHistoryActionRejected(TranslationError),
    /// 清空本地翻译历史被拒绝。
    TranslationHistoryClearRejected(TranslationError),
    /// 本地翻译历史写入失败，但翻译结果仍已完成。
    TranslationHistoryPersistenceFailed(TranslationError),
    /// 本地翻译记忆数量和策略已恢复。
    TranslationMemoryRestored {
        /// 当前数据库中的翻译记忆条目数量。
        count: usize,
        /// 是否允许读写翻译记忆。
        enabled: bool,
    },
    /// 已读取本地翻译记忆及当前数量。
    TranslationMemoryListed {
        /// 按最新写入顺序排列的有限翻译记忆条目。
        entries: Vec<TranslationMemoryEntry>,
        /// 当前数据库中的翻译记忆条目数量。
        count: usize,
    },
    /// 本地翻译记忆策略已更新。
    TranslationMemoryPolicyUpdated {
        /// 是否允许读写翻译记忆。
        enabled: bool,
    },
    /// 本地翻译记忆策略更新被拒绝。
    TranslationMemoryPolicyRejected(TranslationError),
    /// 本地翻译记忆已清空。
    TranslationMemoryCleared,
    /// 清空本地翻译记忆被拒绝。
    TranslationMemoryClearRejected(TranslationError),
    /// 读取或删除本地翻译记忆被拒绝。
    TranslationMemoryActionRejected(TranslationError),
    /// 本地翻译记忆写入失败，但翻译结果仍已完成。
    TranslationMemoryPersistenceFailed(TranslationError),
    /// 启动时恢复的可继续文档任务。
    DocumentJobsRestored {
        /// 状态为 pending 或 running 的任务快照。
        jobs: Vec<DocumentJobSnapshot>,
    },
    /// 已读取本地文档任务。
    DocumentJobsListed {
        /// 按最近更新时间排列的任务快照。
        jobs: Vec<DocumentJobSnapshot>,
    },
    /// 文档任务快照已写入。
    DocumentJobUpdated(DocumentJobSnapshot),
    /// 文档任务已重建为可写入文件的字节。
    DocumentJobExported {
        /// 原始文档文件名，仅用于生成默认扩展名。
        source_name: String,
        /// 已通过 Core 结构校验的输出字节。
        contents: Vec<u8>,
    },
    /// 文档任务某一段产生了翻译事件。
    DocumentJobSegment {
        /// 文档任务标识。
        job_id: String,
        /// 当前段的稳定顺序号。
        index: usize,
        /// 当前段的核心翻译事件。
        event: TranslationEvent,
    },
    /// 文档任务存储在启动或操作期间不可用。
    DocumentJobStorageUnavailable(TranslationError),
    /// 文档任务命令被精确拒绝。
    DocumentJobActionRejected(TranslationError),
    /// 提供商已连接并返回模型。
    Connected {
        /// 已由核心成功连接的规范配置。
        profile: ProviderProfile,
        /// 核心发现的模型。
        models: Vec<ModelDescriptor>,
        /// 本次原子保存成功的非秘密配置。
        saved_profile: Option<ProviderProfile>,
    },
    /// 工作线程已确认用户选择的模型。
    ModelSelected {
        /// 当前活动提供商配置标识。
        profile_id: ProviderProfileId,
        /// 已确认的模型标识。
        model_id: String,
        /// 模型偏好更新后的非秘密持久化配置。
        saved_profile: Option<ProviderProfile>,
    },
    /// 模型选择被精确拒绝且活动选择保持不变。
    ModelSelectionRejected {
        /// 发起选择时的活动提供商配置标识。
        profile_id: ProviderProfileId,
        /// 被拒绝的模型标识。
        model_id: String,
        /// 不包含内容或秘密的类型化错误。
        error: TranslationError,
    },
    /// 已删除一个持久化配置，现有运行时连接保持可用。
    ProfileDeleted {
        /// 已删除的稳定配置标识。
        profile_id: ProviderProfileId,
    },
    /// 配置已删除但 Secret Service 项目清理失败。
    #[cfg(feature = "gui")]
    SecretCleanupFailed {
        /// 已删除配置的稳定标识。
        profile_id: ProviderProfileId,
        /// 不包含秘密值的清理错误。
        error: TranslationError,
    },
    /// 配置删除被精确拒绝且保存状态保持不变。
    ProfileDeletionRejected {
        /// 被拒绝删除的稳定配置标识。
        profile_id: ProviderProfileId,
        /// 不包含内容或秘密的类型化错误。
        error: TranslationError,
    },
    /// 共享核心产生翻译事件。
    Translation(TranslationEvent),
    /// 主提供商发生可重试失败后已选择获准回退配置。
    FallbackSelected {
        /// 发生失败的主配置标识。
        primary_profile_id: ProviderProfileId,
        /// 接收后续请求的获准回退配置标识。
        fallback_profile_id: ProviderProfileId,
    },
    /// 核心事件流在没有终止事件时异常结束。
    OperationFailed(TranslationError),
    /// 翻译命令在创建核心操作之前被拒绝。
    TranslationRejected(TranslationError),
    /// 候选提供商连接被拒绝且现有会话不受影响。
    ProviderRejected {
        /// 被拒绝的完整非秘密候选配置。
        profile: ProviderProfile,
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

/// 可克隆的非阻塞命令句柄，供历史窗口等短生命周期界面使用。
#[derive(Clone)]
pub struct WorkerCommandHandle {
    commands: CommandSender<QueuedCommand>,
}

impl WorkerCommandHandle {
    /// 非阻塞请求读取本地文档任务队列。
    pub fn list_document_jobs(&self) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::ListDocumentJobs)
            .map_err(|_| WorkerSendError)
    }

    /// 非阻塞请求暂停指定的本地文档任务。
    pub fn pause_document_job(&self, job_id: String) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::PauseDocumentJob { job_id })
            .map_err(|_| WorkerSendError)
    }

    /// 非阻塞请求恢复指定的本地文档任务。
    pub fn resume_document_job(&self, job_id: String) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::ResumeDocumentJob { job_id })
            .map_err(|_| WorkerSendError)
    }

    /// 非阻塞请求重试指定的本地文档任务。
    pub fn retry_document_job(&self, job_id: String) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::RetryDocumentJob { job_id })
            .map_err(|_| WorkerSendError)
    }

    /// 非阻塞请求重建本地文档任务输出。
    pub fn export_document_job(&self, job_id: String) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::ExportDocumentJob { job_id })
            .map_err(|_| WorkerSendError)
    }

    /// 非阻塞请求读取本地翻译历史。
    pub fn list_translation_history(&self) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::ListTranslationHistory)
            .map_err(|_| WorkerSendError)
    }

    /// 非阻塞请求删除一条本地翻译历史。
    pub fn delete_translation_history(&self, operation_id: String) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::DeleteTranslationHistory { operation_id })
            .map_err(|_| WorkerSendError)
    }

    /// 非阻塞请求更新本地翻译历史写入策略。
    pub fn set_translation_history_enabled(&self, enabled: bool) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::SetTranslationHistoryEnabled { enabled })
            .map_err(|_| WorkerSendError)
    }

    /// 非阻塞请求更新本地翻译记忆策略。
    pub fn set_translation_memory_enabled(&self, enabled: bool) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::SetTranslationMemoryEnabled { enabled })
            .map_err(|_| WorkerSendError)
    }

    /// 非阻塞请求读取本地翻译记忆。
    pub fn list_translation_memory(&self) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::ListTranslationMemory)
            .map_err(|_| WorkerSendError)
    }

    /// 非阻塞请求删除一条本地翻译记忆。
    pub fn delete_translation_memory(&self, cache_key: String) -> Result<(), WorkerSendError> {
        self.commands
            .try_send(QueuedCommand::DeleteTranslationMemory { cache_key })
            .map_err(|_| WorkerSendError)
    }
}

enum QueuedCommand {
    Connect {
        profile: ProviderProfile,
        secret: Option<SecretValue>,
        persistence: PersistenceIntent,
        cancellation: CancellationToken,
    },
    SelectModel {
        profile_id: ProviderProfileId,
        model_id: String,
    },
    DeleteSavedProfile {
        profile_id: ProviderProfileId,
    },
    ClearTranslationHistory,
    ListTranslationHistory,
    DeleteTranslationHistory {
        operation_id: String,
    },
    SetTranslationHistoryEnabled {
        enabled: bool,
    },
    SetTranslationMemoryEnabled {
        enabled: bool,
    },
    ListTranslationMemory,
    DeleteTranslationMemory {
        cache_key: String,
    },
    ClearTranslationMemory,
    CreateDocumentJob {
        job_id: String,
        job: DocumentJob,
    },
    OcrDocumentJob {
        job_id: String,
        source_name: String,
        contents: Vec<u8>,
    },
    TranslateDocumentJob {
        job_id: String,
        source_locale: Option<String>,
        target_locale: String,
        glossary: Option<Glossary>,
        privacy_mode: TranslationPrivacyMode,
    },
    ListDocumentJobs,
    ExportDocumentJob {
        job_id: String,
    },
    UpdateDocumentSegment {
        job_id: String,
        index: usize,
        translated_text: String,
    },
    ResumeDocumentJob {
        job_id: String,
    },
    RetryDocumentJob {
        job_id: String,
    },
    PauseDocumentJob {
        job_id: String,
    },
    CancelDocumentJob {
        job_id: String,
    },
    Translate(TranslationRequest),
    TranslateWithFallback {
        request: TranslationRequest,
        fallback_profile_id: ProviderProfileId,
    },
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
        Self::spawn_inner(None)
    }

    /// 启动使用指定本地配置数据库的独立运行时。
    #[must_use]
    pub fn spawn_with_database(path: impl Into<PathBuf>) -> Self {
        Self::spawn_inner(Some(path.into()))
    }

    fn spawn_inner(database_path: Option<PathBuf>) -> Self {
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
                    database_path,
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
    #[allow(clippy::too_many_lines)]
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
            WorkerCommand::SelectModel {
                profile_id,
                model_id,
            } => self
                .commands
                .try_send(QueuedCommand::SelectModel {
                    profile_id,
                    model_id,
                })
                .map_err(|_| WorkerSendError),
            WorkerCommand::DeleteSavedProfile { profile_id } => self
                .commands
                .try_send(QueuedCommand::DeleteSavedProfile { profile_id })
                .map_err(|_| WorkerSendError),
            WorkerCommand::ClearTranslationHistory => self
                .commands
                .try_send(QueuedCommand::ClearTranslationHistory)
                .map_err(|_| WorkerSendError),
            WorkerCommand::ListTranslationHistory => self
                .commands
                .try_send(QueuedCommand::ListTranslationHistory)
                .map_err(|_| WorkerSendError),
            WorkerCommand::DeleteTranslationHistory { operation_id } => self
                .commands
                .try_send(QueuedCommand::DeleteTranslationHistory { operation_id })
                .map_err(|_| WorkerSendError),
            WorkerCommand::SetTranslationHistoryEnabled { enabled } => self
                .commands
                .try_send(QueuedCommand::SetTranslationHistoryEnabled { enabled })
                .map_err(|_| WorkerSendError),
            WorkerCommand::SetTranslationMemoryEnabled { enabled } => self
                .commands
                .try_send(QueuedCommand::SetTranslationMemoryEnabled { enabled })
                .map_err(|_| WorkerSendError),
            WorkerCommand::ListTranslationMemory => self
                .commands
                .try_send(QueuedCommand::ListTranslationMemory)
                .map_err(|_| WorkerSendError),
            WorkerCommand::DeleteTranslationMemory { cache_key } => self
                .commands
                .try_send(QueuedCommand::DeleteTranslationMemory { cache_key })
                .map_err(|_| WorkerSendError),
            WorkerCommand::ClearTranslationMemory => self
                .commands
                .try_send(QueuedCommand::ClearTranslationMemory)
                .map_err(|_| WorkerSendError),
            WorkerCommand::CreateDocumentJob { job_id, job } => self
                .commands
                .try_send(QueuedCommand::CreateDocumentJob { job_id, job })
                .map_err(|_| WorkerSendError),
            WorkerCommand::OcrDocumentJob {
                job_id,
                source_name,
                contents,
            } => self
                .commands
                .try_send(QueuedCommand::OcrDocumentJob {
                    job_id,
                    source_name,
                    contents,
                })
                .map_err(|_| WorkerSendError),
            WorkerCommand::TranslateDocumentJob {
                job_id,
                source_locale,
                target_locale,
                glossary,
                privacy_mode,
            } => self
                .commands
                .try_send(QueuedCommand::TranslateDocumentJob {
                    job_id,
                    source_locale,
                    target_locale,
                    glossary,
                    privacy_mode,
                })
                .map_err(|_| WorkerSendError),
            WorkerCommand::ListDocumentJobs => self
                .commands
                .try_send(QueuedCommand::ListDocumentJobs)
                .map_err(|_| WorkerSendError),
            WorkerCommand::ExportDocumentJob { job_id } => self
                .commands
                .try_send(QueuedCommand::ExportDocumentJob { job_id })
                .map_err(|_| WorkerSendError),
            WorkerCommand::UpdateDocumentSegment {
                job_id,
                index,
                translated_text,
            } => self
                .commands
                .try_send(QueuedCommand::UpdateDocumentSegment {
                    job_id,
                    index,
                    translated_text,
                })
                .map_err(|_| WorkerSendError),
            WorkerCommand::ResumeDocumentJob { job_id } => self
                .commands
                .try_send(QueuedCommand::ResumeDocumentJob { job_id })
                .map_err(|_| WorkerSendError),
            WorkerCommand::RetryDocumentJob { job_id } => self
                .commands
                .try_send(QueuedCommand::RetryDocumentJob { job_id })
                .map_err(|_| WorkerSendError),
            WorkerCommand::PauseDocumentJob { job_id } => self
                .commands
                .try_send(QueuedCommand::PauseDocumentJob { job_id })
                .map_err(|_| WorkerSendError),
            WorkerCommand::CancelDocumentJob { job_id } => self
                .commands
                .try_send(QueuedCommand::CancelDocumentJob { job_id })
                .map_err(|_| WorkerSendError),
            WorkerCommand::Translate(request) => self
                .commands
                .try_send(QueuedCommand::Translate(request))
                .map_err(|_| WorkerSendError),
            WorkerCommand::TranslateWithFallback {
                request,
                fallback_profile_id,
            } => self
                .commands
                .try_send(QueuedCommand::TranslateWithFallback {
                    request,
                    fallback_profile_id,
                })
                .map_err(|_| WorkerSendError),
            WorkerCommand::Shutdown => {
                let result = self.commands.try_send(QueuedCommand::Shutdown);
                self.shutdown_cancellation.cancel();
                cancel_active(&self.active_cancellation);
                result.map_err(|_| WorkerSendError)
            }
        }
    }

    /// 创建一个只用于提交命令的可克隆句柄。
    #[must_use]
    pub fn command_handle(&self) -> WorkerCommandHandle {
        WorkerCommandHandle {
            commands: self.commands.clone(),
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

struct ConnectedCandidate {
    runtime_profile: ProviderProfile,
    saved_profile: Option<ProviderProfile>,
    models: Vec<ModelDescriptor>,
    selected_model: Option<String>,
}

struct ActiveTranslation {
    operation: TranslationOperation,
    request: TranslationRequest,
    output: String,
    fallback_profile_id: Option<ProviderProfileId>,
    fallback_model: Option<String>,
    fallback_attempted: bool,
    suppress_fallback_started: bool,
    next_sequence: u64,
}

struct ActiveDocumentTranslation {
    job_id: String,
    segment_index: usize,
    source_locale: Option<String>,
    target_locale: String,
    glossary: Option<Glossary>,
    privacy_mode: TranslationPrivacyMode,
    operation: TranslationOperation,
    output: String,
    cancel_requested: bool,
    pause_requested: bool,
    stop_after_active: bool,
}

struct DocumentTranslationOptions {
    source_locale: Option<String>,
    target_locale: String,
    glossary: Option<Glossary>,
    privacy_mode: TranslationPrivacyMode,
}

// 集中保留命令与事件优先级，避免拆分后破坏单一活动操作约束。
#[allow(clippy::too_many_lines)]
async fn run_worker(
    mut commands: CommandReceiver<QueuedCommand>,
    events: SyncSender<WorkerEvent>,
    active_cancellation: Arc<Mutex<Option<ActiveCancellation>>>,
    shutdown_cancellation: CancellationToken,
    database_path: Option<PathBuf>,
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
    let mut storage = match database_path {
        Some(path) => match open_profile_storage(&path) {
            Ok((storage, profiles, active_profile_id)) => {
                if events
                    .send(WorkerEvent::ProfilesRestored {
                        profiles,
                        active_profile_id,
                    })
                    .is_err()
                {
                    return;
                }
                let history_count = match storage.translation_history_count() {
                    Ok(count) => count,
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationHistoryPersistenceFailed(error))
                            .is_err()
                        {
                            return;
                        }
                        0
                    }
                };
                if events
                    .send(WorkerEvent::TranslationHistoryRestored {
                        count: history_count,
                    })
                    .is_err()
                {
                    return;
                }
                let history_enabled = match storage.translation_history_enabled() {
                    Ok(enabled) => enabled,
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationHistoryPersistenceFailed(error))
                            .is_err()
                        {
                            return;
                        }
                        true
                    }
                };
                if events
                    .send(WorkerEvent::TranslationHistoryPolicyRestored {
                        enabled: history_enabled,
                    })
                    .is_err()
                {
                    return;
                }
                let memory_count = match storage.translation_memory_count() {
                    Ok(count) => count,
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationMemoryPersistenceFailed(error))
                            .is_err()
                        {
                            return;
                        }
                        0
                    }
                };
                let memory_enabled = match storage.translation_memory_enabled() {
                    Ok(enabled) => enabled,
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationMemoryPersistenceFailed(error))
                            .is_err()
                        {
                            return;
                        }
                        true
                    }
                };
                if events
                    .send(WorkerEvent::TranslationMemoryRestored {
                        count: memory_count,
                        enabled: memory_enabled,
                    })
                    .is_err()
                {
                    return;
                }
                match storage.resumable_document_jobs() {
                    Ok(jobs) => {
                        if events
                            .send(WorkerEvent::DocumentJobsRestored { jobs })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobStorageUnavailable(error))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
                Some(storage)
            }
            Err(error) => {
                if events
                    .send(WorkerEvent::ProfileStorageUnavailable(error))
                    .is_err()
                {
                    return;
                }
                None
            }
        },
        None => None,
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
    let mut fallback_manager: Option<ProviderManager> = None;
    let mut fallback_profile: Option<ProviderProfile> = None;
    let mut active_profile: Option<ProviderProfile> = None;
    let mut active_saved_profile: Option<ProviderProfile> = None;
    let mut selected_model: Option<String> = None;
    let mut active: Option<ActiveTranslation> = None;
    let mut active_document: Option<ActiveDocumentTranslation> = None;
    let mut shutting_down = false;
    let mut stop_after_active = false;
    while !shutting_down {
        if let Some(active_translation) = active.as_mut() {
            let operation = &mut active_translation.operation;
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
                        profile,
                        error: TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "A provider cannot be changed while a translation is running.",
                        ),
                    });
                }
                ActiveStep::Command(Some(QueuedCommand::SelectModel {
                    profile_id,
                    model_id,
                })) => {
                    let _ = events.send(WorkerEvent::ModelSelectionRejected {
                        profile_id,
                        model_id,
                        error: TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "A model cannot be changed while a translation is running.",
                        ),
                    });
                }
                ActiveStep::Command(Some(QueuedCommand::DeleteSavedProfile { profile_id })) => {
                    let _ = events.send(WorkerEvent::ProfileDeletionRejected {
                        profile_id,
                        error: TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "A saved profile cannot be removed while a translation is running.",
                        ),
                    });
                }
                ActiveStep::Command(Some(QueuedCommand::ClearTranslationHistory)) => {
                    let _ = events.send(WorkerEvent::TranslationHistoryClearRejected(
                        TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "Translation history cannot be cleared while a translation is running.",
                        ),
                    ));
                }
                ActiveStep::Command(Some(
                    QueuedCommand::ListTranslationHistory
                    | QueuedCommand::DeleteTranslationHistory { .. },
                )) => {
                    let _ = events.send(WorkerEvent::TranslationHistoryActionRejected(
                        TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "Translation history cannot be changed while a translation is running.",
                        ),
                    ));
                }
                ActiveStep::Command(Some(QueuedCommand::SetTranslationHistoryEnabled {
                    enabled: _,
                })) => {
                    let _ = events.send(WorkerEvent::TranslationHistoryPolicyRejected(
                        TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "Translation history policy cannot change while a translation is running.",
                        ),
                    ));
                }
                ActiveStep::Command(Some(QueuedCommand::SetTranslationMemoryEnabled {
                    enabled: _,
                })) => {
                    let _ = events.send(WorkerEvent::TranslationMemoryPolicyRejected(
                        TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "Translation memory policy cannot change while a translation is running.",
                        ),
                    ));
                }
                ActiveStep::Command(Some(QueuedCommand::ClearTranslationMemory)) => {
                    let _ = events.send(WorkerEvent::TranslationMemoryClearRejected(
                        TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "Translation memory cannot be cleared while a translation is running.",
                        ),
                    ));
                }
                ActiveStep::Command(Some(
                    QueuedCommand::ListTranslationMemory
                    | QueuedCommand::DeleteTranslationMemory { .. },
                )) => {
                    let _ = events.send(WorkerEvent::TranslationMemoryActionRejected(
                        TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "Translation memory cannot be changed while a translation is running.",
                        ),
                    ));
                }
                ActiveStep::Command(Some(
                    QueuedCommand::CreateDocumentJob { .. }
                    | QueuedCommand::OcrDocumentJob { .. }
                    | QueuedCommand::TranslateDocumentJob { .. }
                    | QueuedCommand::ListDocumentJobs
                    | QueuedCommand::ExportDocumentJob { .. }
                    | QueuedCommand::UpdateDocumentSegment { .. }
                    | QueuedCommand::ResumeDocumentJob { .. }
                    | QueuedCommand::RetryDocumentJob { .. }
                    | QueuedCommand::PauseDocumentJob { .. }
                    | QueuedCommand::CancelDocumentJob { .. },
                )) => {
                    let _ = events.send(WorkerEvent::DocumentJobActionRejected(
                        TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "A document job cannot change while a translation is running.",
                        ),
                    ));
                }
                ActiveStep::Command(Some(
                    QueuedCommand::Translate(_) | QueuedCommand::TranslateWithFallback { .. },
                )) => {
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
                    if let TranslationEvent::Failed { sequence, error } = &event
                        && !active_translation.fallback_attempted
                        && active_translation
                            .fallback_profile_id
                            .as_ref()
                            .is_some_and(|_| is_retryable_fallback(error))
                        && let Some(fallback_id) = active_translation.fallback_profile_id.clone()
                    {
                        let primary_id =
                            active_profile.as_ref().map(|profile| profile.id().clone());
                        let next_translation = if let (Some(profile), Some(manager)) =
                            (fallback_profile.as_ref(), fallback_manager.as_ref())
                        {
                            let fallback_request = active_translation
                                .request
                                .clone()
                                .with_provider_identity(format!(
                                    "{}@{}",
                                    fallback_id.as_str(),
                                    profile.base_endpoint()
                                ));
                            begin_translation(
                                manager,
                                active_translation.fallback_model.as_deref(),
                                fallback_request,
                            )
                            .ok()
                        } else {
                            None
                        };
                        if let Some(next_translation) = next_translation {
                            active_translation.operation = next_translation.operation;
                            active_translation.request = next_translation.request;
                            active_translation.fallback_attempted = true;
                            active_translation.suppress_fallback_started = true;
                            active_translation.next_sequence = *sequence;
                            set_active_cancellation(
                                &active_cancellation,
                                ActiveCancellation::Translation(
                                    active_translation.operation.cancellation_handle(),
                                ),
                            );
                            if let Some(primary_id) = primary_id
                                && events
                                    .send(WorkerEvent::FallbackSelected {
                                        primary_profile_id: primary_id,
                                        fallback_profile_id: fallback_id,
                                    })
                                    .is_err()
                            {
                                shutting_down = true;
                            }
                            continue;
                        }
                        active_translation.fallback_attempted = true;
                        active_translation.fallback_profile_id = None;
                        fallback_manager = None;
                        fallback_profile = None;
                    }
                    if active_translation.suppress_fallback_started
                        && matches!(&event, TranslationEvent::Started { .. })
                    {
                        active_translation.suppress_fallback_started = false;
                        continue;
                    }
                    let event = if active_translation.fallback_attempted {
                        let sequence = active_translation.next_sequence;
                        active_translation.next_sequence = sequence.saturating_add(1);
                        remap_translation_event(event, sequence)
                    } else {
                        active_translation.next_sequence = event.sequence().saturating_add(1);
                        event
                    };
                    if let TranslationEvent::TextDelta { text, .. } = &event {
                        active_translation.output.push_str(text);
                    }
                    let terminal = event.is_terminal();
                    let completed = matches!(&event, TranslationEvent::Completed { .. });
                    if terminal
                        && completed
                        && !active_translation.request.is_incognito()
                        && let Some(storage) = storage.as_mut()
                    {
                        let history_event = match storage.translation_history_enabled() {
                            Ok(false) => None,
                            Ok(true) => Some(
                                match storage.record_translation_history(
                                    &active_translation.request,
                                    &active_translation.output,
                                ) {
                                    Ok(()) => match storage.translation_history_count() {
                                        Ok(count) => {
                                            WorkerEvent::TranslationHistoryUpdated { count }
                                        }
                                        Err(error) => {
                                            WorkerEvent::TranslationHistoryPersistenceFailed(error)
                                        }
                                    },
                                    Err(error) => {
                                        WorkerEvent::TranslationHistoryPersistenceFailed(error)
                                    }
                                },
                            ),
                            Err(error) => {
                                Some(WorkerEvent::TranslationHistoryPersistenceFailed(error))
                            }
                        };
                        if let Some(history_event) = history_event
                            && events.send(history_event).is_err()
                        {
                            shutting_down = true;
                        }
                        if !shutting_down {
                            let memory_event = match storage.translation_memory_enabled() {
                                Ok(false) => None,
                                Ok(true) => match storage.record_translation_memory(
                                    &active_translation.request,
                                    &active_translation.output,
                                ) {
                                    Ok(()) => None,
                                    Err(error) => {
                                        Some(WorkerEvent::TranslationMemoryPersistenceFailed(error))
                                    }
                                },
                                Err(error) => {
                                    Some(WorkerEvent::TranslationMemoryPersistenceFailed(error))
                                }
                            };
                            if let Some(memory_event) = memory_event
                                && events.send(memory_event).is_err()
                            {
                                shutting_down = true;
                            }
                        }
                    }
                    if events.send(WorkerEvent::Translation(event)).is_err() {
                        shutting_down = true;
                    }
                    if terminal {
                        clear_active_cancellation(&active_cancellation);
                        active = None;
                        fallback_manager = None;
                        fallback_profile = None;
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

        if let Some(mut document_translation) = active_document.take() {
            let step = tokio::select! {
                biased;
                () = shutdown_cancellation.cancelled(), if !document_translation.stop_after_active => ActiveStep::Shutdown,
                command = commands.recv(), if !document_translation.stop_after_active => ActiveStep::Command(command),
                event = document_translation.operation.next_event() => ActiveStep::Event(event),
            };
            match step {
                ActiveStep::Command(Some(QueuedCommand::Cancel)) => {
                    document_translation.cancel_requested = true;
                    document_translation.operation.cancel();
                    active_document = Some(document_translation);
                }
                ActiveStep::Command(Some(QueuedCommand::CancelDocumentJob { job_id }))
                    if job_id == document_translation.job_id =>
                {
                    document_translation.cancel_requested = true;
                    document_translation.operation.cancel();
                    active_document = Some(document_translation);
                }
                ActiveStep::Command(Some(QueuedCommand::PauseDocumentJob { job_id }))
                    if job_id == document_translation.job_id =>
                {
                    document_translation.pause_requested = true;
                    document_translation.operation.cancel();
                    active_document = Some(document_translation);
                }
                ActiveStep::Command(Some(
                    QueuedCommand::PauseDocumentJob { .. }
                    | QueuedCommand::CancelDocumentJob { .. },
                )) => {
                    let _ = events.send(WorkerEvent::DocumentJobActionRejected(
                        TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "A different document job is already translating.",
                        ),
                    ));
                    active_document = Some(document_translation);
                }
                ActiveStep::Shutdown
                | ActiveStep::Command(Some(QueuedCommand::Shutdown) | None) => {
                    document_translation.stop_after_active = true;
                    document_translation.operation.cancel();
                    active_document = Some(document_translation);
                }
                ActiveStep::Command(Some(_)) => {
                    let _ = events.send(WorkerEvent::DocumentJobActionRejected(
                        TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "A document job cannot change while another document job is translating.",
                        ),
                    ));
                    active_document = Some(document_translation);
                }
                ActiveStep::Event(Some(event)) => {
                    if let TranslationEvent::TextDelta { text, .. } = &event {
                        document_translation.output.push_str(text);
                    }
                    let terminal = event.is_terminal();
                    let completed = matches!(&event, TranslationEvent::Completed { .. });
                    if events
                        .send(WorkerEvent::DocumentJobSegment {
                            job_id: document_translation.job_id.clone(),
                            index: document_translation.segment_index,
                            event,
                        })
                        .is_err()
                    {
                        shutting_down = true;
                    }
                    if terminal && !shutting_down {
                        clear_active_cancellation(&active_cancellation);
                        if completed {
                            let update = storage
                                .as_mut()
                                .ok_or_else(|| {
                                    TranslationError::new(
                                        ErrorKind::Persistence,
                                        "Local document job storage is unavailable.",
                                    )
                                })
                                .and_then(|storage| {
                                    storage.update_document_segment(
                                        &document_translation.job_id,
                                        document_translation.segment_index,
                                        &document_translation.output,
                                    )
                                });
                            match update {
                                Ok(snapshot) => {
                                    let snapshot = if document_translation.pause_requested
                                        && snapshot.state != DocumentJobState::Completed
                                    {
                                        storage
                                            .as_mut()
                                            .and_then(|storage| {
                                                storage
                                                    .set_document_job_state(
                                                        &document_translation.job_id,
                                                        DocumentJobState::Paused,
                                                    )
                                                    .ok()
                                            })
                                            .unwrap_or(snapshot)
                                    } else {
                                        snapshot
                                    };
                                    if events
                                        .send(WorkerEvent::DocumentJobUpdated(snapshot.clone()))
                                        .is_err()
                                    {
                                        shutting_down = true;
                                    } else if snapshot.state == DocumentJobState::Completed
                                        || document_translation.stop_after_active
                                        || document_translation.pause_requested
                                    {
                                        shutting_down |= document_translation.stop_after_active;
                                    } else {
                                        match begin_document_segment(
                                            &manager,
                                            active_profile.as_ref(),
                                            selected_model.as_deref(),
                                            &document_translation.job_id,
                                            &snapshot.job,
                                            DocumentTranslationOptions {
                                                source_locale: document_translation
                                                    .source_locale
                                                    .clone(),
                                                target_locale: document_translation
                                                    .target_locale
                                                    .clone(),
                                                glossary: document_translation.glossary.clone(),
                                                privacy_mode: document_translation.privacy_mode,
                                            },
                                        ) {
                                            Ok(next) => {
                                                set_active_cancellation(
                                                    &active_cancellation,
                                                    ActiveCancellation::Translation(
                                                        next.operation.cancellation_handle(),
                                                    ),
                                                );
                                                active_document = Some(next);
                                            }
                                            Err(error) => {
                                                let _ = events.send(
                                                    WorkerEvent::DocumentJobActionRejected(error),
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(error) => {
                                    let _ =
                                        events.send(WorkerEvent::DocumentJobActionRejected(error));
                                }
                            }
                        } else if !document_translation.stop_after_active {
                            let state = if document_translation.pause_requested {
                                DocumentJobState::Paused
                            } else if document_translation.cancel_requested {
                                DocumentJobState::Cancelled
                            } else {
                                DocumentJobState::Failed
                            };
                            match storage
                                .as_mut()
                                .ok_or_else(|| {
                                    TranslationError::new(
                                        ErrorKind::Persistence,
                                        "Local document job storage is unavailable.",
                                    )
                                })
                                .and_then(|storage| {
                                    storage
                                        .set_document_job_state(&document_translation.job_id, state)
                                }) {
                                Ok(snapshot) => {
                                    let _ = events.send(WorkerEvent::DocumentJobUpdated(snapshot));
                                }
                                Err(error) => {
                                    let _ =
                                        events.send(WorkerEvent::DocumentJobActionRejected(error));
                                }
                            }
                        }
                        if document_translation.stop_after_active {
                            shutting_down = true;
                        }
                    } else if !terminal && !shutting_down {
                        active_document = Some(document_translation);
                    }
                }
                ActiveStep::Event(None) => {
                    clear_active_cancellation(&active_cancellation);
                    if !document_translation.stop_after_active {
                        let _ = events.send(WorkerEvent::DocumentJobActionRejected(
                            TranslationError::new(
                                ErrorKind::Internal,
                                "The document translation event stream ended without a terminal event.",
                            ),
                        ));
                        if let Some(storage) = storage.as_mut() {
                            let _ = storage.set_document_job_state(
                                &document_translation.job_id,
                                DocumentJobState::Failed,
                            );
                        }
                    }
                    shutting_down |= document_translation.stop_after_active;
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
                let mut candidate = ProviderManager::new(secret_broker.clone());
                let (result, storage_write_attempted) =
                    match validate_persistence_request(&profile, persistence, storage.is_some()) {
                        Ok(()) => match connect_candidate(
                            &mut candidate,
                            &profile,
                            secret,
                            &cancellation,
                            &mut secret_requests,
                        )
                        .await
                        {
                            Ok(models) => (
                                finish_candidate_connection(
                                    &profile,
                                    models,
                                    persistence,
                                    storage.as_mut(),
                                    &cancellation,
                                ),
                                persistence == PersistenceIntent::Persistent,
                            ),
                            Err(error) => (Err(error), false),
                        },
                        Err(error) => (Err(error), false),
                    };
                clear_active_cancellation(&active_cancellation);
                match result {
                    Ok(connected) => {
                        selected_model.clone_from(&connected.selected_model);
                        active_profile = Some(connected.runtime_profile.clone());
                        active_saved_profile.clone_from(&connected.saved_profile);
                        manager = candidate;
                        if events
                            .send(WorkerEvent::Connected {
                                profile: connected.runtime_profile,
                                models: connected.models,
                                saved_profile: connected.saved_profile,
                            })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        let storage_error = if storage_write_attempted {
                            degrade_profile_storage_after_persistence_error(
                                &error,
                                &mut storage,
                                &mut active_saved_profile,
                            )
                        } else {
                            None
                        };
                        if events
                            .send(WorkerEvent::ProviderRejected { profile, error })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                        if let Some(error) = storage_error
                            && events
                                .send(WorkerEvent::ProfileStorageUnavailable(error))
                                .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::SelectModel {
                profile_id,
                model_id,
            }) => {
                let result = prepare_model_selection(
                    &manager,
                    active_profile.as_ref(),
                    active_saved_profile.as_ref(),
                    storage.as_mut(),
                    &profile_id,
                    &model_id,
                );
                match result {
                    Ok((updated_profile, updated_saved_profile)) => {
                        selected_model = Some(model_id.clone());
                        active_profile = Some(updated_profile);
                        active_saved_profile.clone_from(&updated_saved_profile);
                        if events
                            .send(WorkerEvent::ModelSelected {
                                profile_id,
                                model_id,
                                saved_profile: updated_saved_profile,
                            })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        let storage_error = degrade_profile_storage_after_persistence_error(
                            &error,
                            &mut storage,
                            &mut active_saved_profile,
                        );
                        if events
                            .send(WorkerEvent::ModelSelectionRejected {
                                profile_id,
                                model_id,
                                error,
                            })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                        if let Some(error) = storage_error
                            && events
                                .send(WorkerEvent::ProfileStorageUnavailable(error))
                                .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::DeleteSavedProfile { profile_id }) => {
                let result = delete_saved_profile(storage.as_mut(), &profile_id);
                match result {
                    Ok(secret_ref) => {
                        if active_saved_profile
                            .as_ref()
                            .is_some_and(|profile| profile.id() == &profile_id)
                        {
                            active_saved_profile = None;
                        }
                        #[cfg(feature = "gui")]
                        let deleted_profile_id = profile_id.clone();
                        if events
                            .send(WorkerEvent::ProfileDeleted { profile_id })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                        #[cfg(feature = "gui")]
                        if let Some(secret_ref) = secret_ref {
                            let cleanup = tokio::task::spawn_blocking(move || {
                                crate::secret_service::delete_secret(&secret_ref)
                            })
                            .await
                            .unwrap_or_else(|_| {
                                Err(TranslationError::new(
                                    ErrorKind::SecureStorageUnavailable,
                                    "Secret Service cleanup failed after profile removal.",
                                ))
                            });
                            if let Err(error) = cleanup
                                && events
                                    .send(WorkerEvent::SecretCleanupFailed {
                                        profile_id: deleted_profile_id,
                                        error,
                                    })
                                    .is_err()
                            {
                                shutting_down = true;
                            }
                        }
                        #[cfg(not(feature = "gui"))]
                        let _ = secret_ref;
                    }
                    Err(error) => {
                        let storage_error = degrade_profile_storage_after_persistence_error(
                            &error,
                            &mut storage,
                            &mut active_saved_profile,
                        );
                        if events
                            .send(WorkerEvent::ProfileDeletionRejected { profile_id, error })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                        if let Some(error) = storage_error
                            && events
                                .send(WorkerEvent::ProfileStorageUnavailable(error))
                                .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::SetTranslationHistoryEnabled { enabled }) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local translation history storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        storage.set_translation_history_enabled(enabled)?;
                        Ok(())
                    });
                match result {
                    Ok(()) => {
                        if events
                            .send(WorkerEvent::TranslationHistoryPolicyUpdated { enabled })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationHistoryPolicyRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::ClearTranslationHistory) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local translation history storage is unavailable.",
                        )
                    })
                    .and_then(Storage::clear_translation_history);
                match result {
                    Ok(()) => {
                        if events.send(WorkerEvent::TranslationHistoryCleared).is_err() {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationHistoryClearRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::ListTranslationHistory) => {
                let result = storage
                    .as_ref()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local translation history storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        let entries =
                            storage.translation_history(MAX_TRANSLATION_HISTORY_ENTRIES)?;
                        let count = storage.translation_history_count()?;
                        Ok((entries, count))
                    });
                match result {
                    Ok((entries, count)) => {
                        if events
                            .send(WorkerEvent::TranslationHistoryListed { entries, count })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationHistoryActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::DeleteTranslationHistory { operation_id }) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local translation history storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        storage.delete_translation_history_entry(operation_id.as_str())?;
                        let entries =
                            storage.translation_history(MAX_TRANSLATION_HISTORY_ENTRIES)?;
                        let count = storage.translation_history_count()?;
                        Ok((entries, count))
                    });
                match result {
                    Ok((entries, count)) => {
                        if events
                            .send(WorkerEvent::TranslationHistoryListed { entries, count })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationHistoryActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::SetTranslationMemoryEnabled { enabled }) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local translation memory storage is unavailable.",
                        )
                    })
                    .and_then(|storage| storage.set_translation_memory_enabled(enabled));
                match result {
                    Ok(()) => {
                        if events
                            .send(WorkerEvent::TranslationMemoryPolicyUpdated { enabled })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationMemoryPolicyRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::ClearTranslationMemory) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local translation memory storage is unavailable.",
                        )
                    })
                    .and_then(Storage::clear_translation_memory);
                match result {
                    Ok(()) => {
                        if events.send(WorkerEvent::TranslationMemoryCleared).is_err() {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationMemoryClearRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::ListTranslationMemory) => {
                let result = storage
                    .as_ref()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local translation memory storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        let entries = storage.translation_memory(MAX_TRANSLATION_MEMORY_ENTRIES)?;
                        let count = storage.translation_memory_count()?;
                        Ok((entries, count))
                    });
                match result {
                    Ok((entries, count)) => {
                        if events
                            .send(WorkerEvent::TranslationMemoryListed { entries, count })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationMemoryActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::DeleteTranslationMemory { cache_key }) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local translation memory storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        storage.delete_translation_memory_entry(cache_key.as_str())?;
                        let entries = storage.translation_memory(MAX_TRANSLATION_MEMORY_ENTRIES)?;
                        let count = storage.translation_memory_count()?;
                        Ok((entries, count))
                    });
                match result {
                    Ok((entries, count)) => {
                        if events
                            .send(WorkerEvent::TranslationMemoryListed { entries, count })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::TranslationMemoryActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::OcrDocumentJob {
                job_id,
                source_name,
                contents,
            }) => {
                let result = TesseractOcr::default()
                    .recognize_pdf(&contents)
                    .map_err(|error| {
                        let kind = match error {
                            crate::ocr::OcrError::Unavailable => ErrorKind::UnsupportedCapability,
                            crate::ocr::OcrError::InvalidDocument
                            | crate::ocr::OcrError::TooManyPages
                            | crate::ocr::OcrError::OutputTooLarge
                            | crate::ocr::OcrError::NoText
                            | crate::ocr::OcrError::Failed => ErrorKind::InvalidConfiguration,
                            crate::ocr::OcrError::TimedOut => ErrorKind::Timeout,
                        };
                        TranslationError::new(kind, error.to_string())
                    })
                    .and_then(|pages| {
                        file_import::document_job_from_ocr(&source_name, &pages).map_err(|error| {
                            TranslationError::new(
                                ErrorKind::InvalidConfiguration,
                                error.to_string(),
                            )
                        })
                    })
                    .and_then(|job| {
                        storage
                            .as_mut()
                            .ok_or_else(|| {
                                TranslationError::new(
                                    ErrorKind::Persistence,
                                    "Local document job storage is unavailable.",
                                )
                            })
                            .and_then(|storage| {
                                storage.save_document_job(&job_id, &job, DocumentJobState::Pending)
                            })
                    });
                match result {
                    Ok(snapshot) => {
                        if events
                            .send(WorkerEvent::DocumentJobUpdated(snapshot))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::CreateDocumentJob { job_id, job }) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local document job storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        storage.save_document_job(&job_id, &job, DocumentJobState::Pending)
                    });
                match result {
                    Ok(snapshot) => {
                        if events
                            .send(WorkerEvent::DocumentJobUpdated(snapshot))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::TranslateDocumentJob {
                job_id,
                source_locale,
                target_locale,
                glossary,
                privacy_mode,
            }) => {
                if privacy_mode == TranslationPrivacyMode::Incognito {
                    let _ = events.send(WorkerEvent::DocumentJobActionRejected(
                        TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "Incognito mode cannot persist document job progress.",
                        ),
                    ));
                    continue;
                }
                let persisted_options = match persisted_document_options(
                    active_profile.as_ref(),
                    selected_model.as_deref(),
                    source_locale.clone(),
                    target_locale.clone(),
                    glossary.clone(),
                ) {
                    Ok(options) => options,
                    Err(error) => {
                        let _ = events.send(WorkerEvent::DocumentJobActionRejected(error));
                        continue;
                    }
                };
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local document job storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        let snapshot = storage.document_job(&job_id)?.ok_or_else(|| {
                            TranslationError::new(
                                ErrorKind::InvalidConfiguration,
                                "The document job was not found.",
                            )
                        })?;
                        if !snapshot.state.is_resumable() {
                            return Err(TranslationError::new(
                                ErrorKind::InvalidConfiguration,
                                "The document job is not ready to translate.",
                            ));
                        }
                        if snapshot.job.pending_count() == 0 {
                            return Err(TranslationError::new(
                                ErrorKind::InvalidConfiguration,
                                "The document job has no pending segments.",
                            ));
                        }
                        storage.save_document_job_options(&job_id, &persisted_options)?;
                        storage.set_document_job_state(&job_id, DocumentJobState::Running)
                    })
                    .and_then(|snapshot| {
                        begin_document_segment(
                            &manager,
                            active_profile.as_ref(),
                            selected_model.as_deref(),
                            &job_id,
                            &snapshot.job,
                            DocumentTranslationOptions {
                                source_locale,
                                target_locale,
                                glossary,
                                privacy_mode,
                            },
                        )
                    });
                match result {
                    Ok(document_translation) => {
                        set_active_cancellation(
                            &active_cancellation,
                            ActiveCancellation::Translation(
                                document_translation.operation.cancellation_handle(),
                            ),
                        );
                        active_document = Some(document_translation);
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::ListDocumentJobs) => {
                let result = storage
                    .as_ref()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local document job storage is unavailable.",
                        )
                    })
                    .and_then(|storage| storage.document_jobs(MAX_DOCUMENT_JOBS));
                match result {
                    Ok(jobs) => {
                        if events
                            .send(WorkerEvent::DocumentJobsListed { jobs })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::ExportDocumentJob { job_id }) => {
                let result = storage
                    .as_ref()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local document job storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        let snapshot = storage.document_job(&job_id)?.ok_or_else(|| {
                            TranslationError::new(
                                ErrorKind::InvalidConfiguration,
                                "The document job was not found.",
                            )
                        })?;
                        let (source_name, contents) =
                            match snapshot.job.reconstruct_bytes_with_target_locale(
                                snapshot
                                    .options
                                    .as_ref()
                                    .map(|options| options.target_locale.as_str()),
                            ) {
                                Ok(contents) => (snapshot.job.source_name, contents),
                                Err(DocumentError::PdfTextEncodingUnsupported) => (
                                    alternative_pdf_source_name(&snapshot.job.source_name),
                                    snapshot.job.reconstruct_alternative_html().map_err(
                                        |error| {
                                            TranslationError::new(
                                                ErrorKind::InvalidConfiguration,
                                                error.to_string(),
                                            )
                                        },
                                    )?,
                                ),
                                Err(error) => {
                                    return Err(TranslationError::new(
                                        ErrorKind::InvalidConfiguration,
                                        error.to_string(),
                                    ));
                                }
                            };
                        Ok((source_name, contents))
                    });
                match result {
                    Ok((source_name, contents)) => {
                        if events
                            .send(WorkerEvent::DocumentJobExported {
                                source_name,
                                contents,
                            })
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::UpdateDocumentSegment {
                job_id,
                index,
                translated_text,
            }) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local document job storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        storage.update_document_segment(&job_id, index, &translated_text)
                    });
                match result {
                    Ok(snapshot) => {
                        if events
                            .send(WorkerEvent::DocumentJobUpdated(snapshot))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::ResumeDocumentJob { job_id }) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local document job storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        start_persisted_document_job_translation(
                            storage,
                            &manager,
                            active_profile.as_ref(),
                            selected_model.as_deref(),
                            &job_id,
                            false,
                        )
                    });
                match result {
                    Ok(document_translation) => {
                        set_active_cancellation(
                            &active_cancellation,
                            ActiveCancellation::Translation(
                                document_translation.operation.cancellation_handle(),
                            ),
                        );
                        active_document = Some(document_translation);
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::RetryDocumentJob { job_id }) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local document job storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        start_persisted_document_job_translation(
                            storage,
                            &manager,
                            active_profile.as_ref(),
                            selected_model.as_deref(),
                            &job_id,
                            true,
                        )
                    });
                match result {
                    Ok(document_translation) => {
                        set_active_cancellation(
                            &active_cancellation,
                            ActiveCancellation::Translation(
                                document_translation.operation.cancellation_handle(),
                            ),
                        );
                        active_document = Some(document_translation);
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::PauseDocumentJob { job_id }) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local document job storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        let snapshot = storage.document_job(&job_id)?.ok_or_else(|| {
                            TranslationError::new(
                                ErrorKind::InvalidConfiguration,
                                "The document job was not found.",
                            )
                        })?;
                        if !matches!(
                            snapshot.state,
                            DocumentJobState::Pending
                                | DocumentJobState::Running
                                | DocumentJobState::Paused
                        ) {
                            return Err(TranslationError::new(
                                ErrorKind::InvalidConfiguration,
                                "The document job cannot be paused in its current state.",
                            ));
                        }
                        storage.set_document_job_state(&job_id, DocumentJobState::Paused)
                    });
                match result {
                    Ok(snapshot) => {
                        if events
                            .send(WorkerEvent::DocumentJobUpdated(snapshot))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(QueuedCommand::CancelDocumentJob { job_id }) => {
                let result = storage
                    .as_mut()
                    .ok_or_else(|| {
                        TranslationError::new(
                            ErrorKind::Persistence,
                            "Local document job storage is unavailable.",
                        )
                    })
                    .and_then(|storage| {
                        storage.set_document_job_state(&job_id, DocumentJobState::Cancelled)
                    });
                match result {
                    Ok(snapshot) => {
                        if events
                            .send(WorkerEvent::DocumentJobUpdated(snapshot))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                    Err(error) => {
                        if events
                            .send(WorkerEvent::DocumentJobActionRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                    }
                }
            }
            Some(
                command @ (QueuedCommand::Translate(_)
                | QueuedCommand::TranslateWithFallback { .. }),
            ) => {
                let (request, fallback_profile_id) = match command {
                    QueuedCommand::Translate(request) => (request, None),
                    QueuedCommand::TranslateWithFallback {
                        request,
                        fallback_profile_id,
                    } => (request, Some(fallback_profile_id)),
                    _ => unreachable!(),
                };
                let request = active_profile.as_ref().map_or(request.clone(), |profile| {
                    request.clone().with_provider_identity(format!(
                        "{}@{}",
                        profile.id().as_str(),
                        profile.base_endpoint()
                    ))
                });
                if let Some(storage) = storage.as_mut() {
                    match storage.lookup_translation_memory(&request) {
                        Ok(Some(entry)) => {
                            if !request.is_incognito()
                                && storage.translation_history_enabled().unwrap_or(false)
                            {
                                match storage
                                    .record_translation_history(&request, &entry.translated_text)
                                {
                                    Ok(()) => match storage.translation_history_count() {
                                        Ok(count) => {
                                            if events
                                                .send(WorkerEvent::TranslationHistoryUpdated {
                                                    count,
                                                })
                                                .is_err()
                                            {
                                                shutting_down = true;
                                                continue;
                                            }
                                        }
                                        Err(error) => {
                                            let _ = events.send(
                                                WorkerEvent::TranslationHistoryPersistenceFailed(
                                                    error,
                                                ),
                                            );
                                        }
                                    },
                                    Err(error) => {
                                        let _ = events.send(
                                            WorkerEvent::TranslationHistoryPersistenceFailed(error),
                                        );
                                    }
                                }
                            }
                            if events
                                .send(WorkerEvent::Translation(TranslationEvent::Started {
                                    sequence: 0,
                                }))
                                .is_err()
                                || events
                                    .send(WorkerEvent::Translation(TranslationEvent::TextDelta {
                                        sequence: 1,
                                        text: entry.translated_text.clone(),
                                    }))
                                    .is_err()
                                || events
                                    .send(WorkerEvent::Translation(TranslationEvent::Completed {
                                        sequence: 2,
                                    }))
                                    .is_err()
                            {
                                shutting_down = true;
                                continue;
                            }
                            continue;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            if events
                                .send(WorkerEvent::TranslationMemoryPersistenceFailed(error))
                                .is_err()
                            {
                                shutting_down = true;
                                continue;
                            }
                        }
                    }
                }
                if let Some(mut previous) = fallback_manager.take() {
                    previous.disconnect();
                    fallback_profile = None;
                }
                let mut fallback_model = None;
                if let Some(fallback_id) = fallback_profile_id.as_ref() {
                    if active_profile
                        .as_ref()
                        .is_some_and(|profile| profile.id() == fallback_id)
                    {
                        let error = TranslationError::new(
                            ErrorKind::InvalidConfiguration,
                            "The fallback profile must differ from the active provider.",
                        );
                        if events
                            .send(WorkerEvent::TranslationRejected(error))
                            .is_err()
                        {
                            shutting_down = true;
                        }
                        continue;
                    }
                    let cancellation = shutdown_cancellation.child_token();
                    set_active_cancellation(
                        &active_cancellation,
                        ActiveCancellation::Connection(cancellation.clone()),
                    );
                    let result = connect_fallback_candidate(
                        secret_broker.clone(),
                        storage.as_mut(),
                        fallback_id,
                        &cancellation,
                        &mut secret_requests,
                    )
                    .await;
                    clear_active_cancellation(&active_cancellation);
                    match result {
                        Ok((candidate, profile, model)) => {
                            fallback_manager = Some(candidate);
                            fallback_profile = Some(profile);
                            fallback_model = Some(model);
                        }
                        Err(error) => {
                            if events
                                .send(WorkerEvent::TranslationRejected(error))
                                .is_err()
                            {
                                shutting_down = true;
                            }
                            continue;
                        }
                    }
                }
                match begin_translation(&manager, selected_model.as_deref(), request) {
                    Ok(mut active_translation) => {
                        active_translation.fallback_profile_id = fallback_profile_id;
                        active_translation.fallback_model = fallback_model;
                        set_active_cancellation(
                            &active_cancellation,
                            ActiveCancellation::Translation(
                                active_translation.operation.cancellation_handle(),
                            ),
                        );
                        active = Some(active_translation);
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
        operation.operation.cancel();
    }
    reject_queued_commands_for_shutdown(&mut commands, &events);
    clear_active_cancellation(&active_cancellation);
    manager.disconnect();
    if let Some(mut fallback_manager) = fallback_manager {
        fallback_manager.disconnect();
    }
    server.shutdown().await;
    drop(storage);
    let _ = events.send(WorkerEvent::Stopped);
}

fn reject_queued_commands_for_shutdown(
    commands: &mut CommandReceiver<QueuedCommand>,
    events: &SyncSender<WorkerEvent>,
) {
    while let Ok(command) = commands.try_recv() {
        let result = match command {
            QueuedCommand::Connect { profile, .. } => events.send(WorkerEvent::ProviderRejected {
                profile,
                error: TranslationError::cancelled(),
            }),
            QueuedCommand::SelectModel {
                profile_id,
                model_id,
            } => events.send(WorkerEvent::ModelSelectionRejected {
                profile_id,
                model_id,
                error: TranslationError::cancelled(),
            }),
            QueuedCommand::DeleteSavedProfile { profile_id } => {
                events.send(WorkerEvent::ProfileDeletionRejected {
                    profile_id,
                    error: TranslationError::cancelled(),
                })
            }
            QueuedCommand::ClearTranslationHistory => events.send(
                WorkerEvent::TranslationHistoryClearRejected(TranslationError::cancelled()),
            ),
            QueuedCommand::ListTranslationHistory
            | QueuedCommand::DeleteTranslationHistory { .. } => events.send(
                WorkerEvent::TranslationHistoryActionRejected(TranslationError::cancelled()),
            ),
            QueuedCommand::SetTranslationHistoryEnabled { .. } => events.send(
                WorkerEvent::TranslationHistoryPolicyRejected(TranslationError::cancelled()),
            ),
            QueuedCommand::SetTranslationMemoryEnabled { .. } => events.send(
                WorkerEvent::TranslationMemoryPolicyRejected(TranslationError::cancelled()),
            ),
            QueuedCommand::ClearTranslationMemory => events.send(
                WorkerEvent::TranslationMemoryClearRejected(TranslationError::cancelled()),
            ),
            QueuedCommand::ListTranslationMemory
            | QueuedCommand::DeleteTranslationMemory { .. } => events.send(
                WorkerEvent::TranslationMemoryActionRejected(TranslationError::cancelled()),
            ),
            QueuedCommand::CreateDocumentJob { .. }
            | QueuedCommand::OcrDocumentJob { .. }
            | QueuedCommand::TranslateDocumentJob { .. }
            | QueuedCommand::ListDocumentJobs
            | QueuedCommand::ExportDocumentJob { .. }
            | QueuedCommand::UpdateDocumentSegment { .. }
            | QueuedCommand::ResumeDocumentJob { .. }
            | QueuedCommand::RetryDocumentJob { .. }
            | QueuedCommand::PauseDocumentJob { .. }
            | QueuedCommand::CancelDocumentJob { .. } => events.send(
                WorkerEvent::DocumentJobActionRejected(TranslationError::cancelled()),
            ),
            QueuedCommand::Translate(_) | QueuedCommand::TranslateWithFallback { .. } => events
                .send(WorkerEvent::TranslationRejected(
                    TranslationError::cancelled(),
                )),
            QueuedCommand::Cancel | QueuedCommand::Shutdown => continue,
        };
        if result.is_err() {
            break;
        }
    }
}

fn open_profile_storage(
    path: &Path,
) -> Result<(Storage, Vec<ProviderProfile>, Option<ProviderProfileId>), TranslationError> {
    prepare_database_file(path)?;
    let storage = Storage::open(path)?;
    let profiles = storage.provider_profiles()?;
    let active_profile_id = storage
        .active_provider_profile()?
        .map(|profile| profile.id().clone());
    Ok((storage, profiles, active_profile_id))
}

fn prepare_database_file(path: &Path) -> Result<(), TranslationError> {
    if !path.is_absolute() {
        return Err(TranslationError::new(
            ErrorKind::Persistence,
            "The profile database path must be absolute.",
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        TranslationError::new(
            ErrorKind::Persistence,
            "The profile database directory is invalid.",
        )
    })?;
    ensure_no_symbolic_path_components(parent)?;
    let mut directory = DirBuilder::new();
    directory.recursive(true).mode(0o700);
    directory.create(parent).map_err(|_| {
        TranslationError::new(
            ErrorKind::Persistence,
            "The profile database directory could not be created.",
        )
    })?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(|_| {
        TranslationError::new(
            ErrorKind::Persistence,
            "The profile database directory could not be inspected.",
        )
    })?;
    if !parent_metadata.is_dir()
        || parent_metadata.file_type().is_symlink()
        || parent_metadata.permissions().mode() & 0o077 != 0
    {
        return Err(TranslationError::new(
            ErrorKind::Persistence,
            "The profile database directory permissions are not private.",
        ));
    }
    ensure_no_symbolic_path_components(parent)?;
    ensure_no_symbolic_path_components(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.is_file() || metadata.nlink() != 1 => {
            return Err(TranslationError::new(
                ErrorKind::Persistence,
                "The profile database must be a private regular file.",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            return Err(TranslationError::new(
                ErrorKind::Persistence,
                "The profile database path could not be inspected.",
            ));
        }
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| {
            TranslationError::new(
                ErrorKind::Persistence,
                "The profile database file could not be opened.",
            )
        })?;
    let opened_metadata = file.metadata().map_err(|_| {
        TranslationError::new(
            ErrorKind::Persistence,
            "The profile database file could not be inspected.",
        )
    })?;
    let path_metadata = fs::symlink_metadata(path).map_err(|_| {
        TranslationError::new(
            ErrorKind::Persistence,
            "The profile database path could not be inspected.",
        )
    })?;
    if !opened_metadata.is_file()
        || opened_metadata.nlink() != 1
        || path_metadata.file_type().is_symlink()
        || !path_metadata.is_file()
        || path_metadata.nlink() != 1
        || opened_metadata.dev() != path_metadata.dev()
        || opened_metadata.ino() != path_metadata.ino()
    {
        return Err(TranslationError::new(
            ErrorKind::Persistence,
            "The profile database must be a private regular file.",
        ));
    }
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| {
            TranslationError::new(
                ErrorKind::Persistence,
                "The profile database file permissions could not be restricted.",
            )
        })
}

fn ensure_no_symbolic_path_components(path: &Path) -> Result<(), TranslationError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(TranslationError::new(
                    ErrorKind::Persistence,
                    "The profile database path cannot contain symbolic links.",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => {
                return Err(TranslationError::new(
                    ErrorKind::Persistence,
                    "The profile database path components could not be inspected.",
                ));
            }
        }
    }
    Ok(())
}

fn validate_persistence_request(
    profile: &ProviderProfile,
    persistence: PersistenceIntent,
    storage_available: bool,
) -> Result<(), TranslationError> {
    if profile
        .secret_ref()
        .is_some_and(linguamesh_domain::SecretRef::is_persistent)
    {
        #[cfg(not(feature = "gui"))]
        return Err(TranslationError::new(
            ErrorKind::SecureStorageUnavailable,
            "Secure credential storage is unavailable.",
        ));
    }
    if persistence == PersistenceIntent::Persistent && !storage_available {
        return Err(TranslationError::new(
            ErrorKind::Persistence,
            "Profile storage is unavailable; use session-only mode.",
        ));
    }
    Ok(())
}

// 持久化写失败后停止复用存储句柄，并将活动连接降级为仅会话模式。
fn degrade_profile_storage_after_persistence_error(
    error: &TranslationError,
    storage: &mut Option<Storage>,
    active_saved_profile: &mut Option<ProviderProfile>,
) -> Option<TranslationError> {
    if error.kind != ErrorKind::Persistence || storage.take().is_none() {
        return None;
    }
    *active_saved_profile = None;
    Some(error.clone())
}

fn finish_candidate_connection(
    profile: &ProviderProfile,
    models: Vec<ModelDescriptor>,
    persistence: PersistenceIntent,
    storage: Option<&mut Storage>,
    cancellation: &CancellationToken,
) -> Result<ConnectedCandidate, TranslationError> {
    if cancellation.is_cancelled() {
        return Err(TranslationError::cancelled());
    }
    let selected_model = profile
        .selected_model()
        .filter(|selected| models.iter().any(|model| model.id == *selected))
        .map(str::to_owned);
    let runtime_profile = profile
        .clone()
        .with_selected_model(selected_model.clone())
        .map_err(|error| map_profile_error(&error))?;
    let saved_profile = if persistence == PersistenceIntent::Persistent {
        let saved_profile = profile_without_secret(&runtime_profile)?;
        if cancellation.is_cancelled() {
            return Err(TranslationError::cancelled());
        }
        storage
            .ok_or_else(|| {
                TranslationError::new(
                    ErrorKind::Persistence,
                    "Profile storage became unavailable.",
                )
            })?
            .save_and_activate_provider(&saved_profile)?;
        Some(saved_profile)
    } else {
        None
    };
    Ok(ConnectedCandidate {
        runtime_profile,
        saved_profile,
        models,
        selected_model,
    })
}

fn profile_without_secret(profile: &ProviderProfile) -> Result<ProviderProfile, TranslationError> {
    ProviderProfile::new(
        profile.id().clone(),
        profile.display_name(),
        profile.preset_id(),
        profile.adapter_type(),
        profile.base_endpoint(),
        profile
            .secret_ref()
            .filter(|secret_ref| secret_ref.is_persistent())
            .cloned(),
    )
    .map(|saved| saved.with_enabled(profile.enabled()))
    .and_then(|saved| saved.with_selected_model(profile.selected_model().map(str::to_owned)))
    .map_err(|error| map_profile_error(&error))
}

fn prepare_model_selection(
    manager: &ProviderManager,
    active_profile: Option<&ProviderProfile>,
    active_saved_profile: Option<&ProviderProfile>,
    storage: Option<&mut Storage>,
    profile_id: &ProviderProfileId,
    model_id: &str,
) -> Result<(ProviderProfile, Option<ProviderProfile>), TranslationError> {
    select_model(manager, profile_id, model_id)?;
    let updated_profile = active_profile
        .filter(|profile| profile.id() == profile_id)
        .ok_or_else(|| {
            TranslationError::new(
                ErrorKind::InvalidConfiguration,
                "The model selection belongs to a stale provider.",
            )
        })?
        .clone()
        .with_selected_model(Some(model_id.to_owned()))
        .map_err(|error| map_profile_error(&error))?;
    let updated_saved_profile = match active_saved_profile {
        Some(saved_profile) => {
            let updated = saved_profile
                .clone()
                .with_selected_model(Some(model_id.to_owned()))
                .map_err(|error| map_profile_error(&error))?;
            storage
                .ok_or_else(|| {
                    TranslationError::new(
                        ErrorKind::Persistence,
                        "Profile storage became unavailable.",
                    )
                })?
                .save_and_activate_provider(&updated)?;
            Some(updated)
        }
        None => None,
    };
    Ok((updated_profile, updated_saved_profile))
}

fn delete_saved_profile(
    storage: Option<&mut Storage>,
    profile_id: &ProviderProfileId,
) -> Result<Option<linguamesh_domain::SecretRef>, TranslationError> {
    let storage = storage.ok_or_else(|| {
        TranslationError::new(
            ErrorKind::Persistence,
            "Profile storage is unavailable; no saved profile was removed.",
        )
    })?;
    let secret_ref = storage
        .provider_profile(profile_id)?
        .ok_or_else(|| {
            TranslationError::new(
                ErrorKind::InvalidConfiguration,
                "The saved provider profile no longer exists.",
            )
        })?
        .secret_ref()
        .filter(|secret_ref| secret_ref.is_persistent())
        .cloned();
    if storage.delete_provider_profile(profile_id)? {
        Ok(secret_ref)
    } else {
        Err(TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "The saved provider profile no longer exists.",
        ))
    }
}

fn map_profile_error(error: &linguamesh_domain::ProfileValidationError) -> TranslationError {
    TranslationError::new(
        ErrorKind::InvalidConfiguration,
        format!("The provider profile is invalid: {error}"),
    )
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
    cancellation: &CancellationToken,
    requests: &mut HostSecretRequests,
) -> Result<Vec<ModelDescriptor>, TranslationError> {
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
                let response = if profile.secret_ref() == Some(&required_ref) {
                    if let Some(secret) = session_secret.take() {
                        request.provide_secret(secret)
                    } else if required_ref.is_persistent() {
                        #[cfg(feature = "gui")]
                        {
                            let resolution = tokio::task::spawn_blocking(move || {
                                crate::secret_service::resolve_secret(&required_ref)
                            })
                            .await;
                            match resolution {
                                Ok(Ok(secret)) => request.provide_secret(secret),
                                Ok(Err(crate::secret_service::LookupError::Missing)) => {
                                    request.reject_unavailable()
                                }
                                Ok(Err(_)) | Err(_) => {
                                    request.reject_secure_storage_unavailable()
                                }
                            }
                        }
                        #[cfg(not(feature = "gui"))]
                        {
                            request.reject_secure_storage_unavailable()
                        }
                    } else {
                        request.reject_unavailable()
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

// 只从本地保存配置建立获准回退连接，并重新发现其模型。
async fn connect_fallback_candidate(
    secret_broker: HostSecretBroker,
    storage: Option<&mut Storage>,
    profile_id: &ProviderProfileId,
    cancellation: &CancellationToken,
    requests: &mut HostSecretRequests,
) -> Result<(ProviderManager, ProviderProfile, String), TranslationError> {
    let profile = storage
        .ok_or_else(|| {
            TranslationError::new(
                ErrorKind::InvalidConfiguration,
                "Saved fallback profiles are unavailable.",
            )
        })?
        .provider_profile(profile_id)?
        .ok_or_else(|| {
            TranslationError::new(
                ErrorKind::InvalidConfiguration,
                "The selected fallback profile is no longer saved.",
            )
        })?;
    let model = profile.selected_model().map(str::to_owned).ok_or_else(|| {
        TranslationError::new(
            ErrorKind::ModelUnavailable,
            "Select a model for the approved fallback profile before translating.",
        )
    })?;
    let mut manager = ProviderManager::new(secret_broker);
    let models = connect_candidate(&mut manager, &profile, None, cancellation, requests).await?;
    if !models.iter().any(|candidate| candidate.id == model) {
        manager.disconnect();
        return Err(TranslationError::new(
            ErrorKind::ModelUnavailable,
            "The selected model is not available from the approved fallback provider.",
        ));
    }
    Ok((manager, profile, model))
}

fn select_model(
    manager: &ProviderManager,
    requested_profile_id: &ProviderProfileId,
    model_id: &str,
) -> Result<(), TranslationError> {
    let profile_id = manager.active_profile_id().ok_or_else(|| {
        TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "Connect a provider before selecting a model.",
        )
    })?;
    if profile_id != requested_profile_id {
        return Err(TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "The model selection belongs to a stale provider.",
        ));
    }
    if !manager.models().iter().any(|model| model.id == model_id) {
        return Err(TranslationError::new(
            ErrorKind::ModelUnavailable,
            "The selected model is not available from the active provider.",
        ));
    }
    Ok(())
}

fn begin_translation(
    manager: &ProviderManager,
    selected_model: Option<&str>,
    request: TranslationRequest,
) -> Result<ActiveTranslation, TranslationError> {
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
    Ok(ActiveTranslation {
        operation: engine.translate(request.clone()),
        request,
        output: String::new(),
        fallback_profile_id: None,
        fallback_model: None,
        fallback_attempted: false,
        suppress_fallback_started: false,
        next_sequence: 0,
    })
}

// 仅允许网络或超时错误触发用户明确批准的回退配置。
fn is_retryable_fallback(error: &TranslationError) -> bool {
    matches!(error.kind, ErrorKind::Network | ErrorKind::Timeout)
}

// 将回退操作序号接续到主操作之后，保留已收到的部分输出。
fn remap_translation_event(event: TranslationEvent, sequence: u64) -> TranslationEvent {
    match event {
        TranslationEvent::Started { .. } => TranslationEvent::Started { sequence },
        TranslationEvent::TextDelta { text, .. } => TranslationEvent::TextDelta { sequence, text },
        TranslationEvent::Completed { .. } => TranslationEvent::Completed { sequence },
        TranslationEvent::Cancelled { .. } => TranslationEvent::Cancelled { sequence },
        TranslationEvent::Failed { error, .. } => TranslationEvent::Failed { sequence, error },
    }
}

fn begin_document_segment(
    manager: &ProviderManager,
    active_profile: Option<&ProviderProfile>,
    selected_model: Option<&str>,
    job_id: &str,
    job: &DocumentJob,
    options: DocumentTranslationOptions,
) -> Result<ActiveDocumentTranslation, TranslationError> {
    let segment = job
        .segments
        .iter()
        .find(|segment| {
            segment.kind == linguamesh_document::DocumentSegmentKind::Prose
                && segment.translated_text.is_none()
        })
        .ok_or_else(|| {
            TranslationError::new(
                ErrorKind::InvalidConfiguration,
                "The document job has no pending prose segment.",
            )
        })?;
    let model_id = selected_model.ok_or_else(|| {
        TranslationError::new(
            ErrorKind::ModelUnavailable,
            "Select a model before translating a document job.",
        )
    })?;
    // 将 CSV 或结构化文档的字符串字段解码后再提交给翻译提供方。
    let source_text = job
        .translation_source_text(segment.index)
        .map_err(|error| {
            TranslationError::new(ErrorKind::InvalidConfiguration, error.to_string())
        })?;
    let mut request = TranslationRequest::new(
        source_text.into_owned(),
        options.target_locale.clone(),
        model_id,
    )
    .with_privacy_mode(options.privacy_mode);
    if let Some(glossary) = options.glossary.clone() {
        request = request.with_glossary(glossary);
    }
    if let Some(source_locale) = options.source_locale.as_deref() {
        request.source_locale = Some(source_locale.to_owned());
    }
    if let Some(profile) = active_profile {
        request = request.with_provider_identity(format!(
            "{}@{}",
            profile.id().as_str(),
            profile.base_endpoint()
        ));
    }
    let active_translation = begin_translation(manager, selected_model, request)?;
    Ok(ActiveDocumentTranslation {
        job_id: job_id.to_owned(),
        segment_index: segment.index,
        source_locale: options.source_locale,
        target_locale: options.target_locale,
        glossary: options.glossary,
        privacy_mode: options.privacy_mode,
        operation: active_translation.operation,
        output: String::new(),
        cancel_requested: false,
        pause_requested: false,
        stop_after_active: false,
    })
}

fn prepare_document_job_for_start(
    storage: &mut Storage,
    job_id: &str,
    retry: bool,
) -> Result<DocumentJobSnapshot, TranslationError> {
    let snapshot = storage.document_job(job_id)?.ok_or_else(|| {
        TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "The document job was not found.",
        )
    })?;
    let allowed = if retry {
        matches!(
            snapshot.state,
            DocumentJobState::Pending
                | DocumentJobState::Running
                | DocumentJobState::Paused
                | DocumentJobState::Cancelled
                | DocumentJobState::Failed
        )
    } else {
        snapshot.state.is_resumable()
    };
    if !allowed {
        return Err(TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "The document job is not ready to translate.",
        ));
    }
    if snapshot.job.pending_count() == 0 {
        return Err(TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "The document job has no pending segments.",
        ));
    }
    storage.set_document_job_state(job_id, DocumentJobState::Running)
}

fn persisted_document_options(
    active_profile: Option<&ProviderProfile>,
    selected_model: Option<&str>,
    source_locale: Option<String>,
    target_locale: String,
    glossary: Option<Glossary>,
) -> Result<DocumentJobOptions, TranslationError> {
    let model_id = selected_model.ok_or_else(|| {
        TranslationError::new(
            ErrorKind::ModelUnavailable,
            "Select a model before translating a document job.",
        )
    })?;
    let profile = active_profile.ok_or_else(|| {
        TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "Connect an active provider before translating a document job.",
        )
    })?;
    Ok(DocumentJobOptions {
        source_locale,
        target_locale,
        model_id: model_id.to_owned(),
        provider_id: profile.id().as_str().to_owned(),
        glossary,
    })
}

// 为无法直接编码非 ASCII PDF 文本的任务生成结构化 HTML 文件名。
fn alternative_pdf_source_name(source_name: &str) -> String {
    source_name.rsplit_once('.').map_or_else(
        || format!("{source_name}.html"),
        |(base, _)| format!("{base}.html"),
    )
}

fn start_persisted_document_job_translation(
    storage: &mut Storage,
    manager: &ProviderManager,
    active_profile: Option<&ProviderProfile>,
    selected_model: Option<&str>,
    job_id: &str,
    retry: bool,
) -> Result<ActiveDocumentTranslation, TranslationError> {
    let snapshot = storage.document_job(job_id)?.ok_or_else(|| {
        TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "The document job was not found.",
        )
    })?;
    let options = snapshot.options.clone().ok_or_else(|| {
        TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "The document job has no saved translation options; start it again.",
        )
    })?;
    let profile = active_profile.ok_or_else(|| {
        TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "Reconnect the saved provider before resuming this document job.",
        )
    })?;
    if profile.id().as_str() != options.provider_id {
        return Err(TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "The saved document provider is not the active provider.",
        ));
    }
    if selected_model != Some(options.model_id.as_str()) {
        return Err(TranslationError::new(
            ErrorKind::ModelUnavailable,
            "Select the saved document model before resuming this document job.",
        ));
    }
    start_document_job_translation(
        storage,
        manager,
        active_profile,
        selected_model,
        job_id,
        DocumentTranslationOptions {
            source_locale: options.source_locale,
            target_locale: options.target_locale,
            glossary: options.glossary,
            privacy_mode: TranslationPrivacyMode::Standard,
        },
        retry,
    )
}

fn start_document_job_translation(
    storage: &mut Storage,
    manager: &ProviderManager,
    active_profile: Option<&ProviderProfile>,
    selected_model: Option<&str>,
    job_id: &str,
    options: DocumentTranslationOptions,
    retry: bool,
) -> Result<ActiveDocumentTranslation, TranslationError> {
    let snapshot = prepare_document_job_for_start(storage, job_id, retry)?;
    begin_document_segment(
        manager,
        active_profile,
        selected_model,
        job_id,
        &snapshot.job,
        options,
    )
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
        WorkerCommand, WorkerEvent, alternative_pdf_source_name, profile_without_secret,
        validate_core_contract,
    };
    use crate::model::{ProviderProfile, ProviderProfileId};
    use linguamesh_document::{DocumentFormat, DocumentJob, DocumentJobState};
    use linguamesh_domain::{
        CoreCompatibility, ErrorKind, SecretRef, SecretRefNamespace, SecretValue, TranslationError,
        TranslationEvent, TranslationPrivacyMode, TranslationRequest,
    };
    use linguamesh_engine::core_compatibility;
    use linguamesh_storage::Storage;
    use linguamesh_testkit::FakeProviderServer;
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};
    use tokio::runtime::Builder;
    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;

    enum FakeMode {
        Standard,
        Authenticated(&'static str),
        Delayed(Duration),
        OllamaCompatible,
        OllamaNative,
    }

    static TEST_DATABASE_COUNTER: AtomicUsize = AtomicUsize::new(0);
    const LINUX_ENOSPC: i32 = 28;

    #[test]
    fn pdf_alternative_export_uses_html_suffix() {
        assert_eq!(alternative_pdf_source_name("report.pdf"), "report.html");
        assert_eq!(alternative_pdf_source_name("report"), "report.html");
    }

    struct TestDatabase {
        directory: PathBuf,
        path: PathBuf,
    }

    impl TestDatabase {
        fn new() -> Self {
            let sequence = TEST_DATABASE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "linguamesh-linux-test-{}-{sequence}",
                std::process::id()
            ));
            assert!(
                !directory.exists(),
                "test database directory must be unique"
            );
            let path = directory.join("state.sqlite3");
            Self { directory, path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn assert_private_permissions(&self) {
            let directory_mode = fs::metadata(&self.directory)
                .expect("database directory metadata")
                .permissions()
                .mode()
                & 0o777;
            let database_mode = fs::metadata(&self.path)
                .expect("database file metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(directory_mode, 0o700);
            assert_eq!(database_mode, 0o600);
        }

        fn assert_absent_from_files(&self, forbidden: &[&str]) {
            for entry in fs::read_dir(&self.directory).expect("database directory") {
                let path = entry.expect("database directory entry").path();
                if !path.is_file() {
                    continue;
                }
                let bytes = fs::read(path).expect("database artifact");
                for value in forbidden {
                    assert!(
                        !bytes
                            .windows(value.len())
                            .any(|candidate| candidate == value.as_bytes()),
                        "forbidden value must not be persisted"
                    );
                }
            }
        }
    }

    impl Drop for TestDatabase {
        fn drop(&mut self) {
            if self.directory.exists() {
                let _ = fs::remove_dir_all(&self.directory);
            }
        }
    }

    struct RuntimeFaultMount {
        directory: PathBuf,
        filler: Option<fs::File>,
        externally_managed: bool,
    }

    impl RuntimeFaultMount {
        fn new() -> Self {
            if let Some(directory) = std::env::var_os("LINGUAMESH_RUNTIME_STORAGE_FAULT_DIRECTORY")
            {
                let directory = PathBuf::from(directory);
                Self::validate_mount(&directory);
                return Self {
                    directory,
                    filler: None,
                    externally_managed: true,
                };
            }
            let sequence = TEST_DATABASE_COUNTER.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "linguamesh-linux-runtime-fault-{}-{sequence}",
                std::process::id()
            ));
            assert!(!directory.exists(), "fault mount path must be unique");
            fs::create_dir(&directory).expect("fault mount directory");
            let status = Command::new("mount")
                .args([
                    "-t",
                    "tmpfs",
                    "-o",
                    "mode=0700,size=8m,nosuid,nodev,noexec",
                    "tmpfs",
                ])
                .arg(&directory)
                .status()
                .expect("tmpfs mount command");
            if !status.success() {
                fs::remove_dir(&directory).expect("remove failed fault mount directory");
                panic!("the private tmpfs mount failed");
            }
            Self::validate_mount(&directory);
            Self {
                directory,
                filler: None,
                externally_managed: false,
            }
        }

        fn validate_mount(directory: &Path) {
            assert!(directory.is_absolute(), "fault mount path must be absolute");
            let metadata = fs::symlink_metadata(directory).expect("fault mount metadata");
            assert!(metadata.is_dir(), "fault mount must be a directory");
            assert!(
                !metadata.file_type().is_symlink(),
                "fault mount cannot be a symbolic link"
            );
            assert_eq!(
                metadata.permissions().mode() & 0o077,
                0,
                "fault mount must be private"
            );
            let parent = directory.parent().expect("fault mount parent");
            let parent_metadata = fs::metadata(parent).expect("fault mount parent metadata");
            assert_ne!(
                metadata.dev(),
                parent_metadata.dev(),
                "fault path must be a distinct mounted filesystem"
            );
            assert!(
                fs::read_dir(directory)
                    .expect("read fault mount")
                    .next()
                    .is_none(),
                "fault mount must start empty"
            );
        }

        fn database_path(&self) -> PathBuf {
            self.directory.join("state.sqlite3")
        }

        fn exhaust_space(&mut self) {
            assert!(self.filler.is_none(), "storage fault is already active");
            let sync_status = Command::new("sync").status().expect("sync command");
            assert!(
                sync_status.success(),
                "sync before filling the fault filesystem failed"
            );
            let filler_path = self.directory.join("storage-fault-fill");
            let mut filler = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&filler_path)
                .expect("storage fault filler");
            let block = vec![0_u8; 64 * 1024].into_boxed_slice();
            loop {
                match filler.write(&block) {
                    Ok(0) => panic!("the storage fault filler stopped before ENOSPC"),
                    Ok(_) => {}
                    Err(error) if error.raw_os_error() == Some(LINUX_ENOSPC) => break,
                    Err(error) => panic!("the storage fault filler failed unexpectedly: {error}"),
                }
            }
            self.filler = Some(filler);
        }

        fn clear_fault(&mut self) {
            drop(self.filler.take());
            fs::remove_file(self.directory.join("storage-fault-fill"))
                .expect("remove storage fault filler");
        }

        fn finish(mut self) {
            assert!(self.filler.is_none(), "storage fault must be cleared");
            if self.externally_managed {
                self.directory.clear();
                return;
            }
            let status = Command::new("umount")
                .arg(&self.directory)
                .status()
                .expect("fault mount unmount command");
            assert!(status.success(), "fault mount cleanup failed");
            self.externally_managed = true;
            fs::remove_dir(&self.directory).expect("remove fault mount directory");
            self.directory.clear();
        }
    }

    impl Drop for RuntimeFaultMount {
        fn drop(&mut self) {
            if self.filler.take().is_some() {
                let _ = fs::remove_file(self.directory.join("storage-fault-fill"));
            }
            if self.externally_managed || self.directory.as_os_str().is_empty() {
                return;
            }
            let unmounted = Command::new("umount")
                .arg(&self.directory)
                .status()
                .is_ok_and(|status| status.success());
            if unmounted && self.directory.exists() {
                let _ = fs::remove_dir_all(&self.directory);
            }
        }
    }

    struct ExternalFakeProvider {
        endpoint: String,
        model_requests: Arc<AtomicUsize>,
        chat_requests: Arc<AtomicUsize>,
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
                    let native_ollama = matches!(&mode, FakeMode::OllamaNative);
                    let server = match mode {
                        FakeMode::Standard => FakeProviderServer::start().await,
                        FakeMode::Authenticated(required_secret) => {
                            FakeProviderServer::start_requiring_bearer_token(SecretValue::new(
                                required_secret,
                            ))
                            .await
                        }
                        FakeMode::Delayed(delay) => {
                            FakeProviderServer::start_with_model_delay(delay).await
                        }
                        FakeMode::OllamaCompatible => {
                            FakeProviderServer::start_ollama_compatible().await
                        }
                        FakeMode::OllamaNative => FakeProviderServer::start_ollama_native().await,
                    }
                    .expect("external fake provider");
                    ready_sender
                        .send((
                            if native_ollama {
                                server.ollama_base_url()
                            } else {
                                server.base_url()
                            },
                            server.model_request_counter(),
                            server.chat_request_counter(),
                        ))
                        .expect("provider endpoint");
                    let _ = shutdown_receiver.await;
                    server.shutdown().await;
                });
            });
            let (endpoint, model_requests, chat_requests) = ready_receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("provider startup");
            Self {
                endpoint,
                model_requests,
                chat_requests,
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

    fn assert_chat_requests(
        first: &ExternalFakeProvider,
        first_expected: usize,
        second: &ExternalFakeProvider,
        second_expected: usize,
    ) {
        assert_eq!(first.chat_requests.load(Ordering::SeqCst), first_expected);
        assert_eq!(second.chat_requests.load(Ordering::SeqCst), second_expected);
    }

    fn profile(
        id: &str,
        endpoint: &str,
        secret_ref: Option<SecretRef>,
        selected_model: Option<&str>,
    ) -> ProviderProfile {
        profile_with_preset(
            id,
            "custom-openai-compatible",
            endpoint,
            secret_ref,
            selected_model,
        )
    }

    fn profile_with_preset(
        id: &str,
        preset_id: &str,
        endpoint: &str,
        secret_ref: Option<SecretRef>,
        selected_model: Option<&str>,
    ) -> ProviderProfile {
        ProviderProfile::new(
            ProviderProfileId::parse(id).expect("profile ID"),
            format!("{id} display name"),
            preset_id,
            if preset_id == "ollama" {
                "ollama_chat"
            } else {
                "openai_chat_completions"
            },
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

    fn started_worker_with_database(path: &Path) -> (CoreWorker, Option<ProviderProfile>, String) {
        let worker = CoreWorker::spawn_with_database(path);
        let mut restored_snapshot = None;
        loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("worker startup event")
            {
                WorkerEvent::ProfilesRestored {
                    profiles,
                    active_profile_id,
                } => {
                    assert!(restored_snapshot.is_none());
                    let restored_profile = active_profile_id.as_ref().map(|active_profile_id| {
                        profiles
                            .iter()
                            .find(|profile| profile.id() == active_profile_id)
                            .cloned()
                            .expect("active profile belongs to startup snapshot")
                    });
                    restored_snapshot = Some(restored_profile);
                }
                WorkerEvent::TranslationHistoryRestored { .. }
                | WorkerEvent::TranslationHistoryPolicyRestored { .. }
                | WorkerEvent::TranslationMemoryRestored { .. }
                | WorkerEvent::DocumentJobsRestored { .. } => {}
                WorkerEvent::DemoProviderReady { endpoint } => {
                    return (
                        worker,
                        restored_snapshot.expect("profile storage snapshot"),
                        endpoint,
                    );
                }
                WorkerEvent::ProfileStorageUnavailable(error) => {
                    panic!("profile storage unavailable: {error}");
                }
                _ => panic!("unexpected worker startup event"),
            }
        }
    }

    fn started_worker_with_database_snapshot(
        path: &Path,
    ) -> (
        CoreWorker,
        Vec<ProviderProfile>,
        Option<ProviderProfileId>,
        String,
    ) {
        let worker = CoreWorker::spawn_with_database(path);
        let mut restored_snapshot = None;
        loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("worker startup event")
            {
                WorkerEvent::ProfilesRestored {
                    profiles,
                    active_profile_id,
                } => {
                    assert!(
                        restored_snapshot
                            .replace((profiles, active_profile_id))
                            .is_none()
                    );
                }
                WorkerEvent::TranslationHistoryRestored { .. }
                | WorkerEvent::TranslationHistoryPolicyRestored { .. }
                | WorkerEvent::TranslationMemoryRestored { .. }
                | WorkerEvent::DocumentJobsRestored { .. } => {}
                WorkerEvent::DemoProviderReady { endpoint } => {
                    let (profiles, active_profile_id) =
                        restored_snapshot.expect("profile storage snapshot");
                    return (worker, profiles, active_profile_id, endpoint);
                }
                WorkerEvent::ProfileStorageUnavailable(error) => {
                    panic!("profile storage unavailable: {error}");
                }
                _ => panic!("unexpected worker startup event"),
            }
        }
    }

    fn shutdown(worker: &CoreWorker) {
        worker
            .try_send(WorkerCommand::Shutdown)
            .expect("shutdown command");
        loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("worker shutdown event")
            {
                WorkerEvent::Stopped => return,
                WorkerEvent::Translation(event) if event.is_terminal() => {}
                WorkerEvent::TranslationHistoryUpdated { .. }
                | WorkerEvent::TranslationMemoryPersistenceFailed(_)
                | WorkerEvent::TranslationHistoryPersistenceFailed(_)
                | WorkerEvent::DocumentJobActionRejected(_)
                | WorkerEvent::DocumentJobStorageUnavailable(_)
                | WorkerEvent::DocumentJobUpdated(_)
                | WorkerEvent::DocumentJobsListed { .. }
                | WorkerEvent::DocumentJobsRestored { .. } => {}
                _ => panic!("unexpected worker shutdown event"),
            }
        }
    }

    fn runtime_profile_with_session_secret(saved: &ProviderProfile) -> ProviderProfile {
        ProviderProfile::new(
            saved.id().clone(),
            saved.display_name(),
            saved.preset_id(),
            saved.adapter_type(),
            saved.base_endpoint(),
            Some(SecretRef::new(SecretRefNamespace::Session)),
        )
        .expect("runtime profile")
        .with_enabled(saved.enabled())
        .with_selected_model(saved.selected_model().map(str::to_owned))
        .expect("runtime selected model")
    }

    fn connect_event(
        worker: &CoreWorker,
        profile: ProviderProfile,
        secret: Option<SecretValue>,
        persistence: PersistenceIntent,
    ) -> Result<
        (
            ProviderProfile,
            Vec<linguamesh_domain::ModelDescriptor>,
            Option<ProviderProfile>,
        ),
        TranslationError,
    > {
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
            WorkerEvent::Connected {
                profile,
                models,
                saved_profile,
            } => Ok((profile, models, saved_profile)),
            WorkerEvent::ProviderRejected { error, .. } => Err(error),
            _ => panic!("unexpected connection event"),
        }
    }

    fn connect(
        worker: &CoreWorker,
        profile: ProviderProfile,
        secret: Option<SecretValue>,
        persistence: PersistenceIntent,
    ) -> Result<Vec<linguamesh_domain::ModelDescriptor>, TranslationError> {
        connect_event(worker, profile, secret, persistence).map(|(_, models, _)| models)
    }

    fn select_event(
        worker: &CoreWorker,
        profile_id: &str,
        model_id: &str,
    ) -> Result<Option<ProviderProfile>, TranslationError> {
        worker
            .try_send(WorkerCommand::SelectModel {
                profile_id: ProviderProfileId::parse(profile_id).expect("profile ID"),
                model_id: model_id.to_owned(),
            })
            .expect("model command");
        match worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("model event")
        {
            WorkerEvent::ModelSelected {
                model_id: selected,
                saved_profile,
                ..
            } if selected == model_id => Ok(saved_profile),
            WorkerEvent::ModelSelectionRejected { error, .. } => Err(error),
            _ => panic!("unexpected model event"),
        }
    }

    fn select(worker: &CoreWorker, profile_id: &str, model_id: &str) {
        select_event(worker, profile_id, model_id).expect("model selection");
    }

    fn set_history_policy(worker: &CoreWorker, enabled: bool) {
        worker
            .try_send(WorkerCommand::SetTranslationHistoryEnabled { enabled })
            .expect("history policy command");
        match worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("history policy event")
        {
            WorkerEvent::TranslationHistoryPolicyUpdated { enabled: updated }
                if updated == enabled => {}
            WorkerEvent::TranslationHistoryPolicyRejected(error) => {
                panic!("history policy rejected: {error}")
            }
            _ => panic!("unexpected history policy event"),
        }
    }

    fn set_memory_policy(worker: &CoreWorker, enabled: bool) {
        worker
            .try_send(WorkerCommand::SetTranslationMemoryEnabled { enabled })
            .expect("memory policy command");
        match worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("memory policy event")
        {
            WorkerEvent::TranslationMemoryPolicyUpdated { enabled: updated }
                if updated == enabled => {}
            WorkerEvent::TranslationMemoryPolicyRejected(error) => {
                panic!("memory policy rejected: {error}")
            }
            _ => panic!("unexpected memory policy event"),
        }
    }

    fn delete_event(worker: &CoreWorker, profile_id: &str) -> Result<(), TranslationError> {
        worker
            .try_send(WorkerCommand::DeleteSavedProfile {
                profile_id: ProviderProfileId::parse(profile_id).expect("profile ID"),
            })
            .expect("delete profile command");
        match worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("delete profile event")
        {
            WorkerEvent::ProfileDeleted {
                profile_id: deleted,
            } if deleted.as_str() == profile_id => Ok(()),
            WorkerEvent::ProfileDeletionRejected { error, .. } => Err(error),
            _ => panic!("unexpected delete profile event"),
        }
    }

    fn expect_runtime_storage_unavailable(worker: &CoreWorker) {
        let event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("runtime storage event");
        assert!(matches!(
            event,
            WorkerEvent::ProfileStorageUnavailable(error)
                if error.kind == ErrorKind::Persistence
        ));
    }

    fn translate(worker: &CoreWorker, model_id: &str) -> (String, TranslationEvent) {
        translate_request(worker, TranslationRequest::new("Hello", "zh-CN", model_id))
    }

    fn translate_request(
        worker: &CoreWorker,
        request: TranslationRequest,
    ) -> (String, TranslationEvent) {
        worker
            .try_send(WorkerCommand::Translate(request))
            .expect("translation command");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = String::new();
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let timeout = deadline.duration_since(now).min(Duration::from_millis(500));
            let event = match worker.events.recv_timeout(timeout) {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("translation event channel disconnected")
                }
            };
            if let WorkerEvent::Translation(event) = event {
                match event {
                    TranslationEvent::TextDelta { text, .. } => output.push_str(&text),
                    event if event.is_terminal() => return (output, event),
                    _ => {}
                }
            }
        }
        panic!("translation did not terminate before the deadline");
    }

    #[test]
    fn completed_history_is_persisted_and_incognito_is_skipped() {
        let database = TestDatabase::new();
        let (worker, _, endpoint) = started_worker_with_database(database.path());
        connect(
            &worker,
            profile("history-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        select(&worker, "history-provider", "fake-translator");

        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));

        let (private_output, private_terminal) = translate_request(
            &worker,
            TranslationRequest::new("Private", "zh-CN", "fake-translator")
                .with_privacy_mode(TranslationPrivacyMode::Incognito),
        );
        assert_eq!(private_output, "你好，LinguaMesh！");
        assert!(matches!(
            private_terminal,
            TranslationEvent::Completed { .. }
        ));
        shutdown(&worker);

        let storage = Storage::open(database.path()).expect("history storage");
        assert_eq!(
            storage.translation_history_count().expect("history count"),
            1
        );
    }

    #[test]
    fn history_list_and_delete_commands_refresh_the_snapshot() {
        let database = TestDatabase::new();
        let (worker, _, endpoint) = started_worker_with_database(database.path());
        connect(
            &worker,
            profile("history-controls", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        select(&worker, "history-controls", "fake-translator");
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));

        worker
            .try_send(WorkerCommand::ListTranslationHistory)
            .expect("list history command");
        let entry = match worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("history list event")
        {
            WorkerEvent::TranslationHistoryListed { entries, count } => {
                assert_eq!(count, 1);
                entries.into_iter().next().expect("history entry")
            }
            _ => panic!("unexpected history event"),
        };
        assert_eq!(entry.source_text, "Hello");
        worker
            .try_send(WorkerCommand::DeleteTranslationHistory {
                operation_id: entry.operation_id,
            })
            .expect("delete history command");
        match worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("history deletion event")
        {
            WorkerEvent::TranslationHistoryListed { entries, count } => {
                assert!(entries.is_empty());
                assert_eq!(count, 0);
            }
            _ => panic!("unexpected deletion event"),
        }
        shutdown(&worker);
    }

    #[test]
    fn document_jobs_persist_segment_progress_and_restore_after_restart() {
        let database = TestDatabase::new();
        let worker = CoreWorker::spawn_with_database(database.path());
        loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("startup event")
            {
                WorkerEvent::DemoProviderReady { .. } => break,
                WorkerEvent::ProfileStorageUnavailable(error) => {
                    panic!("document storage unavailable: {error}");
                }
                _ => {}
            }
        }
        let job = DocumentJob::from_text("notes.txt", DocumentFormat::Txt, "one\ntwo");
        worker
            .try_send(WorkerCommand::CreateDocumentJob {
                job_id: "document-job-1".to_owned(),
                job,
            })
            .expect("create document job");
        let created = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("created event");
        assert!(
            matches!(created, WorkerEvent::DocumentJobUpdated(snapshot) if snapshot.state == DocumentJobState::Pending)
        );
        worker
            .try_send(WorkerCommand::UpdateDocumentSegment {
                job_id: "document-job-1".to_owned(),
                index: 0,
                translated_text: "一".to_owned(),
            })
            .expect("update document segment");
        let updated = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("updated event");
        assert!(
            matches!(updated, WorkerEvent::DocumentJobUpdated(snapshot) if snapshot.state == DocumentJobState::Running && snapshot.job.pending_count() == 1)
        );
        shutdown(&worker);

        let worker = CoreWorker::spawn_with_database(database.path());
        let restored = loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("restart startup event")
            {
                WorkerEvent::DocumentJobsRestored { jobs } => break jobs,
                WorkerEvent::ProfileStorageUnavailable(error) => {
                    panic!("document storage unavailable after restart: {error}");
                }
                _ => {}
            }
        };
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].job_id, "document-job-1");
        assert_eq!(restored[0].state, DocumentJobState::Running);
        assert_eq!(restored[0].job.pending_count(), 1);
        worker
            .try_send(WorkerCommand::UpdateDocumentSegment {
                job_id: "document-job-1".to_owned(),
                index: 1,
                translated_text: "二".to_owned(),
            })
            .expect("complete restored document job");
        let completed = loop {
            if let WorkerEvent::DocumentJobUpdated(snapshot) = worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("completion event")
            {
                break snapshot;
            }
        };
        assert_eq!(completed.state, DocumentJobState::Completed);
        assert_eq!(completed.job.reconstruct().expect("reconstruct"), "一\n二");
        shutdown(&worker);
    }

    #[test]
    fn document_job_translation_runs_each_pending_segment_and_reconstructs_output() {
        let database = TestDatabase::new();
        let (worker, _, endpoint) = started_worker_with_database(database.path());
        connect(
            &worker,
            profile("document-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        select(&worker, "document-provider", "fake-translator");
        worker
            .try_send(WorkerCommand::CreateDocumentJob {
                job_id: "document-translate-1".to_owned(),
                job: DocumentJob::from_text("notes.txt", DocumentFormat::Txt, "one\ntwo"),
            })
            .expect("create document job");
        assert!(matches!(
            worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("created event"),
            WorkerEvent::DocumentJobUpdated(snapshot)
                if snapshot.state == DocumentJobState::Pending
        ));
        worker
            .try_send(WorkerCommand::TranslateDocumentJob {
                job_id: "document-translate-1".to_owned(),
                source_locale: Some("en".to_owned()),
                target_locale: "zh-CN".to_owned(),
                glossary: None,
                privacy_mode: TranslationPrivacyMode::Standard,
            })
            .expect("translate document job");
        let completed = loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(10))
                .expect("document translation event")
            {
                WorkerEvent::DocumentJobUpdated(snapshot)
                    if snapshot.state == DocumentJobState::Completed =>
                {
                    break snapshot;
                }
                WorkerEvent::DocumentJobSegment { event, .. } => {
                    assert!(!matches!(event, TranslationEvent::Failed { .. }));
                }
                _ => {}
            }
        };
        assert_eq!(completed.job.pending_count(), 0);
        assert_eq!(
            completed.job.reconstruct().expect("reconstruct"),
            "你好，LinguaMesh！\n你好，LinguaMesh！"
        );
        shutdown(&worker);
    }

    #[test]
    fn document_job_translation_cancellation_persists_cancelled_state() {
        let database = TestDatabase::new();
        let (worker, _, endpoint) = started_worker_with_database(database.path());
        connect(
            &worker,
            profile("document-cancel-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        select(&worker, "document-cancel-provider", "fake-slow-translator");
        worker
            .try_send(WorkerCommand::CreateDocumentJob {
                job_id: "document-cancel-1".to_owned(),
                job: DocumentJob::from_text("notes.txt", DocumentFormat::Txt, "one"),
            })
            .expect("create document job");
        assert!(matches!(
            worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("created event"),
            WorkerEvent::DocumentJobUpdated(snapshot)
                if snapshot.state == DocumentJobState::Pending
        ));
        worker
            .try_send(WorkerCommand::TranslateDocumentJob {
                job_id: "document-cancel-1".to_owned(),
                source_locale: Some("en".to_owned()),
                target_locale: "zh-CN".to_owned(),
                glossary: None,
                privacy_mode: TranslationPrivacyMode::Standard,
            })
            .expect("translate document job");
        let mut cancel_sent = false;
        let cancelled = loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(10))
                .expect("document cancellation event")
            {
                WorkerEvent::DocumentJobSegment { event, .. } => {
                    if matches!(event, TranslationEvent::TextDelta { .. }) && !cancel_sent {
                        worker
                            .try_send(WorkerCommand::CancelDocumentJob {
                                job_id: "document-cancel-1".to_owned(),
                            })
                            .expect("cancel document job");
                        cancel_sent = true;
                    }
                    assert!(!matches!(event, TranslationEvent::Failed { .. }));
                }
                WorkerEvent::DocumentJobUpdated(snapshot)
                    if snapshot.state == DocumentJobState::Cancelled =>
                {
                    break snapshot;
                }
                _ => {}
            }
        };
        assert!(cancel_sent);
        assert!(cancelled.job.pending_count() > 0);
        shutdown(&worker);
    }

    #[test]
    fn document_job_translation_can_pause_resume_and_retry() {
        let database = TestDatabase::new();
        let (worker, _, endpoint) = started_worker_with_database(database.path());
        connect(
            &worker,
            profile("document-pause-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        select(&worker, "document-pause-provider", "fake-slow-translator");
        worker
            .try_send(WorkerCommand::CreateDocumentJob {
                job_id: "document-pause-1".to_owned(),
                job: DocumentJob::from_text("notes.txt", DocumentFormat::Txt, "one\ntwo"),
            })
            .expect("create document job");
        assert!(matches!(
            worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("created event"),
            WorkerEvent::DocumentJobUpdated(snapshot)
                if snapshot.state == DocumentJobState::Pending
        ));
        worker
            .try_send(WorkerCommand::TranslateDocumentJob {
                job_id: "document-pause-1".to_owned(),
                source_locale: Some("en".to_owned()),
                target_locale: "zh-CN".to_owned(),
                glossary: None,
                privacy_mode: TranslationPrivacyMode::Standard,
            })
            .expect("translate document job");
        let mut pause_sent = false;
        let paused = loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(10))
                .expect("document pause event")
            {
                WorkerEvent::DocumentJobSegment { event, .. } => {
                    if matches!(event, TranslationEvent::TextDelta { .. }) && !pause_sent {
                        worker
                            .try_send(WorkerCommand::PauseDocumentJob {
                                job_id: "document-pause-1".to_owned(),
                            })
                            .expect("pause document job");
                        pause_sent = true;
                    }
                }
                WorkerEvent::DocumentJobUpdated(snapshot)
                    if snapshot.state == DocumentJobState::Paused =>
                {
                    break snapshot;
                }
                _ => {}
            }
        };
        assert!(pause_sent);
        assert!(paused.job.pending_count() > 0);
        worker
            .try_send(WorkerCommand::ResumeDocumentJob {
                job_id: "document-pause-1".to_owned(),
            })
            .expect("resume document job");
        let completed = loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(10))
                .expect("document resume event")
            {
                WorkerEvent::DocumentJobUpdated(snapshot)
                    if snapshot.state == DocumentJobState::Completed =>
                {
                    break snapshot;
                }
                _ => {}
            }
        };
        assert_eq!(completed.job.pending_count(), 0);
        shutdown(&worker);
    }

    #[test]
    fn document_job_resume_reuses_saved_options_after_worker_restart() {
        let database = TestDatabase::new();
        let (worker, _, endpoint) = started_worker_with_database(database.path());
        connect(
            &worker,
            profile("document-restart-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        select(&worker, "document-restart-provider", "fake-slow-translator");
        worker
            .try_send(WorkerCommand::CreateDocumentJob {
                job_id: "document-restart-1".to_owned(),
                job: DocumentJob::from_text("notes.txt", DocumentFormat::Txt, "one\ntwo"),
            })
            .expect("create document job");
        assert!(matches!(
            worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("created event"),
            WorkerEvent::DocumentJobUpdated(snapshot)
                if snapshot.state == DocumentJobState::Pending
        ));
        worker
            .try_send(WorkerCommand::TranslateDocumentJob {
                job_id: "document-restart-1".to_owned(),
                source_locale: Some("en".to_owned()),
                target_locale: "zh-CN".to_owned(),
                glossary: None,
                privacy_mode: TranslationPrivacyMode::Standard,
            })
            .expect("translate document job");
        let mut pause_sent = false;
        loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(10))
                .expect("document pause event")
            {
                WorkerEvent::DocumentJobSegment { event, .. }
                    if matches!(event, TranslationEvent::TextDelta { .. }) && !pause_sent =>
                {
                    worker
                        .try_send(WorkerCommand::PauseDocumentJob {
                            job_id: "document-restart-1".to_owned(),
                        })
                        .expect("pause document job");
                    pause_sent = true;
                }
                WorkerEvent::DocumentJobUpdated(snapshot)
                    if snapshot.state == DocumentJobState::Paused =>
                {
                    assert!(pause_sent);
                    break;
                }
                _ => {}
            }
        }
        shutdown(&worker);

        let (worker, _, endpoint) = started_worker_with_database(database.path());
        connect(
            &worker,
            profile("document-restart-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("reconnect");
        select(&worker, "document-restart-provider", "fake-slow-translator");
        worker
            .try_send(WorkerCommand::ResumeDocumentJob {
                job_id: "document-restart-1".to_owned(),
            })
            .expect("resume restored document job");
        loop {
            match worker
                .events
                .recv_timeout(Duration::from_secs(10))
                .expect("document resume event")
            {
                WorkerEvent::DocumentJobUpdated(snapshot)
                    if snapshot.state == DocumentJobState::Completed =>
                {
                    assert_eq!(snapshot.job.pending_count(), 0);
                    break;
                }
                _ => {}
            }
        }
        shutdown(&worker);
    }

    #[test]
    fn history_policy_persists_and_preserves_existing_entries() {
        let database = TestDatabase::new();
        let (worker, _, endpoint) = started_worker_with_database(database.path());
        connect(
            &worker,
            profile("history-policy", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        select(&worker, "history-policy", "fake-translator");
        let _ = translate(&worker, "fake-translator");
        set_history_policy(&worker, false);
        let _ = translate(&worker, "fake-translator");
        set_history_policy(&worker, true);
        let _ = translate(&worker, "fake-translator");
        shutdown(&worker);

        let storage = Storage::open(database.path()).expect("history storage");
        assert!(storage.translation_history_enabled().expect("policy"));
        assert_eq!(storage.translation_history_count().expect("count"), 2);
    }

    #[test]
    fn translation_memory_controls_persist_and_reuse_completed_results() {
        let database = TestDatabase::new();
        let (worker, _, endpoint) = started_worker_with_database(database.path());
        connect(
            &worker,
            profile("memory-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("connection");
        select(&worker, "memory-provider", "fake-translator");
        let (first_output, first_terminal) = translate(&worker, "fake-translator");
        assert_eq!(first_output, "你好，LinguaMesh！");
        assert!(matches!(first_terminal, TranslationEvent::Completed { .. }));
        worker
            .try_send(WorkerCommand::ListTranslationMemory)
            .expect("list memory");
        let entry = match worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("memory list event")
        {
            WorkerEvent::TranslationMemoryListed { entries, count } => {
                assert_eq!(count, 1);
                entries.into_iter().next().expect("memory entry")
            }
            _ => panic!("unexpected memory list event"),
        };
        set_memory_policy(&worker, false);
        let (second_output, second_terminal) = translate(&worker, "fake-translator");
        assert_eq!(second_output, "你好，LinguaMesh！");
        assert!(matches!(
            second_terminal,
            TranslationEvent::Completed { .. }
        ));
        worker
            .try_send(WorkerCommand::DeleteTranslationMemory {
                cache_key: entry.cache_key,
            })
            .expect("delete memory");
        match worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("memory deletion event")
        {
            WorkerEvent::TranslationMemoryListed { entries, count } => {
                assert!(entries.is_empty());
                assert_eq!(count, 0);
            }
            _ => panic!("unexpected memory deletion event"),
        }
        worker
            .try_send(WorkerCommand::ClearTranslationMemory)
            .expect("clear memory");
        assert!(matches!(
            worker
                .events
                .recv_timeout(Duration::from_secs(5))
                .expect("memory clear event"),
            WorkerEvent::TranslationMemoryCleared
        ));
        shutdown(&worker);
        let storage = Storage::open(database.path()).expect("memory storage");
        assert!(!storage.translation_memory_enabled().expect("policy"));
        assert_eq!(storage.translation_memory_count().expect("count"), 0);
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

        select(&worker, "explicit-provider", "fake-translator");
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
    }

    #[test]
    fn authenticated_session_secret_is_consumed_once_and_old_session_survives_failure() {
        let external = ExternalFakeProvider::start(FakeMode::Authenticated("fake-session-secret"));
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
        select(&worker, "authenticated-provider", "fake-translator");

        let error = connect(&worker, authenticated, None, PersistenceIntent::SessionOnly)
            .expect_err("missing one-shot secret");
        assert_eq!(error.kind, ErrorKind::SecretUnavailable);

        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
    }

    #[test]
    fn unavailable_secret_backends_fail_closed() {
        let external =
            ExternalFakeProvider::start(FakeMode::Authenticated("persistent-secret-test-canary"));
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

        let database = TestDatabase::new();
        let (secure_worker, _, _) = started_worker_with_database(database.path());
        let persistent_ref = profile(
            "persistent-secret",
            &external.endpoint,
            Some(SecretRef::new(SecretRefNamespace::SecretService)),
            None,
        );
        let error = connect(
            &secure_worker,
            persistent_ref.clone(),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect_err("secure storage unavailable");
        assert!(matches!(
            error.kind,
            ErrorKind::SecretUnavailable | ErrorKind::SecureStorageUnavailable
        ));

        let error = connect(
            &secure_worker,
            persistent_ref,
            None,
            PersistenceIntent::Persistent,
        )
        .expect_err("persistent secret takes precedence");
        assert!(matches!(
            error.kind,
            ErrorKind::SecretUnavailable | ErrorKind::SecureStorageUnavailable
        ));
        shutdown(&secure_worker);

        let persistent_profile = profile("persistent-profile", &external.endpoint, None, None);
        let error = connect(
            &worker,
            persistent_profile,
            None,
            PersistenceIntent::Persistent,
        )
        .expect_err("profile persistence unavailable");
        assert_eq!(error.kind, ErrorKind::Persistence);
        shutdown(&worker);
    }

    #[cfg(feature = "gui")]
    #[ignore = "requires the persistent Secret Service onboarding fixture"]
    #[test]
    fn persistent_secret_onboarding_connects_without_credential_reentry() {
        const SECRET_REF: &str = "secret-service:22222222-2222-4222-8222-222222222222";
        const SECRET_CANARY: &str = "PERSISTENT_ONBOARDING_SECRET_CANARY";
        let external = ExternalFakeProvider::start(FakeMode::Authenticated(SECRET_CANARY));
        let secret_ref = SecretRef::parse(SECRET_REF).expect("persistent onboarding reference");
        crate::secret_service::store_secret(&secret_ref, &SecretValue::new(SECRET_CANARY))
            .expect("store onboarding credential");
        let database = TestDatabase::new();
        let (worker, restored, _) = started_worker_with_database(database.path());
        assert!(restored.is_none());

        let profile = profile(
            "persistent-onboarding",
            &external.endpoint,
            Some(secret_ref.clone()),
            None,
        );
        let (connected, models, saved_profile) =
            connect_event(&worker, profile, None, PersistenceIntent::Persistent)
                .expect("connect through Secret Service");
        assert_eq!(connected.secret_ref(), Some(&secret_ref));
        assert!(models.iter().any(|model| model.id == "fake-translator"));
        assert!(saved_profile.is_some_and(|saved| { saved.secret_ref() == Some(&secret_ref) }));
        select_event(&worker, "persistent-onboarding", "fake-translator")
            .expect("select onboarding model");
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        database.assert_private_permissions();
        database.assert_absent_from_files(&[SECRET_CANARY]);
        shutdown(&worker);

        let (restarted, restored, _) = started_worker_with_database(database.path());
        let restored = restored.expect("restored persistent profile");
        assert_eq!(restored.secret_ref(), Some(&secret_ref));
        let (connected, models, _) =
            connect_event(&restarted, restored, None, PersistenceIntent::Persistent)
                .expect("reconnect through restored Secret Service reference");
        assert_eq!(connected.secret_ref(), Some(&secret_ref));
        assert!(models.iter().any(|model| model.id == "fake-translator"));
        let (output, terminal) = translate(&restarted, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        crate::secret_service::delete_secret(&secret_ref).expect("delete onboarding credential");
        shutdown(&restarted);
        database.assert_absent_from_files(&[SECRET_CANARY]);
    }

    #[test]
    fn profile_persistence_preserves_only_persistent_secret_references() {
        let persistent = profile(
            "persistent-ref",
            "http://127.0.0.1:1/v1/",
            Some(SecretRef::new(SecretRefNamespace::SecretService)),
            None,
        );
        let saved = profile_without_secret(&persistent).expect("persistent profile");
        assert!(saved.secret_ref().is_some_and(SecretRef::is_persistent));

        let session = profile(
            "session-ref",
            "http://127.0.0.1:1/v1/",
            Some(SecretRef::new(SecretRefNamespace::Session)),
            None,
        );
        let saved = profile_without_secret(&session).expect("session profile");
        assert!(saved.secret_ref().is_none());
    }

    #[test]
    fn loopback_openai_compatible_provider_translates_without_secret() {
        let external = ExternalFakeProvider::start(FakeMode::Standard);
        let (worker, _) = started_worker();
        let runtime = profile("loopback-provider", &external.endpoint, None, None);
        let models = connect(&worker, runtime, None, PersistenceIntent::SessionOnly)
            .expect("loopback provider connection");
        assert!(models.iter().any(|model| model.id == "fake-translator"));
        select_event(&worker, "loopback-provider", "fake-translator")
            .expect("select loopback model");
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert_eq!(external.chat_requests.load(Ordering::SeqCst), 1);
        shutdown(&worker);
    }

    #[test]
    fn loopback_ollama_compatible_provider_translates_without_secret() {
        let external = ExternalFakeProvider::start(FakeMode::OllamaCompatible);
        let (worker, _) = started_worker();
        let runtime = profile_with_preset(
            "ollama-loopback",
            "local-loopback",
            &external.endpoint,
            None,
            None,
        );
        let models = connect(&worker, runtime, None, PersistenceIntent::SessionOnly)
            .expect("ollama-compatible provider connection");
        assert!(models.iter().any(|model| model.id == "llama3.2:latest"));
        select_event(&worker, "ollama-loopback", "llama3.2:latest").expect("select Ollama model");
        let (output, terminal) = translate(&worker, "llama3.2:latest");
        assert_eq!(output, "你好，Ollama！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert_eq!(external.chat_requests.load(Ordering::SeqCst), 1);
        shutdown(&worker);
    }

    #[test]
    fn native_ollama_provider_translates_without_secret() {
        let external = ExternalFakeProvider::start(FakeMode::OllamaNative);
        let (worker, _) = started_worker();
        let runtime =
            profile_with_preset("ollama-native", "ollama", &external.endpoint, None, None);
        let models = connect(&worker, runtime, None, PersistenceIntent::SessionOnly)
            .expect("native Ollama provider connection");
        assert!(models.iter().any(|model| model.id == "llama3.2:latest"));
        select_event(&worker, "ollama-native", "llama3.2:latest")
            .expect("select native Ollama model");
        let (output, terminal) = translate(&worker, "llama3.2:latest");
        assert_eq!(output, "你好，Ollama！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert_eq!(external.chat_requests.load(Ordering::SeqCst), 1);
        shutdown(&worker);
    }

    #[test]
    fn remembered_session_profile_and_model_restore_without_secret() {
        const SECRET_CANARY: &str = "PERSISTENCE_SECRET_CANARY";
        let external = ExternalFakeProvider::start(FakeMode::Authenticated(SECRET_CANARY));
        let database = TestDatabase::new();
        let (worker, restored, _) = started_worker_with_database(database.path());
        assert!(restored.is_none());

        let session_ref = SecretRef::new(SecretRefNamespace::Session);
        let session_ref_canary = session_ref.as_str().to_owned();
        let runtime = profile(
            "restart-provider",
            &external.endpoint,
            Some(session_ref),
            None,
        );
        let (connected, models, saved_profile) = connect_event(
            &worker,
            runtime,
            Some(SecretValue::new(SECRET_CANARY)),
            PersistenceIntent::Persistent,
        )
        .expect("persistent connection");
        assert_eq!(
            connected.secret_ref().map(SecretRef::namespace),
            Some("session")
        );
        assert!(models.iter().any(|model| model.id == "fake-translator"));
        let saved_profile = saved_profile.expect("saved non-secret profile");
        assert!(saved_profile.secret_ref().is_none());
        let saved_profile = select_event(&worker, "restart-provider", "fake-translator")
            .expect("saved model selection")
            .expect("updated saved profile");
        assert_eq!(saved_profile.selected_model(), Some("fake-translator"));
        assert!(saved_profile.secret_ref().is_none());
        database.assert_absent_from_files(&[
            SECRET_CANARY,
            "session:",
            session_ref_canary.as_str(),
        ]);
        shutdown(&worker);

        database.assert_private_permissions();
        database.assert_absent_from_files(&[
            SECRET_CANARY,
            "session:",
            session_ref_canary.as_str(),
        ]);

        let requests_before_restart = external.model_requests.load(Ordering::SeqCst);
        let (restarted, restored, _) = started_worker_with_database(database.path());
        assert_eq!(
            external.model_requests.load(Ordering::SeqCst),
            requests_before_restart
        );
        let restored = restored.expect("restored profile");
        assert_eq!(restored.id().as_str(), "restart-provider");
        assert_eq!(restored.selected_model(), Some("fake-translator"));
        assert!(restored.secret_ref().is_none());

        let runtime = runtime_profile_with_session_secret(&restored);
        let (connected, _, saved_profile) = connect_event(
            &restarted,
            runtime,
            Some(SecretValue::new(SECRET_CANARY)),
            PersistenceIntent::Persistent,
        )
        .expect("reconnected saved profile");
        assert_eq!(connected.selected_model(), Some("fake-translator"));
        assert!(saved_profile.is_some_and(|profile| profile.secret_ref().is_none()));
        let (output, terminal) = translate(&restarted, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        shutdown(&restarted);

        database.assert_absent_from_files(&[
            SECRET_CANARY,
            "session:",
            session_ref_canary.as_str(),
        ]);
    }

    // 单一重启流程覆盖多配置更新、默认选择和删除后的会话连续性。
    #[allow(clippy::too_many_lines)]
    #[test]
    fn multiple_profiles_restore_without_requests_and_delete_preserves_live_session() {
        let first = ExternalFakeProvider::start(FakeMode::Standard);
        let second = ExternalFakeProvider::start(FakeMode::Standard);
        let database = TestDatabase::new();
        let (worker, profiles, active_profile_id, _) =
            started_worker_with_database_snapshot(database.path());
        assert!(profiles.is_empty());
        assert!(active_profile_id.is_none());

        connect(
            &worker,
            profile("profile-a", &first.endpoint, None, None),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("first persistent connection");
        select_event(&worker, "profile-a", "fake-translator")
            .expect("first model selection")
            .expect("first saved profile update");
        connect(
            &worker,
            profile("profile-b", &second.endpoint, None, None),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("second persistent connection");
        select_event(&worker, "profile-b", "fake-slow-translator")
            .expect("second model selection")
            .expect("second saved profile update");
        let updated_a = ProviderProfile::new(
            ProviderProfileId::parse("profile-a").expect("profile ID"),
            "Updated profile A",
            "custom-openai-compatible",
            "openai_chat_completions",
            &first.endpoint,
            None,
        )
        .expect("updated first profile")
        .with_selected_model(Some("fake-translator".to_owned()))
        .expect("updated first model");
        connect(&worker, updated_a, None, PersistenceIntent::Persistent)
            .expect("independent first profile update");
        connect(
            &worker,
            profile(
                "profile-b",
                &second.endpoint,
                None,
                Some("fake-slow-translator"),
            ),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("restore second profile as active");
        shutdown(&worker);

        let first_requests = first.model_requests.load(Ordering::SeqCst);
        let second_requests = second.model_requests.load(Ordering::SeqCst);
        let (restarted, profiles, active_profile_id, _) =
            started_worker_with_database_snapshot(database.path());
        assert_eq!(first.model_requests.load(Ordering::SeqCst), first_requests);
        assert_eq!(
            second.model_requests.load(Ordering::SeqCst),
            second_requests
        );
        assert_eq!(profiles.len(), 2);
        assert_eq!(
            active_profile_id.as_ref().map(ProviderProfileId::as_str),
            Some("profile-b")
        );
        let restored_a = profiles
            .iter()
            .find(|profile| profile.id().as_str() == "profile-a")
            .expect("restored first profile");
        let restored_b = profiles
            .iter()
            .find(|profile| profile.id().as_str() == "profile-b")
            .expect("restored second profile");
        assert_eq!(restored_a.display_name(), "Updated profile A");
        assert_eq!(restored_a.selected_model(), Some("fake-translator"));
        assert_eq!(restored_b.selected_model(), Some("fake-slow-translator"));

        delete_event(&restarted, "profile-a").expect("delete inactive profile");
        let error = delete_event(&restarted, "missing-profile").expect_err("missing profile");
        assert_eq!(error.kind, ErrorKind::InvalidConfiguration);
        let restored_b = restored_b.clone();
        let (_, _, saved_profile) =
            connect_event(&restarted, restored_b, None, PersistenceIntent::Persistent)
                .expect("reconnect active saved profile");
        assert!(saved_profile.is_some());
        delete_event(&restarted, "profile-b").expect("delete connected profile");

        let (output, terminal) = translate(&restarted, "fake-slow-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        let updated = select_event(&restarted, "profile-b", "fake-translator")
            .expect("session-only model selection after deletion");
        assert!(updated.is_none());
        shutdown(&restarted);

        let (final_worker, profiles, active_profile_id, _) =
            started_worker_with_database_snapshot(database.path());
        assert!(profiles.is_empty());
        assert!(active_profile_id.is_none());
        shutdown(&final_worker);
    }

    #[test]
    fn multiple_session_credentials_stay_isolated_and_never_reach_storage() {
        const FIRST_SECRET: &str = "FIRST_PROFILE_SECRET_CANARY";
        const SECOND_SECRET: &str = "SECOND_PROFILE_SECRET_CANARY";
        let first = ExternalFakeProvider::start(FakeMode::Authenticated(FIRST_SECRET));
        let second = ExternalFakeProvider::start(FakeMode::Authenticated(SECOND_SECRET));
        let database = TestDatabase::new();
        let (worker, profiles, active_profile_id, _) =
            started_worker_with_database_snapshot(database.path());
        assert!(profiles.is_empty());
        assert!(active_profile_id.is_none());

        let first_ref = SecretRef::new(SecretRefNamespace::Session);
        let first_ref_text = first_ref.as_str().to_owned();
        connect(
            &worker,
            profile("secret-profile-a", &first.endpoint, Some(first_ref), None),
            Some(SecretValue::new(FIRST_SECRET)),
            PersistenceIntent::Persistent,
        )
        .expect("first authenticated persistence");
        select_event(&worker, "secret-profile-a", "fake-translator")
            .expect("first saved model")
            .expect("first saved profile");

        let second_ref = SecretRef::new(SecretRefNamespace::Session);
        let second_ref_text = second_ref.as_str().to_owned();
        connect(
            &worker,
            profile("secret-profile-b", &second.endpoint, Some(second_ref), None),
            Some(SecretValue::new(SECOND_SECRET)),
            PersistenceIntent::Persistent,
        )
        .expect("second authenticated persistence");
        select_event(&worker, "secret-profile-b", "fake-slow-translator")
            .expect("second saved model")
            .expect("second saved profile");
        database.assert_absent_from_files(&[
            FIRST_SECRET,
            SECOND_SECRET,
            "session:",
            first_ref_text.as_str(),
            second_ref_text.as_str(),
        ]);
        shutdown(&worker);

        let (restarted, profiles, active_profile_id, _) =
            started_worker_with_database_snapshot(database.path());
        assert_eq!(profiles.len(), 2);
        assert_eq!(
            active_profile_id.as_ref().map(ProviderProfileId::as_str),
            Some("secret-profile-b")
        );
        let saved_a = profiles
            .iter()
            .find(|profile| profile.id().as_str() == "secret-profile-a")
            .expect("saved first profile");
        assert!(saved_a.secret_ref().is_none());
        let error = connect(
            &restarted,
            runtime_profile_with_session_secret(saved_a),
            None,
            PersistenceIntent::Persistent,
        )
        .expect_err("credential re-entry required");
        assert_eq!(error.kind, ErrorKind::SecretUnavailable);

        let saved_b = profiles
            .iter()
            .find(|profile| profile.id().as_str() == "secret-profile-b")
            .expect("saved second profile");
        let error = connect(
            &restarted,
            runtime_profile_with_session_secret(saved_b),
            Some(SecretValue::new(FIRST_SECRET)),
            PersistenceIntent::Persistent,
        )
        .expect_err("credentials are not copied between profiles");
        assert_eq!(error.kind, ErrorKind::Authentication);
        assert!(!error.message.contains(FIRST_SECRET));
        connect(
            &restarted,
            runtime_profile_with_session_secret(saved_b),
            Some(SecretValue::new(SECOND_SECRET)),
            PersistenceIntent::Persistent,
        )
        .expect("second credential re-entry");
        shutdown(&restarted);

        database.assert_absent_from_files(&[
            FIRST_SECRET,
            SECOND_SECRET,
            "session:",
            first_ref_text.as_str(),
            second_ref_text.as_str(),
        ]);
    }

    // 场景五在单一时序中验证认证提供商切换和失败回滚。
    #[allow(clippy::too_many_lines)]
    #[test]
    fn scenario_five_routes_translations_only_to_confirmed_authenticated_provider() {
        const FIRST_SECRET: &str = "SCENARIO_FIVE_FIRST_SECRET";
        const SECOND_SECRET: &str = "SCENARIO_FIVE_SECOND_SECRET";
        let first = ExternalFakeProvider::start(FakeMode::Authenticated(FIRST_SECRET));
        let second = ExternalFakeProvider::start(FakeMode::Authenticated(SECOND_SECRET));
        let database = TestDatabase::new();
        let (worker, profiles, active_profile_id, _) =
            started_worker_with_database_snapshot(database.path());
        assert!(profiles.is_empty());
        assert!(active_profile_id.is_none());

        let first_ref = SecretRef::new(SecretRefNamespace::Session);
        let first_ref_text = first_ref.as_str().to_owned();
        connect(
            &worker,
            profile("scenario-five-a", &first.endpoint, Some(first_ref), None),
            Some(SecretValue::new(FIRST_SECRET)),
            PersistenceIntent::Persistent,
        )
        .expect("first authenticated connection");
        select_event(&worker, "scenario-five-a", "fake-translator")
            .expect("first model selection")
            .expect("first saved model");
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert_chat_requests(&first, 1, &second, 0);

        let second_ref = SecretRef::new(SecretRefNamespace::Session);
        let second_ref_text = second_ref.as_str().to_owned();
        connect(
            &worker,
            profile("scenario-five-b", &second.endpoint, Some(second_ref), None),
            Some(SecretValue::new(SECOND_SECRET)),
            PersistenceIntent::Persistent,
        )
        .expect("second authenticated connection");
        select_event(&worker, "scenario-five-b", "fake-slow-translator")
            .expect("second model selection")
            .expect("second saved model");
        let (output, terminal) = translate(&worker, "fake-slow-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert_chat_requests(&first, 1, &second, 1);

        let first_reconnect_ref = SecretRef::new(SecretRefNamespace::Session);
        let first_reconnect_ref_text = first_reconnect_ref.as_str().to_owned();
        connect(
            &worker,
            profile(
                "scenario-five-a",
                &first.endpoint,
                Some(first_reconnect_ref),
                Some("fake-translator"),
            ),
            Some(SecretValue::new(FIRST_SECRET)),
            PersistenceIntent::Persistent,
        )
        .expect("single reconnect to first provider");
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert_chat_requests(&first, 1, &second, 1);

        let second_reconnect_ref = SecretRef::new(SecretRefNamespace::Session);
        let second_reconnect_ref_text = second_reconnect_ref.as_str().to_owned();
        connect(
            &worker,
            profile(
                "scenario-five-b",
                &second.endpoint,
                Some(second_reconnect_ref),
                Some("fake-slow-translator"),
            ),
            Some(SecretValue::new(SECOND_SECRET)),
            PersistenceIntent::Persistent,
        )
        .expect("single reconnect to second provider");
        let (output, terminal) = translate(&worker, "fake-slow-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert_chat_requests(&first, 1, &second, 1);

        let candidate_ref = SecretRef::new(SecretRefNamespace::Session);
        let candidate_ref_text = candidate_ref.as_str().to_owned();
        let error = connect(
            &worker,
            profile(
                "scenario-five-b",
                &second.endpoint,
                Some(candidate_ref),
                Some("fake-slow-translator"),
            ),
            Some(SecretValue::new(FIRST_SECRET)),
            PersistenceIntent::Persistent,
        )
        .expect_err("candidate authentication rejection");
        assert_eq!(error.kind, ErrorKind::Authentication);

        let (output, terminal) = translate(&worker, "fake-slow-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert_chat_requests(&first, 1, &second, 1);
        let forbidden_storage_values = [
            FIRST_SECRET,
            SECOND_SECRET,
            "session:",
            first_ref_text.as_str(),
            second_ref_text.as_str(),
            first_reconnect_ref_text.as_str(),
            second_reconnect_ref_text.as_str(),
            candidate_ref_text.as_str(),
        ];
        database.assert_absent_from_files(&forbidden_storage_values);
        shutdown(&worker);

        let (restarted, profiles, active_profile_id, _) =
            started_worker_with_database_snapshot(database.path());
        assert_eq!(profiles.len(), 2);
        assert_eq!(
            active_profile_id.as_ref().map(ProviderProfileId::as_str),
            Some("scenario-five-b")
        );
        let restored_a = profiles
            .iter()
            .find(|profile| profile.id().as_str() == "scenario-five-a")
            .expect("restored first profile");
        let restored_b = profiles
            .iter()
            .find(|profile| profile.id().as_str() == "scenario-five-b")
            .expect("restored second profile");
        assert_eq!(restored_a.selected_model(), Some("fake-translator"));
        assert_eq!(restored_b.selected_model(), Some("fake-slow-translator"));
        shutdown(&restarted);
        database.assert_absent_from_files(&forbidden_storage_values);
    }

    #[test]
    fn approved_fallback_retries_only_after_retryable_primary_failure() {
        let primary = ExternalFakeProvider::start(FakeMode::Standard);
        let fallback = ExternalFakeProvider::start(FakeMode::Standard);
        let database = TestDatabase::new();
        let (worker, _, _) = started_worker_with_database(database.path());
        connect(
            &worker,
            profile("approved-fallback", &fallback.endpoint, None, None),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("fallback profile connection");
        select(&worker, "approved-fallback", "fake-translator");
        connect(
            &worker,
            profile("primary-network", &primary.endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("primary session connection");
        select(&worker, "primary-network", "fake-translator");
        drop(primary);

        worker
            .try_send(WorkerCommand::TranslateWithFallback {
                request: TranslationRequest::new("Hello", "zh-CN", "fake-translator"),
                fallback_profile_id: ProviderProfileId::parse("approved-fallback")
                    .expect("fallback profile ID"),
            })
            .expect("fallback translation command");
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut output = String::new();
        let mut fallback_selected = false;
        let terminal = loop {
            assert!(Instant::now() < deadline, "fallback translation timed out");
            let event = match worker.events.recv_timeout(Duration::from_millis(500)) {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("fallback translation event channel disconnected")
                }
            };
            match event {
                WorkerEvent::FallbackSelected {
                    primary_profile_id,
                    fallback_profile_id,
                } => {
                    assert_eq!(primary_profile_id.as_str(), "primary-network");
                    assert_eq!(fallback_profile_id.as_str(), "approved-fallback");
                    fallback_selected = true;
                }
                WorkerEvent::Translation(TranslationEvent::TextDelta { text, .. }) => {
                    output.push_str(&text);
                }
                WorkerEvent::Translation(event) if event.is_terminal() => break event,
                WorkerEvent::Translation(_)
                | WorkerEvent::TranslationHistoryUpdated { .. }
                | WorkerEvent::TranslationHistoryPersistenceFailed(_) => {}
                _ => panic!("unexpected fallback event"),
            }
        };
        assert!(fallback_selected);
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert_eq!(fallback.chat_requests.load(Ordering::SeqCst), 1);
        shutdown(&worker);
    }

    #[test]
    fn session_switch_does_not_replace_saved_restart_profile() {
        let first = ExternalFakeProvider::start(FakeMode::Standard);
        let second = ExternalFakeProvider::start(FakeMode::Standard);
        let database = TestDatabase::new();
        let (worker, restored, _) = started_worker_with_database(database.path());
        assert!(restored.is_none());

        connect(
            &worker,
            profile("switch-provider", &first.endpoint, None, None),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("saved connection");
        select_event(&worker, "switch-provider", "fake-translator")
            .expect("saved model selection")
            .expect("saved profile update");

        connect(
            &worker,
            profile("switch-provider", &second.endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("session switch");
        shutdown(&worker);

        let (restarted, restored, _) = started_worker_with_database(database.path());
        let restored = restored.expect("saved restart profile");
        assert_eq!(restored.base_endpoint(), first.endpoint);
        assert_ne!(restored.base_endpoint(), second.endpoint);
        assert_eq!(restored.selected_model(), Some("fake-translator"));
        shutdown(&restarted);
    }

    #[test]
    fn rejected_persistent_changes_preserve_saved_profile_and_model() {
        let database = TestDatabase::new();
        let (worker, restored, endpoint) = started_worker_with_database(database.path());
        assert!(restored.is_none());
        connect(
            &worker,
            profile("stable-provider", &endpoint, None, None),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("saved connection");
        select_event(&worker, "stable-provider", "fake-translator")
            .expect("saved model selection")
            .expect("saved profile update");

        let error = select_event(&worker, "stale-provider", "fake-slow-translator")
            .expect_err("stale model selection");
        assert_eq!(error.kind, ErrorKind::InvalidConfiguration);
        let error = connect(
            &worker,
            profile(
                "unavailable-persistent-provider",
                "http://127.0.0.1:1/v1/",
                None,
                None,
            ),
            None,
            PersistenceIntent::Persistent,
        )
        .expect_err("failed persistent switch");
        assert_eq!(error.kind, ErrorKind::Network);
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        shutdown(&worker);

        let (restarted, restored, _) = started_worker_with_database(database.path());
        let restored = restored.expect("stable saved profile");
        assert_eq!(restored.id().as_str(), "stable-provider");
        assert_eq!(restored.selected_model(), Some("fake-translator"));
        shutdown(&restarted);
    }

    #[test]
    #[ignore = "requires a private storage-fault mount namespace"]
    fn runtime_storage_write_failures_degrade_to_session_mode_without_false_commits() {
        assert!(
            matches!(
                std::env::var("LINGUAMESH_RUNTIME_STORAGE_FAULT_TEST"),
                Ok(value) if value == "1"
            ),
            "the runtime storage fault test must use its dedicated namespace runner"
        );
        let baseline_server = ExternalFakeProvider::start(FakeMode::Standard);
        let rejected_server = ExternalFakeProvider::start(FakeMode::Standard);
        let mut fault_mount = RuntimeFaultMount::new();
        let database_path = fault_mount.database_path();

        let (worker, restored, _) = started_worker_with_database(&database_path);
        assert!(restored.is_none());
        connect(
            &worker,
            profile("baseline-provider", &baseline_server.endpoint, None, None),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("baseline persistent connection");
        select_event(&worker, "baseline-provider", "fake-translator")
            .expect("baseline model selection")
            .expect("baseline saved model");

        fault_mount.exhaust_space();
        let error = select_event(&worker, "baseline-provider", "fake-slow-translator")
            .expect_err("full-filesystem model update");
        assert_eq!(error.kind, ErrorKind::Persistence);
        expect_runtime_storage_unavailable(&worker);
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert!(
            select_event(&worker, "baseline-provider", "fake-slow-translator")
                .expect("session-only model selection after storage failure")
                .is_none()
        );
        let (output, terminal) = translate(&worker, "fake-slow-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        shutdown(&worker);
        fault_mount.clear_fault();

        let (worker, profiles, active_profile_id, _) =
            started_worker_with_database_snapshot(&database_path);
        assert_eq!(profiles.len(), 1);
        let baseline_profile = profiles[0].clone();
        assert_eq!(baseline_profile.id().as_str(), "baseline-provider");
        assert_eq!(baseline_profile.selected_model(), Some("fake-translator"));
        assert_eq!(active_profile_id.as_ref(), Some(baseline_profile.id()));
        connect(
            &worker,
            baseline_profile.clone(),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("reconnect baseline before delete fault");

        fault_mount.exhaust_space();
        let error =
            delete_event(&worker, "baseline-provider").expect_err("full-filesystem deletion");
        assert_eq!(error.kind, ErrorKind::Persistence);
        expect_runtime_storage_unavailable(&worker);
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        shutdown(&worker);
        fault_mount.clear_fault();

        let (worker, profiles, active_profile_id, _) =
            started_worker_with_database_snapshot(&database_path);
        assert_eq!(profiles, vec![baseline_profile.clone()]);
        assert_eq!(active_profile_id.as_ref(), Some(baseline_profile.id()));
        connect(
            &worker,
            baseline_profile.clone(),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("reconnect baseline before provider fault");

        fault_mount.exhaust_space();
        let error = connect(
            &worker,
            profile("rejected-provider", &rejected_server.endpoint, None, None),
            None,
            PersistenceIntent::Persistent,
        )
        .expect_err("full-filesystem provider persistence");
        assert_eq!(error.kind, ErrorKind::Persistence);
        expect_runtime_storage_unavailable(&worker);
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        assert_chat_requests(&baseline_server, 4, &rejected_server, 0);
        assert_eq!(rejected_server.model_requests.load(Ordering::SeqCst), 1);
        shutdown(&worker);
        fault_mount.clear_fault();

        let (worker, profiles, active_profile_id, _) =
            started_worker_with_database_snapshot(&database_path);
        assert_eq!(profiles, vec![baseline_profile.clone()]);
        assert_eq!(profiles[0].selected_model(), Some("fake-translator"));
        assert_eq!(active_profile_id.as_ref(), Some(baseline_profile.id()));
        shutdown(&worker);
        fault_mount.finish();
    }

    #[test]
    fn public_cancel_preserves_the_confirmed_restart_profile() {
        let baseline = ExternalFakeProvider::start(FakeMode::Standard);
        let delayed = ExternalFakeProvider::start(FakeMode::Delayed(Duration::from_secs(2)));
        let database = TestDatabase::new();
        let (worker, restored, _) = started_worker_with_database(database.path());
        assert!(restored.is_none());

        connect(
            &worker,
            profile("baseline-provider", &baseline.endpoint, None, None),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("baseline connection");
        select_event(&worker, "baseline-provider", "fake-translator")
            .expect("baseline model selection")
            .expect("baseline saved profile");

        worker
            .try_send(WorkerCommand::Connect {
                profile: profile("cancelled-provider", &delayed.endpoint, None, None),
                secret: None,
                persistence: PersistenceIntent::Persistent,
            })
            .expect("candidate connection");
        let deadline = Instant::now() + Duration::from_secs(5);
        while delayed.model_requests.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(delayed.model_requests.load(Ordering::SeqCst), 1);
        worker.try_send(WorkerCommand::Cancel).expect("cancel");
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(1)),
            Ok(WorkerEvent::ProviderRejected { error, .. })
                if error.kind == ErrorKind::Cancelled
        ));

        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        shutdown(&worker);

        let (restarted, restored, _) = started_worker_with_database(database.path());
        let restored = restored.expect("confirmed restart profile");
        assert_eq!(restored.id().as_str(), "baseline-provider");
        assert_eq!(restored.base_endpoint(), baseline.endpoint);
        assert_eq!(restored.selected_model(), Some("fake-translator"));
        shutdown(&restarted);
    }

    #[test]
    fn pre_cancelled_connection_is_never_persisted() {
        let database = TestDatabase::new();
        let (worker, restored, endpoint) = started_worker_with_database(database.path());
        assert!(restored.is_none());
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        worker
            .commands
            .try_send(QueuedCommand::Connect {
                profile: profile("cancelled-provider", &endpoint, None, None),
                secret: None,
                persistence: PersistenceIntent::Persistent,
                cancellation,
            })
            .expect("pre-cancelled connection");
        let event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("cancelled connection event");
        assert!(matches!(
            event,
            WorkerEvent::ProviderRejected { error, .. }
                if error.kind == ErrorKind::Cancelled
        ));
        shutdown(&worker);

        let (restarted, restored, _) = started_worker_with_database(database.path());
        assert!(restored.is_none());
        shutdown(&restarted);
    }

    #[test]
    fn unavailable_database_reports_error_but_session_mode_still_works() {
        let worker = CoreWorker::spawn_with_database("relative.sqlite3");
        let storage_event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("storage event");
        assert!(matches!(
            storage_event,
            WorkerEvent::ProfileStorageUnavailable(error)
                if error.kind == ErrorKind::Persistence
        ));
        let ready_event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("demo provider event");
        let WorkerEvent::DemoProviderReady { endpoint } = ready_event else {
            panic!("expected demo provider readiness");
        };

        connect(
            &worker,
            profile("fallback-provider", &endpoint, None, None),
            None,
            PersistenceIntent::SessionOnly,
        )
        .expect("session fallback connection");
        select(&worker, "fallback-provider", "fake-translator");
        let (output, terminal) = translate(&worker, "fake-translator");
        assert_eq!(output, "你好，LinguaMesh！");
        assert!(matches!(terminal, TranslationEvent::Completed { .. }));
        let error = delete_event(&worker, "fallback-provider")
            .expect_err("delete requires profile storage");
        assert_eq!(error.kind, ErrorKind::Persistence);
        shutdown(&worker);
    }

    #[test]
    fn active_translation_rejects_saved_profile_deletion() {
        let database = TestDatabase::new();
        let (worker, restored, endpoint) = started_worker_with_database(database.path());
        assert!(restored.is_none());
        connect(
            &worker,
            profile("busy-profile", &endpoint, None, None),
            None,
            PersistenceIntent::Persistent,
        )
        .expect("persistent connection");
        select_event(&worker, "busy-profile", "fake-slow-translator")
            .expect("saved slow model")
            .expect("saved profile update");
        worker
            .try_send(WorkerCommand::Translate(TranslationRequest::new(
                "Hello",
                "zh-CN",
                "fake-slow-translator",
            )))
            .expect("translation command");
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(5)),
            Ok(WorkerEvent::Translation(TranslationEvent::Started { .. }))
        ));
        worker
            .try_send(WorkerCommand::DeleteSavedProfile {
                profile_id: ProviderProfileId::parse("busy-profile").expect("profile ID"),
            })
            .expect("delete command");

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut deletion_rejected = false;
        let mut translation_completed = false;
        while Instant::now() < deadline && !(deletion_rejected && translation_completed) {
            let event = match worker.events.recv_timeout(Duration::from_millis(500)) {
                Ok(event) => event,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    panic!("worker disconnected during busy operation")
                }
            };
            match event {
                WorkerEvent::ProfileDeletionRejected { error, .. } => {
                    assert_eq!(error.kind, ErrorKind::InvalidConfiguration);
                    deletion_rejected = true;
                }
                WorkerEvent::Translation(event) if event.is_terminal() => {
                    assert!(matches!(event, TranslationEvent::Completed { .. }));
                    translation_completed = true;
                }
                WorkerEvent::Translation(_)
                | WorkerEvent::TranslationHistoryUpdated { .. }
                | WorkerEvent::TranslationHistoryPersistenceFailed(_) => {}
                _ => panic!("unexpected busy operation event"),
            }
        }
        assert!(deletion_rejected);
        assert!(translation_completed);
        delete_event(&worker, "busy-profile").expect("delete after translation");
        shutdown(&worker);
    }

    #[test]
    fn permissive_database_directory_is_rejected_before_file_creation() {
        let database = TestDatabase::new();
        fs::create_dir(&database.directory).expect("database directory");
        fs::set_permissions(&database.directory, fs::Permissions::from_mode(0o755))
            .expect("database directory permissions");

        let worker = CoreWorker::spawn_with_database(database.path());
        let storage_event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("storage event");
        assert!(matches!(
            storage_event,
            WorkerEvent::ProfileStorageUnavailable(error)
                if error.kind == ErrorKind::Persistence
        ));
        assert!(!database.path().exists());
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(5)),
            Ok(WorkerEvent::DemoProviderReady { .. })
        ));
        shutdown(&worker);
    }

    #[test]
    fn symbolic_ancestor_is_rejected_without_creating_the_database() {
        let database = TestDatabase::new();
        fs::create_dir(&database.directory).expect("database directory");
        fs::set_permissions(&database.directory, fs::Permissions::from_mode(0o700))
            .expect("database directory permissions");
        let target = database.directory.join("target");
        fs::create_dir(&target).expect("symbolic-link target");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700))
            .expect("symbolic-link target permissions");
        let linked_parent = database.directory.join("linked-parent");
        symlink(&target, &linked_parent).expect("symbolic parent");
        let linked_database = linked_parent.join("state.sqlite3");

        let worker = CoreWorker::spawn_with_database(&linked_database);
        let storage_event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("storage event");
        assert!(matches!(
            storage_event,
            WorkerEvent::ProfileStorageUnavailable(error)
                if error.kind == ErrorKind::Persistence
        ));
        assert!(!target.join("state.sqlite3").exists());
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(5)),
            Ok(WorkerEvent::DemoProviderReady { .. })
        ));
        shutdown(&worker);
    }

    #[test]
    fn hard_link_database_is_rejected_without_modifying_its_target() {
        let database = TestDatabase::new();
        fs::create_dir(&database.directory).expect("database directory");
        fs::set_permissions(&database.directory, fs::Permissions::from_mode(0o700))
            .expect("database directory permissions");
        let target = database.directory.join("target.sqlite3");
        let original = b"NOT_A_DATABASE";
        fs::write(&target, original).expect("database target");
        fs::hard_link(&target, database.path()).expect("database hard link");

        let worker = CoreWorker::spawn_with_database(database.path());
        let storage_event = worker
            .events
            .recv_timeout(Duration::from_secs(5))
            .expect("storage event");
        assert!(matches!(
            storage_event,
            WorkerEvent::ProfileStorageUnavailable(error)
                if error.kind == ErrorKind::Persistence
        ));
        assert_eq!(fs::read(&target).expect("database target"), original);
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(5)),
            Ok(WorkerEvent::DemoProviderReady { .. })
        ));
        shutdown(&worker);
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

        worker
            .commands
            .try_send(QueuedCommand::DeleteSavedProfile {
                profile_id: ProviderProfileId::parse("full-queue-provider").expect("profile ID"),
            })
            .expect("queued delete command");
        for index in 0..(COMMAND_CAPACITY - 1) {
            worker
                .commands
                .try_send(QueuedCommand::SelectModel {
                    profile_id: ProviderProfileId::parse("full-queue-provider")
                        .expect("profile ID"),
                    model_id: format!("queued-model-{index}"),
                })
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
        let rejected = worker
            .events
            .recv_timeout(Duration::from_secs(1))
            .expect("queued delete cancellation");
        assert!(matches!(
            rejected,
            WorkerEvent::ProfileDeletionRejected { error, .. }
                if error.kind == ErrorKind::Cancelled
        ));
        for _ in 0..(COMMAND_CAPACITY - 1) {
            let rejected = worker
                .events
                .recv_timeout(Duration::from_secs(1))
                .expect("queued command cancellation");
            assert!(matches!(
                rejected,
                WorkerEvent::ModelSelectionRejected { error, .. }
                    if error.kind == ErrorKind::Cancelled
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
        select(&worker, "working-provider", "fake-translator");

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
        select(&worker, "slow-provider", "fake-slow-translator");
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
        select(
            &worker,
            "shutdown-translation-provider",
            "fake-slow-translator",
        );
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
