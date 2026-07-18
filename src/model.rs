use crate::localization;
use linguamesh_domain::{
    ErrorKind, ModelDescriptor, TranslationError, TranslationEvent, TranslationRequest,
};
pub use linguamesh_domain::{ProviderProfile, ProviderProfileId};
use linguamesh_protocol::PROTOCOL_VERSION;
use std::collections::HashSet;
use std::error::Error;
use std::fmt;

/// 当前检查点提供的本地提供商标识。
pub const LOCAL_FAKE_PROVIDER_ID: &str = "local-fake-provider";

/// 描述原生客户端的可见操作状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppStatus {
    /// 尚未连接任何提供商。
    Disconnected,
    /// 正在连接共享核心提供商。
    Connecting,
    /// 已准备接受翻译请求。
    Ready,
    /// 正在接收流式结果。
    Translating,
    /// 已请求取消并等待核心终止事件。
    Cancelling,
    /// 翻译成功完成。
    Completed,
    /// 翻译已取消且可能保留部分输出。
    Cancelled,
    /// 最近的操作失败。
    Failed,
}

impl AppStatus {
    /// 返回稳定的英文状态标签。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting",
            Self::Ready => "Ready",
            Self::Translating => "Translating",
            Self::Cancelling => "Cancelling",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
            Self::Failed => "Failed",
        }
    }

    const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

/// 描述首次提供商配置流程的派生阶段。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OnboardingStage {
    /// 工作线程仍在完成启动检查。
    Starting,
    /// 工作线程不可用，需要重新启动应用。
    Unavailable,
    /// 用户需要选择或填写提供商配置。
    ConfigureProvider,
    /// 提供商连接和模型发现正在进行。
    Connecting,
    /// 提供商已连接但尚未确认模型。
    SelectModel,
    /// 提供商与模型均已准备好。
    Ready,
}

impl OnboardingStage {
    /// 返回不包含用户配置内容的稳定英文标签。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Starting => "Starting",
            Self::Unavailable => "Unavailable",
            Self::ConfigureProvider => "Configure provider",
            Self::Connecting => "Connecting",
            Self::SelectModel => "Select model",
            Self::Ready => "Ready",
        }
    }
}

/// 描述客户端外观偏好。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThemePreference {
    /// 跟随桌面外观。
    #[default]
    System,
    /// 强制浅色外观。
    Light,
    /// 强制深色外观。
    Dark,
}

impl ThemePreference {
    /// 返回英文设置标签。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

/// 描述当前界面区域设置。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UiLocale {
    /// 使用规范英文回退。
    #[default]
    English,
    /// 记录简体中文界面偏好。
    SimplifiedChinese,
    /// 使用繁体中文目录。
    TraditionalChinese,
    /// 使用西班牙语目录。
    Spanish,
    /// 使用法语目录。
    French,
    /// 使用德语目录。
    German,
    /// 使用日语目录。
    Japanese,
    /// 使用韩语目录。
    Korean,
    /// 使用巴西葡萄牙语目录。
    BrazilianPortuguese,
    /// 使用俄语目录。
    Russian,
    /// 使用阿拉伯语目录。
    Arabic,
    /// 使用印地语目录。
    Hindi,
}

impl UiLocale {
    /// 返回原生界面下拉框使用的稳定顺序。
    pub const ALL: [Self; 12] = [
        Self::English,
        Self::SimplifiedChinese,
        Self::TraditionalChinese,
        Self::Spanish,
        Self::French,
        Self::German,
        Self::Japanese,
        Self::Korean,
        Self::BrazilianPortuguese,
        Self::Russian,
        Self::Arabic,
        Self::Hindi,
    ];

    /// 返回界面中显示的名称。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "Simplified Chinese",
            Self::TraditionalChinese => "Traditional Chinese",
            Self::Spanish => "Spanish",
            Self::French => "French",
            Self::German => "German",
            Self::Japanese => "Japanese",
            Self::Korean => "Korean",
            Self::BrazilianPortuguese => "Portuguese (Brazil)",
            Self::Russian => "Russian",
            Self::Arabic => "Arabic",
            Self::Hindi => "Hindi",
        }
    }

    /// 返回稳定的语言标签。
    #[must_use]
    pub const fn language_tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
            Self::TraditionalChinese => "zh-Hant",
            Self::Spanish => "es",
            Self::French => "fr",
            Self::German => "de",
            Self::Japanese => "ja",
            Self::Korean => "ko",
            Self::BrazilianPortuguese => "pt-BR",
            Self::Russian => "ru",
            Self::Arabic => "ar",
            Self::Hindi => "hi",
        }
    }

    /// 将界面下拉框索引转换为受支持的区域设置。
    #[must_use]
    pub const fn from_index(index: usize) -> Self {
        if index < Self::ALL.len() {
            Self::ALL[index]
        } else {
            Self::English
        }
    }

    /// 返回是否需要从右向左的文字方向。
    #[must_use]
    pub const fn is_rtl(self) -> bool {
        matches!(self, Self::Arabic)
    }
}

/// 表示本地状态转换拒绝的操作。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateError {
    /// 请求未提供源文本。
    MissingSource,
    /// 请求未选择模型。
    MissingModel,
    /// 模型不在当前发现结果中。
    UnknownModel(String),
    /// 上一次模型选择仍在等待工作线程确认。
    ModelSelectionPending,
    /// 模型确认结果不属于当前待确认选择。
    UnexpectedModelSelection,
    /// 提供商配置当前不可用。
    InvalidProfile,
    /// 尚未连接可用的提供商。
    MissingProvider,
    /// 连接结果不属于当前待提交的配置。
    UnexpectedProviderConnection,
    /// 上一次翻译仍在运行。
    Busy,
    /// 提供商连接尚未完成。
    Connecting,
    /// 已保存配置与当前运行时配置或保存意图不一致。
    InvalidSavedProfile,
    /// 本地配置存储当前不可用。
    ProfileStorageUnavailable,
    /// 另一个删除操作仍在等待工作线程确认。
    ProfileDeletionPending,
    /// 删除结果不属于当前等待确认的配置。
    UnexpectedProfileDeletion,
    /// 首个核心事件不是开始事件。
    UnexpectedFirstEvent,
    /// 核心流在首个事件之后重复发出开始事件。
    UnexpectedStartedEvent,
    /// 核心事件序号没有严格递增。
    NonIncreasingSequence,
    /// 终止事件之后又收到事件。
    EventAfterTerminal,
}

impl fmt::Display for StateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSource => formatter.write_str("Enter source text before translating."),
            Self::MissingModel => formatter.write_str("Select a model before translating."),
            Self::UnknownModel(model) => write!(formatter, "Model is not available: {model}"),
            Self::ModelSelectionPending => {
                formatter.write_str("A model selection is still being confirmed.")
            }
            Self::UnexpectedModelSelection => {
                formatter.write_str("The model selection result is stale or unexpected.")
            }
            Self::InvalidProfile => formatter.write_str("The provider profile is disabled."),
            Self::MissingProvider => formatter.write_str("Connect a provider before translating."),
            Self::UnexpectedProviderConnection => {
                formatter.write_str("The provider connection result is stale or unexpected.")
            }
            Self::Busy => formatter.write_str("A translation is already running."),
            Self::Connecting => formatter.write_str("A provider connection is still in progress."),
            Self::InvalidSavedProfile => {
                formatter.write_str("The saved provider profile does not match this operation.")
            }
            Self::ProfileStorageUnavailable => {
                formatter.write_str("Saved profile storage is unavailable.")
            }
            Self::ProfileDeletionPending => {
                formatter.write_str("A saved profile deletion is still being confirmed.")
            }
            Self::UnexpectedProfileDeletion => {
                formatter.write_str("The saved profile deletion result is stale or unexpected.")
            }
            Self::UnexpectedFirstEvent => {
                formatter.write_str("The core stream did not begin with a started event.")
            }
            Self::UnexpectedStartedEvent => {
                formatter.write_str("The core stream produced more than one started event.")
            }
            Self::NonIncreasingSequence => {
                formatter.write_str("The core stream produced an out-of-order event.")
            }
            Self::EventAfterTerminal => {
                formatter.write_str("The core stream produced an event after termination.")
            }
        }
    }
}

impl Error for StateError {}

#[derive(Clone, Copy, Eq, PartialEq)]
enum WorkerStartup {
    Pending,
    Ready,
    Failed,
}

/// 描述本地配置存储的启动结果。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProfileStorageStatus {
    /// 工作线程尚未报告存储结果。
    #[default]
    Pending,
    /// 本地配置存储已打开并可接受持久化操作。
    Available,
    /// 本地配置存储不可用，仅允许会话操作。
    Unavailable,
}

impl ProfileStorageStatus {
    /// 返回不包含配置内容的英文诊断标签。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Available => "Available",
            Self::Unavailable => "Unavailable",
        }
    }
}

/// 保存与工具包无关的原生界面状态。
#[derive(Clone)]
pub struct AppState {
    worker_startup: WorkerStartup,
    active_provider: Option<ProviderProfile>,
    pending_provider: Option<ProviderProfile>,
    saved_profiles: Vec<ProviderProfile>,
    selected_saved_profile_id: Option<ProviderProfileId>,
    persisted_active_profile_id: Option<ProviderProfileId>,
    active_saved_profile_id: Option<ProviderProfileId>,
    profile_storage_status: ProfileStorageStatus,
    pending_profile_deletion: Option<ProviderProfileId>,
    pending_provider_will_be_saved: bool,
    models: Vec<ModelDescriptor>,
    selected_model: Option<String>,
    pending_model_selection: Option<String>,
    source_text: String,
    source_locale: Option<String>,
    target_locale: String,
    output: String,
    partial_output: bool,
    status: AppStatus,
    error: Option<TranslationError>,
    theme: ThemePreference,
    locale: UiLocale,
    last_sequence: Option<u64>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            worker_startup: WorkerStartup::Pending,
            active_provider: None,
            pending_provider: None,
            saved_profiles: Vec::new(),
            selected_saved_profile_id: None,
            persisted_active_profile_id: None,
            active_saved_profile_id: None,
            profile_storage_status: ProfileStorageStatus::Pending,
            pending_profile_deletion: None,
            pending_provider_will_be_saved: false,
            models: Vec::new(),
            selected_model: None,
            pending_model_selection: None,
            source_text: String::new(),
            source_locale: None,
            target_locale: "zh-CN".to_owned(),
            output: String::new(),
            partial_output: false,
            status: AppStatus::Disconnected,
            error: None,
            theme: ThemePreference::System,
            locale: UiLocale::English,
            last_sequence: None,
        }
    }
}

impl AppState {
    /// 指示工作线程已完成存储检查和本地服务启动。
    #[must_use]
    pub const fn worker_ready(&self) -> bool {
        matches!(self.worker_startup, WorkerStartup::Ready)
    }

    /// 指示工作线程已停止或启动失败。
    #[must_use]
    pub const fn worker_unavailable(&self) -> bool {
        matches!(self.worker_startup, WorkerStartup::Failed)
    }

    /// 标记工作线程已准备接受用户命令。
    pub const fn mark_worker_ready(&mut self) {
        self.worker_startup = WorkerStartup::Ready;
    }

    /// 标记工作线程不可再接受用户命令。
    pub const fn mark_worker_unavailable(&mut self) {
        self.worker_startup = WorkerStartup::Failed;
    }

    /// 返回当前状态。
    #[must_use]
    pub const fn status(&self) -> AppStatus {
        self.status
    }

    /// 仅依据当前权威状态推导提供商引导阶段。
    #[must_use]
    pub const fn onboarding_stage(&self) -> OnboardingStage {
        if self.worker_unavailable() {
            OnboardingStage::Unavailable
        } else if !self.worker_ready() {
            OnboardingStage::Starting
        } else if matches!(self.status, AppStatus::Connecting) {
            OnboardingStage::Connecting
        } else if self.active_provider.is_none() {
            OnboardingStage::ConfigureProvider
        } else if self.pending_model_selection.is_some() || self.selected_model.is_none() {
            OnboardingStage::SelectModel
        } else {
            OnboardingStage::Ready
        }
    }

    /// 返回当前提供商标识。
    #[must_use]
    pub fn provider_id(&self) -> Option<&ProviderProfileId> {
        self.active_provider.as_ref().map(ProviderProfile::id)
    }

    /// 返回当前活动提供商。
    #[must_use]
    pub const fn active_provider(&self) -> Option<&ProviderProfile> {
        self.active_provider.as_ref()
    }

    /// 返回正在连接且尚未提交的提供商。
    #[must_use]
    pub const fn pending_provider(&self) -> Option<&ProviderProfile> {
        self.pending_provider.as_ref()
    }

    /// 返回按显示名称和稳定标识排序的全部已保存配置。
    #[must_use]
    pub fn saved_profiles(&self) -> &[ProviderProfile] {
        &self.saved_profiles
    }

    /// 返回当前在配置表单中选择的已保存配置标识。
    #[must_use]
    pub const fn selected_saved_profile_id(&self) -> Option<&ProviderProfileId> {
        self.selected_saved_profile_id.as_ref()
    }

    /// 返回当前在配置表单中选择的已保存配置。
    #[must_use]
    pub fn selected_saved_profile(&self) -> Option<&ProviderProfile> {
        self.selected_saved_profile_id
            .as_ref()
            .and_then(|profile_id| saved_profile_by_id(&self.saved_profiles, profile_id))
    }

    /// 返回兼容单配置调用方的当前表单配置。
    #[must_use]
    pub fn saved_profile(&self) -> Option<&ProviderProfile> {
        self.selected_saved_profile()
    }

    /// 返回下次启动默认显示的已保存配置标识。
    #[must_use]
    pub const fn persisted_active_profile_id(&self) -> Option<&ProviderProfileId> {
        self.persisted_active_profile_id.as_ref()
    }

    /// 返回当前运行时连接对应的已保存配置标识。
    #[must_use]
    pub const fn active_saved_profile_id(&self) -> Option<&ProviderProfileId> {
        self.active_saved_profile_id.as_ref()
    }

    /// 返回本地配置存储的启动结果。
    #[must_use]
    pub const fn profile_storage_status(&self) -> ProfileStorageStatus {
        self.profile_storage_status
    }

    /// 指示本地配置存储是否可接受持久化操作。
    #[must_use]
    pub const fn profile_storage_available(&self) -> bool {
        matches!(self.profile_storage_status, ProfileStorageStatus::Available)
    }

    /// 返回正在等待工作线程确认删除的配置标识。
    #[must_use]
    pub const fn pending_profile_deletion(&self) -> Option<&ProviderProfileId> {
        self.pending_profile_deletion.as_ref()
    }

    /// 指示当前待连接配置是否要求原子保存。
    #[must_use]
    pub const fn pending_provider_will_be_saved(&self) -> bool {
        self.pending_provider_will_be_saved
    }

    /// 指示当前活动配置是否与已保存配置对应。
    #[must_use]
    pub const fn active_provider_is_saved(&self) -> bool {
        self.active_saved_profile_id.is_some()
    }

    /// 返回已发现模型。
    #[must_use]
    pub fn models(&self) -> &[ModelDescriptor] {
        &self.models
    }

    /// 返回当前模型标识。
    #[must_use]
    pub fn selected_model(&self) -> Option<&str> {
        self.selected_model.as_deref()
    }

    /// 返回正在等待工作线程确认的模型标识。
    #[must_use]
    pub fn pending_model_selection(&self) -> Option<&str> {
        self.pending_model_selection.as_deref()
    }

    /// 返回当前源文本。
    #[must_use]
    pub fn source_text(&self) -> &str {
        &self.source_text
    }

    /// 返回当前目标语言标签。
    #[must_use]
    pub fn target_locale(&self) -> &str {
        &self.target_locale
    }

    /// 返回当前流式输出。
    #[must_use]
    pub fn output(&self) -> &str {
        &self.output
    }

    /// 指示输出是否为未完成结果。
    #[must_use]
    pub const fn has_partial_output(&self) -> bool {
        self.partial_output
    }

    /// 返回当前外观偏好。
    #[must_use]
    pub const fn theme(&self) -> ThemePreference {
        self.theme
    }

    /// 返回当前界面区域设置。
    #[must_use]
    pub const fn locale(&self) -> UiLocale {
        self.locale
    }

    /// 原子恢复全部非会话配置和持久化活动选择，不建立网络连接。
    pub fn restore_saved_profiles(
        &mut self,
        mut saved_profiles: Vec<ProviderProfile>,
        persisted_active_profile_id: Option<ProviderProfileId>,
    ) -> Result<(), StateError> {
        if self.status != AppStatus::Disconnected
            || self.active_provider.is_some()
            || self.pending_provider.is_some()
            || self.pending_profile_deletion.is_some()
            || saved_profiles.iter().any(|profile| {
                profile
                    .secret_ref()
                    .is_some_and(|secret_ref| !secret_ref.is_persistent())
            })
        {
            return Err(StateError::InvalidSavedProfile);
        }
        sort_saved_profiles(&mut saved_profiles);
        let mut profile_ids = HashSet::with_capacity(saved_profiles.len());
        if saved_profiles
            .iter()
            .any(|profile| !profile_ids.insert(profile.id().as_str()))
        {
            return Err(StateError::InvalidSavedProfile);
        }
        if persisted_active_profile_id
            .as_ref()
            .is_some_and(|profile_id| {
                saved_profile_by_id(&saved_profiles, profile_id)
                    .is_none_or(|profile| !profile.enabled())
            })
        {
            return Err(StateError::InvalidSavedProfile);
        }

        self.saved_profiles = saved_profiles;
        self.selected_saved_profile_id
            .clone_from(&persisted_active_profile_id);
        self.persisted_active_profile_id = persisted_active_profile_id;
        self.active_saved_profile_id = None;
        self.profile_storage_status = ProfileStorageStatus::Available;
        self.error = None;
        Ok(())
    }

    /// 仅恢复一个非会话配置，不建立连接或选择模型。
    pub fn restore_saved_profile(
        &mut self,
        saved_profile: ProviderProfile,
    ) -> Result<(), StateError> {
        let profile_id = saved_profile.id().clone();
        self.restore_saved_profiles(vec![saved_profile], Some(profile_id))
    }

    /// 记录配置存储不可用并保留已经建立的会话连接。
    pub fn profile_storage_unavailable(&mut self, error: TranslationError) {
        self.saved_profiles.clear();
        self.selected_saved_profile_id = None;
        self.persisted_active_profile_id = None;
        self.active_saved_profile_id = None;
        self.pending_profile_deletion = None;
        self.profile_storage_status = ProfileStorageStatus::Unavailable;
        self.provider_failed(error);
    }

    /// 仅更改配置表单选择，不连接或激活提供商。
    pub fn select_saved_profile(
        &mut self,
        profile_id: Option<&ProviderProfileId>,
    ) -> Result<(), StateError> {
        if self.pending_profile_deletion.is_some() {
            return Err(StateError::ProfileDeletionPending);
        }
        if self.pending_model_selection.is_some() {
            return Err(StateError::ModelSelectionPending);
        }
        if self.status == AppStatus::Connecting {
            return Err(StateError::Connecting);
        }
        if matches!(self.status, AppStatus::Translating | AppStatus::Cancelling) {
            return Err(StateError::Busy);
        }
        if profile_id.is_some_and(|profile_id| {
            saved_profile_by_id(&self.saved_profiles, profile_id).is_none()
        }) {
            return Err(StateError::InvalidSavedProfile);
        }
        self.selected_saved_profile_id = profile_id.cloned();
        Ok(())
    }

    /// 开始连接一个已验证的规范提供商配置。
    pub fn begin_provider_connection(
        &mut self,
        profile: ProviderProfile,
    ) -> Result<(), StateError> {
        self.begin_provider_connection_with_persistence(profile, false)
    }

    /// 开始连接并记录成功后是否应提交非秘密配置。
    pub fn begin_provider_connection_with_persistence(
        &mut self,
        profile: ProviderProfile,
        remember_profile: bool,
    ) -> Result<(), StateError> {
        if !profile.enabled() {
            return Err(StateError::InvalidProfile);
        }
        if matches!(self.status, AppStatus::Translating | AppStatus::Cancelling) {
            return Err(StateError::Busy);
        }
        if self.pending_profile_deletion.is_some() {
            return Err(StateError::ProfileDeletionPending);
        }
        if self.pending_model_selection.is_some() {
            return Err(StateError::ModelSelectionPending);
        }
        if self.status == AppStatus::Connecting {
            return Err(StateError::Connecting);
        }
        if remember_profile && !self.profile_storage_available() {
            return Err(StateError::ProfileStorageUnavailable);
        }
        self.pending_provider = Some(profile);
        self.pending_provider_will_be_saved = remember_profile;
        self.status = AppStatus::Connecting;
        self.error = None;
        Ok(())
    }

    /// 仅在发现成功且结果匹配时原子提交提供商和模型。
    pub fn provider_connected(
        &mut self,
        profile: ProviderProfile,
        models: Vec<ModelDescriptor>,
    ) -> Result<(), StateError> {
        self.provider_connected_with_saved_profile(profile, models, None)
    }

    /// 仅在连接结果与保存结果同时匹配时原子提交状态。
    pub fn provider_connected_with_saved_profile(
        &mut self,
        profile: ProviderProfile,
        models: Vec<ModelDescriptor>,
        saved_profile: Option<ProviderProfile>,
    ) -> Result<(), StateError> {
        let Some(pending_profile) = self.pending_provider.as_ref() else {
            return Err(StateError::UnexpectedProviderConnection);
        };
        let expected_profile = pending_profile
            .clone()
            .with_selected_model(
                pending_profile
                    .selected_model()
                    .filter(|selected| models.iter().any(|model| model.id == *selected))
                    .map(str::to_owned),
            )
            .map_err(|_| StateError::UnexpectedProviderConnection)?;
        if profile != expected_profile {
            return Err(StateError::UnexpectedProviderConnection);
        }
        if self.pending_provider_will_be_saved != saved_profile.is_some()
            || saved_profile.as_ref().is_some_and(|saved| {
                saved
                    .secret_ref()
                    .is_some_and(|secret_ref| !secret_ref.is_persistent())
                    || !profiles_match_except_secret(&profile, saved)
            })
        {
            return Err(StateError::InvalidSavedProfile);
        }

        let selected_model = profile.selected_model().map(str::to_owned);
        let saved_profile_id = saved_profile
            .as_ref()
            .map(|saved_profile| saved_profile.id().clone());
        let mut saved_profiles = self.saved_profiles.clone();
        if let Some(saved_profile) = saved_profile {
            upsert_saved_profile(&mut saved_profiles, saved_profile);
        }

        self.active_provider = Some(profile);
        self.pending_provider = None;
        self.pending_provider_will_be_saved = false;
        self.active_saved_profile_id.clone_from(&saved_profile_id);
        if let Some(saved_profile_id) = saved_profile_id {
            self.saved_profiles = saved_profiles;
            self.selected_saved_profile_id = Some(saved_profile_id.clone());
            self.persisted_active_profile_id = Some(saved_profile_id);
        }
        self.models = models;
        self.selected_model = selected_model;
        self.pending_model_selection = None;
        self.status = AppStatus::Ready;
        self.error = None;
        Ok(())
    }

    /// 仅当失败结果匹配当前待连接配置时才回滚连接。
    pub fn provider_connection_failed(
        &mut self,
        profile: &ProviderProfile,
        error: TranslationError,
    ) -> Result<(), StateError> {
        if self.pending_provider.as_ref() != Some(profile) {
            return Err(StateError::UnexpectedProviderConnection);
        }
        self.provider_failed(error);
        Ok(())
    }

    /// 记录连接失败并保留上一个可用配置。
    pub fn provider_failed(&mut self, error: TranslationError) {
        self.pending_provider = None;
        self.pending_provider_will_be_saved = false;
        if !matches!(self.status, AppStatus::Translating | AppStatus::Cancelling) {
            self.status = if self.active_provider.is_some() {
                AppStatus::Ready
            } else {
                AppStatus::Failed
            };
        }
        self.error = Some(error);
    }

    /// 开始等待工作线程确认一个已发现模型。
    pub fn begin_model_selection(&mut self, model_id: &str) -> Result<(), StateError> {
        if matches!(self.status, AppStatus::Connecting) {
            return Err(StateError::Connecting);
        }
        if matches!(self.status, AppStatus::Translating | AppStatus::Cancelling) {
            return Err(StateError::Busy);
        }
        if self.pending_profile_deletion.is_some() {
            return Err(StateError::ProfileDeletionPending);
        }
        if self.active_provider.is_none() {
            return Err(StateError::MissingProvider);
        }
        if !self.models.iter().any(|model| model.id == model_id) {
            return Err(StateError::UnknownModel(model_id.to_owned()));
        }
        self.pending_model_selection = Some(model_id.to_owned());
        self.error = None;
        Ok(())
    }

    /// 仅提交当前等待确认的模型选择。
    pub fn confirm_model_selection(&mut self, model_id: &str) -> Result<(), StateError> {
        let profile_id = self
            .active_provider
            .as_ref()
            .map(|profile| profile.id().clone())
            .ok_or(StateError::MissingProvider)?;
        let saved_profile = self
            .active_saved_profile_id
            .as_ref()
            .map(|saved_profile_id| {
                if saved_profile_id != &profile_id {
                    return Err(StateError::InvalidSavedProfile);
                }
                saved_profile_by_id(&self.saved_profiles, saved_profile_id)
                    .cloned()
                    .ok_or(StateError::InvalidSavedProfile)?
                    .with_selected_model(Some(model_id.to_owned()))
                    .map_err(|_| StateError::InvalidSavedProfile)
            })
            .transpose()?;
        self.confirm_model_selection_with_saved_profile(&profile_id, model_id, saved_profile)
    }

    /// 仅提交与活动配置、待确认模型和保存结果完全匹配的选择。
    pub fn confirm_model_selection_with_saved_profile(
        &mut self,
        profile_id: &ProviderProfileId,
        model_id: &str,
        saved_profile: Option<ProviderProfile>,
    ) -> Result<(), StateError> {
        let Some(active_provider) = self.active_provider.as_ref() else {
            return Err(StateError::MissingProvider);
        };
        if active_provider.id() != profile_id
            || self.pending_model_selection.as_deref() != Some(model_id)
        {
            return Err(StateError::UnexpectedModelSelection);
        }
        if !self.models.iter().any(|model| model.id == model_id) {
            return Err(StateError::UnknownModel(model_id.to_owned()));
        }
        if self.active_saved_profile_id.is_some() != saved_profile.is_some() {
            return Err(StateError::InvalidSavedProfile);
        }
        if self
            .active_saved_profile_id
            .as_ref()
            .is_some_and(|saved_id| {
                saved_id != profile_id
                    || self.persisted_active_profile_id.as_ref() != Some(profile_id)
            })
        {
            return Err(StateError::InvalidSavedProfile);
        }
        let updated_active = active_provider
            .clone()
            .with_selected_model(Some(model_id.to_owned()))
            .map_err(|_| StateError::InvalidSavedProfile)?;
        if saved_profile.as_ref().is_some_and(|saved| {
            saved
                .secret_ref()
                .is_some_and(|secret_ref| !secret_ref.is_persistent())
                || !profiles_match_except_secret(&updated_active, saved)
        }) {
            return Err(StateError::InvalidSavedProfile);
        }
        let saved_profile_present = saved_profile.is_some();
        let mut saved_profiles = self.saved_profiles.clone();
        if let Some(saved_profile) = saved_profile {
            if saved_profile_by_id(&saved_profiles, profile_id).is_none() {
                return Err(StateError::InvalidSavedProfile);
            }
            upsert_saved_profile(&mut saved_profiles, saved_profile);
        }

        self.active_provider = Some(updated_active);
        self.selected_model = Some(model_id.to_owned());
        self.pending_model_selection = None;
        if saved_profile_present {
            self.saved_profiles = saved_profiles;
        }
        Ok(())
    }

    /// 回滚当前等待确认的模型选择并保留活动模型。
    pub fn model_selection_failed(
        &mut self,
        model_id: &str,
        error: TranslationError,
    ) -> Result<(), StateError> {
        if self.pending_model_selection.as_deref() != Some(model_id) {
            return Err(StateError::UnexpectedModelSelection);
        }
        self.pending_model_selection = None;
        self.error = Some(error);
        Ok(())
    }

    /// 开始等待工作线程确认删除一个已保存配置。
    pub fn begin_profile_deletion(
        &mut self,
        profile_id: &ProviderProfileId,
    ) -> Result<(), StateError> {
        if !self.profile_storage_available() {
            return Err(StateError::ProfileStorageUnavailable);
        }
        if self.pending_profile_deletion.is_some() {
            return Err(StateError::ProfileDeletionPending);
        }
        if self.status == AppStatus::Connecting {
            return Err(StateError::Connecting);
        }
        if matches!(self.status, AppStatus::Translating | AppStatus::Cancelling) {
            return Err(StateError::Busy);
        }
        if self.pending_model_selection.is_some() {
            return Err(StateError::ModelSelectionPending);
        }
        if saved_profile_by_id(&self.saved_profiles, profile_id).is_none() {
            return Err(StateError::InvalidSavedProfile);
        }

        self.pending_profile_deletion = Some(profile_id.clone());
        self.error = None;
        Ok(())
    }

    /// 仅提交与当前待删除标识完全匹配的删除结果。
    pub fn confirm_profile_deletion(
        &mut self,
        profile_id: &ProviderProfileId,
    ) -> Result<(), StateError> {
        if self.pending_profile_deletion.as_ref() != Some(profile_id) {
            return Err(StateError::UnexpectedProfileDeletion);
        }
        let Some(index) = self
            .saved_profiles
            .iter()
            .position(|profile| profile.id() == profile_id)
        else {
            return Err(StateError::UnexpectedProfileDeletion);
        };

        self.saved_profiles.remove(index);
        if self.selected_saved_profile_id.as_ref() == Some(profile_id) {
            self.selected_saved_profile_id = None;
        }
        if self.persisted_active_profile_id.as_ref() == Some(profile_id) {
            self.persisted_active_profile_id = None;
        }
        if self.active_saved_profile_id.as_ref() == Some(profile_id) {
            self.active_saved_profile_id = None;
        }
        self.pending_profile_deletion = None;
        self.error = None;
        Ok(())
    }

    /// 回滚精确匹配的配置删除并保留全部运行时和持久化镜像状态。
    pub fn profile_deletion_failed(
        &mut self,
        profile_id: &ProviderProfileId,
        error: TranslationError,
    ) -> Result<(), StateError> {
        if self.pending_profile_deletion.as_ref() != Some(profile_id) {
            return Err(StateError::UnexpectedProfileDeletion);
        }
        self.pending_profile_deletion = None;
        self.error = Some(error);
        Ok(())
    }

    /// 直接选择已发现模型。
    pub fn select_model(&mut self, model_id: &str) -> Result<(), StateError> {
        if self.models.iter().any(|model| model.id == model_id) {
            self.selected_model = Some(model_id.to_owned());
            self.pending_model_selection = None;
            Ok(())
        } else {
            Err(StateError::UnknownModel(model_id.to_owned()))
        }
    }

    /// 更新源文本。
    pub fn set_source_text(&mut self, source_text: impl Into<String>) {
        self.source_text = source_text.into();
    }

    /// 更新可选源语言标签。
    pub fn set_source_locale(&mut self, source_locale: Option<String>) {
        self.source_locale = source_locale;
    }

    /// 更新目标语言标签。
    pub fn set_target_locale(&mut self, target_locale: impl Into<String>) {
        self.target_locale = target_locale.into();
    }

    /// 更新外观偏好。
    pub const fn set_theme(&mut self, theme: ThemePreference) {
        self.theme = theme;
    }

    /// 更新界面区域设置。
    pub const fn set_locale(&mut self, locale: UiLocale) {
        self.locale = locale;
    }

    /// 创建共享核心请求并重置流式状态。
    pub fn begin_translation(&mut self) -> Result<TranslationRequest, StateError> {
        if self.status == AppStatus::Connecting {
            return Err(StateError::Connecting);
        }
        if matches!(self.status, AppStatus::Translating | AppStatus::Cancelling) {
            return Err(StateError::Busy);
        }
        if self.pending_model_selection.is_some() {
            return Err(StateError::ModelSelectionPending);
        }
        if self.pending_profile_deletion.is_some() {
            return Err(StateError::ProfileDeletionPending);
        }
        if self.active_provider.is_none() {
            return Err(StateError::MissingProvider);
        }
        if self.source_text.trim().is_empty() {
            return Err(StateError::MissingSource);
        }
        let model_id = self
            .selected_model
            .clone()
            .ok_or(StateError::MissingModel)?;
        let mut request = TranslationRequest::new(
            self.source_text.clone(),
            self.target_locale.clone(),
            model_id,
        );
        request.source_locale.clone_from(&self.source_locale);
        self.output.clear();
        self.partial_output = false;
        self.status = AppStatus::Translating;
        self.error = None;
        self.last_sequence = None;
        Ok(request)
    }

    /// 标记用户已请求取消。
    pub fn request_cancellation(&mut self) -> Result<(), StateError> {
        if self.status != AppStatus::Translating {
            return Err(StateError::Busy);
        }
        self.status = AppStatus::Cancelling;
        Ok(())
    }

    /// 应用共享核心按序产生的事件。
    pub fn apply_translation_event(&mut self, event: TranslationEvent) -> Result<(), StateError> {
        if self.status.is_terminal() {
            return Err(StateError::EventAfterTerminal);
        }
        let sequence = event.sequence();
        if self.last_sequence.is_none() && !matches!(&event, TranslationEvent::Started { .. }) {
            return Err(StateError::UnexpectedFirstEvent);
        }
        if self.last_sequence.is_some() && matches!(&event, TranslationEvent::Started { .. }) {
            return Err(StateError::UnexpectedStartedEvent);
        }
        if self.last_sequence.is_some_and(|last| sequence <= last) {
            return Err(StateError::NonIncreasingSequence);
        }
        self.last_sequence = Some(sequence);
        match event {
            TranslationEvent::Started { .. } => {
                if self.status != AppStatus::Cancelling {
                    self.status = AppStatus::Translating;
                }
            }
            TranslationEvent::TextDelta { text, .. } => {
                self.output.push_str(&text);
                self.partial_output = true;
            }
            TranslationEvent::Completed { .. } => {
                self.status = AppStatus::Completed;
                self.partial_output = false;
            }
            TranslationEvent::Cancelled { .. } => {
                self.status = AppStatus::Cancelled;
                self.partial_output = !self.output.is_empty();
            }
            TranslationEvent::Failed { error, .. } => {
                self.status = AppStatus::Failed;
                self.partial_output = !self.output.is_empty();
                self.error = Some(error);
            }
        }
        Ok(())
    }

    /// 记录无法提交到工作线程的客户端错误。
    pub fn record_client_error(&mut self, message: impl Into<String>) {
        self.status = AppStatus::Failed;
        self.error = Some(TranslationError::new(ErrorKind::Internal, message));
    }

    /// 记录流协议错误并等待取消终止事件。
    pub fn record_stream_error(&mut self, message: impl Into<String>) {
        self.status = AppStatus::Cancelling;
        self.partial_output = !self.output.is_empty();
        self.error = Some(TranslationError::new(ErrorKind::Internal, message));
    }

    /// 记录不再等待终止事件的操作失败。
    pub fn record_operation_failure(&mut self, error: TranslationError) {
        self.status = AppStatus::Failed;
        self.partial_output = !self.output.is_empty();
        self.error = Some(error);
    }

    /// 返回安全的类型化错误文本。
    #[must_use]
    pub fn error_text(&self) -> Option<String> {
        self.error.as_ref().map(|error| {
            let category = match error.kind {
                ErrorKind::Cancelled => "Cancellation",
                ErrorKind::InvalidEndpoint => "Invalid endpoint",
                ErrorKind::Network => "Network",
                ErrorKind::Timeout => "Timeout",
                ErrorKind::Authentication => "Authentication",
                ErrorKind::ModelUnavailable => "Model unavailable",
                ErrorKind::MalformedResponse => "Malformed response",
                ErrorKind::Persistence => "Persistence",
                ErrorKind::ProtocolIncompatible => "Protocol incompatible",
                ErrorKind::InvalidConfiguration => "Invalid configuration",
                ErrorKind::UnsupportedCapability => "Unsupported capability",
                ErrorKind::SecretUnavailable => "Secret unavailable",
                ErrorKind::SecureStorageUnavailable => "Secure storage unavailable",
                ErrorKind::Internal => "Internal",
            };
            format!("{category}: {}", error.message)
        })
    }

    /// 返回面向界面的本地化错误文本；可变诊断内容保留英文回退。
    #[must_use]
    pub fn localized_error_text(&self, locale: UiLocale) -> Option<String> {
        self.error.as_ref().map(|error| {
            let (category_key, category_fallback) = error_category(error.kind);
            let category = localization::text(locale, category_key, category_fallback);
            let message = localized_error_message(locale, &error.message);
            format!("{category}: {message}")
        })
    }

    /// 构建不包含源文本或凭据的诊断摘要。
    #[must_use]
    pub fn diagnostics_text(&self) -> String {
        format!(
            "Core protocol: {PROTOCOL_VERSION}\nOnboarding: {}\nProvider: {}\nProvider saved: {}\nProfile storage: {}\nSaved profiles: {}\nSaved profile: {}\nPersisted active profile: {}\nSaved model: {}\nModel selected: {}\nModel selection pending: {}\nProfile deletion pending: {}\nStatus: {}\nTheme: {}\nLocale: {}\nOutput bytes: {}",
            self.onboarding_stage().label(),
            yes_no(self.active_provider.is_some()),
            yes_no(self.active_provider_is_saved()),
            self.profile_storage_status.label(),
            self.saved_profiles.len(),
            yes_no(self.selected_saved_profile_id.is_some()),
            yes_no(self.persisted_active_profile_id.is_some()),
            yes_no(
                self.selected_saved_profile()
                    .is_some_and(|profile| profile.selected_model().is_some())
            ),
            if self.selected_model.is_some() {
                "Yes"
            } else {
                "No"
            },
            if self.pending_model_selection.is_some() {
                "Yes"
            } else {
                "No"
            },
            yes_no(self.pending_profile_deletion.is_some()),
            self.status.label(),
            self.theme.label(),
            self.locale.language_tag(),
            self.output.len()
        )
    }
}

fn error_category(kind: ErrorKind) -> (&'static str, &'static str) {
    match kind {
        ErrorKind::Cancelled => ("error.category.cancellation", "Cancellation"),
        ErrorKind::InvalidEndpoint => ("error.category.invalid_endpoint", "Invalid endpoint"),
        ErrorKind::Network => ("error.category.network", "Network"),
        ErrorKind::Timeout => ("error.category.timeout", "Timeout"),
        ErrorKind::Authentication => ("error.category.authentication", "Authentication"),
        ErrorKind::ModelUnavailable => ("error.category.model_unavailable", "Model unavailable"),
        ErrorKind::MalformedResponse => ("error.category.malformed_response", "Malformed response"),
        ErrorKind::Persistence => ("error.category.persistence", "Persistence"),
        ErrorKind::ProtocolIncompatible => (
            "error.category.protocol_incompatible",
            "Protocol incompatible",
        ),
        ErrorKind::InvalidConfiguration => (
            "error.category.invalid_configuration",
            "Invalid configuration",
        ),
        ErrorKind::UnsupportedCapability => (
            "error.category.unsupported_capability",
            "Unsupported capability",
        ),
        ErrorKind::SecretUnavailable => ("error.category.secret_unavailable", "Secret unavailable"),
        ErrorKind::SecureStorageUnavailable => (
            "error.category.secure_storage_unavailable",
            "Secure storage unavailable",
        ),
        ErrorKind::Internal => ("error.category.internal", "Internal"),
    }
}

fn state_error_message(message: &str) -> Option<(&'static str, &'static str)> {
    Some(match message {
        "Enter source text before translating." => (
            "error.state.missing_source",
            "Enter source text before translating.",
        ),
        "Select a model before translating." => (
            "error.state.missing_model",
            "Select a model before translating.",
        ),
        "A model selection is still being confirmed." => (
            "error.state.model_selection_pending",
            "A model selection is still being confirmed.",
        ),
        "The model selection result is stale or unexpected." => (
            "error.state.unexpected_model_selection",
            "The model selection result is stale or unexpected.",
        ),
        "The provider profile is disabled." => (
            "error.state.invalid_profile",
            "The provider profile is disabled.",
        ),
        "Connect a provider before translating." => (
            "error.state.missing_provider",
            "Connect a provider before translating.",
        ),
        "The provider connection result is stale or unexpected." => (
            "error.state.unexpected_provider_connection",
            "The provider connection result is stale or unexpected.",
        ),
        "A translation is already running." => {
            ("error.state.busy", "A translation is already running.")
        }
        "A provider connection is still in progress." => (
            "error.state.connecting",
            "A provider connection is still in progress.",
        ),
        "The saved provider profile does not match this operation." => (
            "error.state.invalid_saved_profile",
            "The saved provider profile does not match this operation.",
        ),
        "Saved profile storage is unavailable." => (
            "error.state.profile_storage_unavailable",
            "Saved profile storage is unavailable.",
        ),
        "A saved profile deletion is still being confirmed." => (
            "error.state.profile_deletion_pending",
            "A saved profile deletion is still being confirmed.",
        ),
        "The saved profile deletion result is stale or unexpected." => (
            "error.state.unexpected_profile_deletion",
            "The saved profile deletion result is stale or unexpected.",
        ),
        "The core stream did not begin with a started event." => (
            "error.state.unexpected_first_event",
            "The core stream did not begin with a started event.",
        ),
        "The core stream produced more than one started event." => (
            "error.state.unexpected_started_event",
            "The core stream produced more than one started event.",
        ),
        "The core stream produced an out-of-order event." => (
            "error.state.non_increasing_sequence",
            "The core stream produced an out-of-order event.",
        ),
        "The core stream produced an event after termination." => (
            "error.state.event_after_terminal",
            "The core stream produced an event after termination.",
        ),
        _ => return additional_state_error_message(message),
    })
}

#[allow(clippy::too_many_lines)]
fn additional_state_error_message(message: &str) -> Option<(&'static str, &'static str)> {
    Some(match message {
        "A provider cannot be changed while a translation is running." => (
            "error.state.provider_change_busy",
            "A provider cannot be changed while a translation is running.",
        ),
        "A model cannot be changed while a translation is running." => (
            "error.state.model_change_busy",
            "A model cannot be changed while a translation is running.",
        ),
        "A saved profile cannot be removed while a translation is running." => (
            "error.state.profile_removal_busy",
            "A saved profile cannot be removed while a translation is running.",
        ),
        "The core event stream ended without a terminal event." => (
            "error.state.stream_missing_terminal",
            "The core event stream ended without a terminal event.",
        ),
        "Secret Service cleanup failed after profile removal." => (
            "error.storage.cleanup_failed",
            "Secret Service cleanup failed after profile removal.",
        ),
        "Secure credential storage is unavailable." => (
            "error.storage.secure_unavailable",
            "Secure credential storage is unavailable.",
        ),
        "Profile storage is unavailable; use session-only mode." => (
            "error.storage.session_only",
            "Profile storage is unavailable; use session-only mode.",
        ),
        "Profile storage became unavailable." => (
            "error.storage.became_unavailable",
            "Profile storage became unavailable.",
        ),
        "Profile storage is unavailable; no saved profile was removed." => (
            "error.storage.remove_failed",
            "Profile storage is unavailable; no saved profile was removed.",
        ),
        "The saved provider profile no longer exists." => (
            "error.profile.not_found",
            "The saved provider profile no longer exists.",
        ),
        "Connect a provider before selecting a model." => (
            "error.provider.select_model_requires_connection",
            "Connect a provider before selecting a model.",
        ),
        "The model selection belongs to a stale provider." => (
            "error.provider.stale_model_selection",
            "The model selection belongs to a stale provider.",
        ),
        "The selected model is not available from the active provider." => (
            "error.provider.model_unavailable",
            "The selected model is not available from the active provider.",
        ),
        "The translation request does not use the confirmed model selection." => (
            "error.provider.unconfirmed_model",
            "The translation request does not use the confirmed model selection.",
        ),
        "The core command queue is unavailable or full." => (
            "error.worker.command_queue_unavailable",
            "The core command queue is unavailable or full.",
        ),
        "The selected file is not valid UTF-8 text." => (
            "error.file.invalid_utf8",
            "The selected file is not valid UTF-8 text.",
        ),
        "A unique provider profile ID could not be generated." => (
            "error.profile.id_generation_failed",
            "A unique provider profile ID could not be generated.",
        ),
        "The saved model could not be updated." => (
            "error.profile.saved_model_update_failed",
            "The saved model could not be updated.",
        ),
        "The host secret request channel closed." => (
            "error.secret.request_channel_closed",
            "The host secret request channel closed.",
        ),
        "The Core secret request was no longer active." => (
            "error.secret.request_inactive",
            "The Core secret request was no longer active.",
        ),
        "A session credential requires an explicit session secret reference." => (
            "error.provider.session_ref_required",
            "A session credential requires an explicit session secret reference.",
        ),
        "The profile database path must be absolute." => (
            "error.storage.path_absolute",
            "The profile database path must be absolute.",
        ),
        "The profile database directory is invalid." => (
            "error.storage.directory_invalid",
            "The profile database directory is invalid.",
        ),
        "The profile database directory could not be created." => (
            "error.storage.directory_create",
            "The profile database directory could not be created.",
        ),
        "The profile database directory could not be inspected." => (
            "error.storage.directory_inspect",
            "The profile database directory could not be inspected.",
        ),
        "The profile database directory permissions are not private." => (
            "error.storage.directory_permissions",
            "The profile database directory permissions are not private.",
        ),
        "The profile database must be a private regular file." => (
            "error.storage.file_regular",
            "The profile database must be a private regular file.",
        ),
        "The profile database path could not be inspected." => (
            "error.storage.path_inspect",
            "The profile database path could not be inspected.",
        ),
        "The profile database file could not be opened." => (
            "error.storage.file_open",
            "The profile database file could not be opened.",
        ),
        "The profile database file could not be inspected." => (
            "error.storage.file_inspect",
            "The profile database file could not be inspected.",
        ),
        "The profile database file permissions could not be restricted." => (
            "error.storage.file_permissions",
            "The profile database file permissions could not be restricted.",
        ),
        "The profile database path cannot contain symbolic links." => (
            "error.storage.path_symlink",
            "The profile database path cannot contain symbolic links.",
        ),
        "The profile database path components could not be inspected." => (
            "error.storage.path_components_inspect",
            "The profile database path components could not be inspected.",
        ),
        _ => return None,
    })
}

fn localized_error_message(locale: UiLocale, message: &str) -> String {
    if let Some((key, fallback)) = state_error_message(message) {
        return localization::text(locale, key, fallback);
    }

    for (prefix, key, fallback) in [
        (
            "The provider profile is invalid: ",
            "error.profile.invalid_with_detail",
            "The provider profile is invalid: {error}",
        ),
        (
            "The generated provider profile ID is invalid: ",
            "error.profile.id_invalid",
            "The generated provider profile ID is invalid: {error}",
        ),
        (
            "The shared Core contract is incompatible: ",
            "error.core.contract_incompatible",
            "The shared Core contract is incompatible: {error}",
        ),
        (
            "Core compatibility could not be read: ",
            "error.core.compatibility_read_failed",
            "Core compatibility could not be read: {error}",
        ),
        (
            "Failed to start the loopback provider: ",
            "error.provider.loopback_start_failed",
            "Failed to start the loopback provider: {error}",
        ),
        (
            "Failed to start the core runtime: ",
            "error.runtime.start_failed",
            "Failed to start the core runtime: {error}",
        ),
    ] {
        if let Some(detail) = message.strip_prefix(prefix) {
            return localization::text(locale, key, fallback).replace("{error}", detail);
        }
    }

    message.to_owned()
}

fn saved_profile_by_id<'a>(
    saved_profiles: &'a [ProviderProfile],
    profile_id: &ProviderProfileId,
) -> Option<&'a ProviderProfile> {
    saved_profiles
        .iter()
        .find(|profile| profile.id() == profile_id)
}

fn sort_saved_profiles(saved_profiles: &mut [ProviderProfile]) {
    saved_profiles.sort_by(|left, right| {
        left.display_name()
            .cmp(right.display_name())
            .then_with(|| left.id().as_str().cmp(right.id().as_str()))
    });
}

fn upsert_saved_profile(saved_profiles: &mut Vec<ProviderProfile>, saved_profile: ProviderProfile) {
    match saved_profiles
        .iter()
        .position(|profile| profile.id() == saved_profile.id())
    {
        Some(index) => saved_profiles[index] = saved_profile,
        None => saved_profiles.push(saved_profile),
    }
    sort_saved_profiles(saved_profiles);
}

fn profiles_match_except_secret(runtime: &ProviderProfile, saved: &ProviderProfile) -> bool {
    runtime.id() == saved.id()
        && runtime.display_name() == saved.display_name()
        && runtime.preset_id() == saved.preset_id()
        && runtime.adapter_type() == saved.adapter_type()
        && runtime.base_endpoint() == saved.base_endpoint()
        && runtime.enabled() == saved.enabled()
        && runtime.selected_model() == saved.selected_model()
}

const fn yes_no(value: bool) -> &'static str {
    if value { "Yes" } else { "No" }
}

#[cfg(test)]
mod tests {
    use super::{
        AppState, AppStatus, OnboardingStage, ProfileStorageStatus, ProviderProfile,
        ProviderProfileId, StateError, ThemePreference, UiLocale,
    };
    use linguamesh_domain::{
        ErrorKind, ModelDescriptor, ModelSource, SecretRef, SecretRefNamespace, TranslationError,
        TranslationEvent,
    };

    fn profile(
        id: &str,
        display_name: &str,
        endpoint: &str,
        selected_model: Option<&str>,
    ) -> ProviderProfile {
        profile_with_secret(id, display_name, endpoint, selected_model, None)
    }

    fn profile_with_secret(
        id: &str,
        display_name: &str,
        endpoint: &str,
        selected_model: Option<&str>,
        secret_ref: Option<SecretRef>,
    ) -> ProviderProfile {
        ProviderProfile::new(
            ProviderProfileId::parse(id).expect("profile ID"),
            display_name,
            "openai-compatible",
            "openai_chat_completions",
            endpoint,
            secret_ref,
        )
        .expect("profile")
        .with_selected_model(selected_model.map(str::to_owned))
        .expect("selected model")
    }

    fn discovered_model(id: &str) -> ModelDescriptor {
        ModelDescriptor {
            id: id.to_owned(),
            display_name: format!("{id} display name"),
            source: ModelSource::Discovered,
        }
    }

    fn connected_state() -> AppState {
        let mut state = state_with_profile_storage();
        let profile = profile(
            "local-fake-provider",
            "Local fake provider",
            "http://127.0.0.1:4000/v1/",
            None,
        );
        state
            .begin_provider_connection(profile.clone())
            .expect("begin connection");
        state
            .provider_connected(profile, vec![discovered_model("fake-translator")])
            .expect("provider connected");
        state.select_model("fake-translator").expect("select model");
        state.set_source_text("Hello");
        state
    }

    #[test]
    fn localized_error_text_localizes_fixed_state_messages_and_categories() {
        let mut state = AppState::default();
        state.record_client_error(StateError::MissingSource.to_string());
        assert_eq!(
            state
                .localized_error_text(UiLocale::SimplifiedChinese)
                .as_deref(),
            Some("内部错误: 翻译前请输入源文本。")
        );

        state.record_operation_failure(TranslationError::new(
            ErrorKind::Network,
            "The provider could not be reached.",
        ));
        assert_eq!(
            state
                .localized_error_text(UiLocale::SimplifiedChinese)
                .as_deref(),
            Some("网络: The provider could not be reached.")
        );

        state.record_client_error("The selected file is not valid UTF-8 text.");
        assert_eq!(
            state
                .localized_error_text(UiLocale::SimplifiedChinese)
                .as_deref(),
            Some("内部错误: 所选文件不是有效的 UTF-8 文本。")
        );

        state.record_client_error("The provider profile is invalid: invalid endpoint");
        assert_eq!(
            state
                .localized_error_text(UiLocale::SimplifiedChinese)
                .as_deref(),
            Some("内部错误: 提供商配置无效：invalid endpoint")
        );
    }

    #[test]
    fn localized_error_text_localizes_runtime_and_storage_messages() {
        let cases = [
            (
                ErrorKind::ProtocolIncompatible,
                "Core compatibility could not be read: missing compatibility symbol",
                "协议不兼容: 无法读取核心兼容性信息：missing compatibility symbol",
            ),
            (
                ErrorKind::Network,
                "Failed to start the loopback provider: address already in use",
                "网络: 启动回环提供商失败：address already in use",
            ),
            (
                ErrorKind::Internal,
                "Failed to start the core runtime: runtime initialization failed",
                "内部错误: 启动核心运行时失败：runtime initialization failed",
            ),
            (
                ErrorKind::Persistence,
                "The profile database path must be absolute.",
                "持久化: 配置数据库路径必须为绝对路径。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database directory is invalid.",
                "持久化: 配置数据库目录无效。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database directory could not be created.",
                "持久化: 无法创建配置数据库目录。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database directory could not be inspected.",
                "持久化: 无法检查配置数据库目录。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database directory permissions are not private.",
                "持久化: 配置数据库目录权限不是私有的。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database must be a private regular file.",
                "持久化: 配置数据库必须是私有普通文件。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database path could not be inspected.",
                "持久化: 无法检查配置数据库路径。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database file could not be opened.",
                "持久化: 无法打开配置数据库文件。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database file could not be inspected.",
                "持久化: 无法检查配置数据库文件。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database file permissions could not be restricted.",
                "持久化: 无法限制配置数据库文件权限。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database path cannot contain symbolic links.",
                "持久化: 配置数据库路径不能包含符号链接。",
            ),
            (
                ErrorKind::Persistence,
                "The profile database path components could not be inspected.",
                "持久化: 无法检查配置数据库路径组件。",
            ),
        ];
        for (kind, message, expected) in cases {
            let mut state = AppState::default();
            state.record_operation_failure(TranslationError::new(kind, message));
            assert_eq!(
                state
                    .localized_error_text(UiLocale::SimplifiedChinese)
                    .as_deref(),
                Some(expected)
            );
        }
    }

    fn state_with_profile_storage() -> AppState {
        let mut state = AppState::default();
        state
            .restore_saved_profiles(Vec::new(), None)
            .expect("profile storage");
        state
    }

    #[test]
    fn default_state_has_no_active_or_pending_provider() {
        let mut state = AppState::default();

        assert_eq!(state.status(), AppStatus::Disconnected);
        assert_eq!(state.onboarding_stage(), OnboardingStage::Starting);
        assert!(!state.worker_ready());
        assert!(state.active_provider().is_none());
        assert!(state.provider_id().is_none());
        assert!(state.pending_provider().is_none());
        assert!(state.saved_profile().is_none());
        assert!(!state.pending_provider_will_be_saved());
        assert!(!state.active_provider_is_saved());
        assert!(state.models().is_empty());
        assert_eq!(state.selected_model(), None);
        assert_eq!(state.pending_model_selection(), None);
        assert!(state.diagnostics_text().contains("Provider: No"));

        state.mark_worker_ready();
        assert!(state.worker_ready());
        assert_eq!(state.onboarding_stage(), OnboardingStage::ConfigureProvider);

        state.mark_worker_unavailable();
        assert!(!state.worker_ready());
        assert!(state.worker_unavailable());
        assert_eq!(state.onboarding_stage(), OnboardingStage::Unavailable);

        state.set_source_text("Hello");
        assert_eq!(state.begin_translation(), Err(StateError::MissingProvider));
    }

    #[test]
    fn restored_saved_profile_remains_disconnected() {
        let mut state = AppState::default();
        let saved = profile(
            "saved-provider",
            "Saved provider",
            "http://127.0.0.1:11434/v1/",
            Some("saved-model"),
        );

        state
            .restore_saved_profile(saved.clone())
            .expect("restore saved profile");
        state.mark_worker_ready();

        assert_eq!(state.status(), AppStatus::Disconnected);
        assert_eq!(state.onboarding_stage(), OnboardingStage::ConfigureProvider);
        assert_eq!(state.saved_profile(), Some(&saved));
        assert!(state.active_provider().is_none());
        assert!(state.pending_provider().is_none());
        assert!(!state.pending_provider_will_be_saved());
        assert!(!state.active_provider_is_saved());
        assert!(state.models().is_empty());
        assert_eq!(state.selected_model(), None);
    }

    #[test]
    fn onboarding_stages_follow_connection_and_model_readiness() {
        let mut state = state_with_profile_storage();
        state.mark_worker_ready();
        let provider = profile(
            "onboarding-provider",
            "Onboarding provider",
            "http://127.0.0.1:11434/v1/",
            None,
        );

        assert_eq!(state.onboarding_stage(), OnboardingStage::ConfigureProvider);
        state
            .begin_provider_connection(provider.clone())
            .expect("begin onboarding connection");
        assert_eq!(state.onboarding_stage(), OnboardingStage::Connecting);
        state
            .provider_connected(provider, vec![discovered_model("onboarding-model")])
            .expect("complete onboarding connection");
        assert_eq!(state.onboarding_stage(), OnboardingStage::SelectModel);
        state
            .select_model("onboarding-model")
            .expect("select onboarding model");
        assert_eq!(state.onboarding_stage(), OnboardingStage::Ready);
    }

    #[test]
    fn onboarding_labels_are_stable_and_non_sensitive() {
        assert_eq!(OnboardingStage::Starting.label(), "Starting");
        assert_eq!(OnboardingStage::Unavailable.label(), "Unavailable");
        assert_eq!(
            OnboardingStage::ConfigureProvider.label(),
            "Configure provider"
        );
        assert_eq!(OnboardingStage::Connecting.label(), "Connecting");
        assert_eq!(OnboardingStage::SelectModel.label(), "Select model");
        assert_eq!(OnboardingStage::Ready.label(), "Ready");
    }

    #[test]
    fn session_secret_reference_cannot_be_restored_as_saved_state() {
        let mut state = AppState::default();
        let invalid = profile_with_secret(
            "session-provider",
            "Session provider",
            "http://127.0.0.1:11434/v1/",
            None,
            Some(SecretRef::new(SecretRefNamespace::Session)),
        );

        assert_eq!(
            state.restore_saved_profile(invalid),
            Err(StateError::InvalidSavedProfile)
        );
        assert_eq!(state.status(), AppStatus::Disconnected);
        assert!(state.saved_profile().is_none());
    }

    #[test]
    fn multiple_profiles_restore_atomically_sorted_and_disconnected() {
        let mut state = AppState::default();
        let alpha = profile(
            "alpha-provider",
            "Shared provider",
            "http://127.0.0.1:11434/v1/",
            Some("alpha-model"),
        );
        let beta = profile(
            "beta-provider",
            "Shared provider",
            "http://127.0.0.1:11434/v1/",
            Some("beta-model"),
        );

        state
            .restore_saved_profiles(vec![beta.clone(), alpha.clone()], Some(beta.id().clone()))
            .expect("restore profiles");
        state.mark_worker_ready();

        assert_eq!(
            state.profile_storage_status(),
            ProfileStorageStatus::Available
        );
        assert_eq!(state.onboarding_stage(), OnboardingStage::ConfigureProvider);
        assert_eq!(state.saved_profiles(), &[alpha, beta.clone()]);
        assert_eq!(state.selected_saved_profile(), Some(&beta));
        assert_eq!(state.persisted_active_profile_id(), Some(beta.id()));
        assert!(state.active_saved_profile_id().is_none());
        assert_eq!(state.status(), AppStatus::Disconnected);
        assert!(state.active_provider().is_none());
        assert!(state.models().is_empty());
        assert_eq!(state.selected_model(), None);
    }

    #[test]
    fn invalid_profile_snapshots_preserve_the_previous_snapshot() {
        let mut state = AppState::default();
        let baseline = profile(
            "baseline-provider",
            "Baseline provider",
            "http://127.0.0.1:11434/v1/",
            Some("baseline-model"),
        );
        state
            .restore_saved_profiles(vec![baseline.clone()], Some(baseline.id().clone()))
            .expect("baseline snapshot");
        let original_profiles = state.saved_profiles().to_vec();
        let original_selected = state.selected_saved_profile_id().cloned();
        let original_active = state.persisted_active_profile_id().cloned();

        let duplicate_first = profile(
            "duplicate-provider",
            "Alpha duplicate",
            "http://127.0.0.1:11435/v1/",
            None,
        );
        let duplicate_second = profile(
            "duplicate-provider",
            "Zulu duplicate",
            "http://127.0.0.1:11436/v1/",
            None,
        );
        let middle = profile(
            "middle-provider",
            "Middle provider",
            "http://127.0.0.1:11437/v1/",
            None,
        );
        assert_eq!(
            state.restore_saved_profiles(
                vec![duplicate_second, middle, duplicate_first.clone()],
                Some(duplicate_first.id().clone()),
            ),
            Err(StateError::InvalidSavedProfile)
        );
        let missing_id = ProviderProfileId::parse("missing-provider").expect("profile ID");
        assert_eq!(
            state.restore_saved_profiles(vec![duplicate_first], Some(missing_id)),
            Err(StateError::InvalidSavedProfile)
        );
        let session = profile_with_secret(
            "session-provider",
            "Session provider",
            "http://127.0.0.1:11438/v1/",
            None,
            Some(SecretRef::new(SecretRefNamespace::Session)),
        );
        assert_eq!(
            state.restore_saved_profiles(vec![session], None),
            Err(StateError::InvalidSavedProfile)
        );

        assert_eq!(state.saved_profiles(), original_profiles);
        assert_eq!(
            state.selected_saved_profile_id(),
            original_selected.as_ref()
        );
        assert_eq!(
            state.persisted_active_profile_id(),
            original_active.as_ref()
        );
        assert_eq!(
            state.profile_storage_status(),
            ProfileStorageStatus::Available
        );
    }

    #[test]
    fn selecting_a_saved_profile_does_not_activate_or_connect_it() {
        let mut state = AppState::default();
        let alpha = profile(
            "alpha-provider",
            "Alpha provider",
            "http://127.0.0.1:11434/v1/",
            None,
        );
        let beta = profile(
            "beta-provider",
            "Beta provider",
            "http://127.0.0.1:11435/v1/",
            None,
        );
        state
            .restore_saved_profiles(vec![alpha.clone(), beta.clone()], Some(alpha.id().clone()))
            .expect("restore profiles");

        state
            .select_saved_profile(Some(beta.id()))
            .expect("select beta");
        assert_eq!(state.selected_saved_profile(), Some(&beta));
        assert_eq!(state.persisted_active_profile_id(), Some(alpha.id()));
        assert!(state.active_provider().is_none());
        assert_eq!(state.status(), AppStatus::Disconnected);

        state
            .select_saved_profile(None)
            .expect("select new profile");
        let unknown = ProviderProfileId::parse("unknown-provider").expect("profile ID");
        assert_eq!(
            state.select_saved_profile(Some(&unknown)),
            Err(StateError::InvalidSavedProfile)
        );
        assert!(state.selected_saved_profile().is_none());
        assert_eq!(state.persisted_active_profile_id(), Some(alpha.id()));
    }

    #[test]
    fn deliberate_model_selection_is_required_and_request_uses_state() {
        let mut state = AppState::default();
        let profile = profile(
            "local-fake-provider",
            "Local fake provider",
            "http://127.0.0.1:4000/v1/",
            None,
        );
        state
            .begin_provider_connection(profile.clone())
            .expect("begin connection");
        state
            .provider_connected(profile, vec![discovered_model("fake-translator")])
            .expect("provider connected");
        state.set_source_text("Hello");

        assert_eq!(state.selected_model(), None);
        assert_eq!(state.begin_translation(), Err(StateError::MissingModel));

        state.select_model("fake-translator").expect("select model");
        state.set_source_locale(Some("en".to_owned()));
        let request = state.begin_translation().expect("request");
        assert_eq!(request.model_id, "fake-translator");
        assert_eq!(request.source_locale.as_deref(), Some("en"));
        assert_eq!(request.target_locale, "zh-CN");
        assert_eq!(state.status(), AppStatus::Translating);
    }

    #[test]
    fn pending_model_selection_blocks_conflicting_actions_until_confirmation() {
        let mut state = connected_state();
        state.mark_worker_ready();
        state.models.push(discovered_model("fake-slow-translator"));

        assert_eq!(state.onboarding_stage(), OnboardingStage::Ready);

        state
            .begin_model_selection("fake-slow-translator")
            .expect("begin model selection");

        assert_eq!(
            state.pending_model_selection(),
            Some("fake-slow-translator")
        );
        assert_eq!(state.selected_model(), Some("fake-translator"));
        assert_eq!(state.onboarding_stage(), OnboardingStage::SelectModel);
        assert_eq!(
            state.begin_translation(),
            Err(StateError::ModelSelectionPending)
        );
        assert_eq!(
            state.begin_provider_connection(profile(
                "next-provider",
                "Next provider",
                "http://127.0.0.1:11434/v1/",
                None,
            )),
            Err(StateError::ModelSelectionPending)
        );
        assert_eq!(
            state.select_saved_profile(None),
            Err(StateError::ModelSelectionPending)
        );
        assert_eq!(
            state.confirm_model_selection("fake-translator"),
            Err(StateError::UnexpectedModelSelection)
        );
        assert_eq!(
            state.pending_model_selection(),
            Some("fake-slow-translator")
        );

        state
            .confirm_model_selection("fake-slow-translator")
            .expect("confirm model selection");
        assert_eq!(state.pending_model_selection(), None);
        assert_eq!(state.selected_model(), Some("fake-slow-translator"));
        assert_eq!(state.onboarding_stage(), OnboardingStage::Ready);
    }

    #[test]
    fn rejected_model_selection_preserves_confirmed_model() {
        let mut state = connected_state();
        state.models.push(discovered_model("fake-slow-translator"));
        state
            .begin_model_selection("fake-slow-translator")
            .expect("begin model selection");

        state
            .model_selection_failed(
                "fake-slow-translator",
                TranslationError::new(ErrorKind::ModelUnavailable, "Model disappeared."),
            )
            .expect("reject model selection");

        assert_eq!(state.pending_model_selection(), None);
        assert_eq!(state.selected_model(), Some("fake-translator"));
        assert_eq!(state.status(), AppStatus::Ready);
        assert_eq!(
            state.error_text().as_deref(),
            Some("Model unavailable: Model disappeared.")
        );
    }

    #[test]
    fn streamed_output_completes_in_order() {
        let mut state = connected_state();
        state.begin_translation().expect("request");
        state
            .apply_translation_event(TranslationEvent::Started { sequence: 0 })
            .expect("started");
        state
            .apply_translation_event(TranslationEvent::TextDelta {
                sequence: 1,
                text: "你好".to_owned(),
            })
            .expect("delta");
        assert!(state.has_partial_output());
        state
            .apply_translation_event(TranslationEvent::Completed { sequence: 2 })
            .expect("completed");
        assert_eq!(state.output(), "你好");
        assert_eq!(state.status(), AppStatus::Completed);
        assert!(!state.has_partial_output());
    }

    #[test]
    fn cancellation_retains_partial_output() {
        let mut state = connected_state();
        state.begin_translation().expect("request");
        state
            .apply_translation_event(TranslationEvent::Started { sequence: 0 })
            .expect("started");
        state
            .apply_translation_event(TranslationEvent::TextDelta {
                sequence: 1,
                text: "partial".to_owned(),
            })
            .expect("delta");
        state.request_cancellation().expect("cancel request");
        state
            .apply_translation_event(TranslationEvent::Cancelled { sequence: 2 })
            .expect("cancelled");
        assert_eq!(state.output(), "partial");
        assert!(state.has_partial_output());
        assert_eq!(state.status(), AppStatus::Cancelled);
    }

    #[test]
    fn cancellation_requested_before_started_remains_pending() {
        let mut state = connected_state();
        state.begin_translation().expect("request");
        state.request_cancellation().expect("cancel request");
        state
            .apply_translation_event(TranslationEvent::Started { sequence: 0 })
            .expect("started");
        assert_eq!(state.status(), AppStatus::Cancelling);
        state
            .apply_translation_event(TranslationEvent::Cancelled { sequence: 1 })
            .expect("cancelled");
        assert_eq!(state.status(), AppStatus::Cancelled);
    }

    #[test]
    fn typed_error_is_safe_and_actionable() {
        let mut state = connected_state();
        state.begin_translation().expect("request");
        state
            .apply_translation_event(TranslationEvent::Started { sequence: 0 })
            .expect("started");
        state
            .apply_translation_event(TranslationEvent::Failed {
                sequence: 1,
                error: TranslationError::new(
                    ErrorKind::Authentication,
                    "Check the configured credential.",
                ),
            })
            .expect("failed");
        assert_eq!(
            state.error_text().as_deref(),
            Some("Authentication: Check the configured credential.")
        );
    }

    #[test]
    fn alpha_two_error_kinds_have_actionable_categories() {
        let cases = [
            (ErrorKind::InvalidConfiguration, "Invalid configuration"),
            (ErrorKind::UnsupportedCapability, "Unsupported capability"),
            (ErrorKind::SecretUnavailable, "Secret unavailable"),
            (
                ErrorKind::SecureStorageUnavailable,
                "Secure storage unavailable",
            ),
        ];

        for (kind, category) in cases {
            let mut state = AppState::default();
            state.provider_failed(TranslationError::new(kind, "Action is required."));
            assert_eq!(
                state.error_text(),
                Some(format!("{category}: Action is required."))
            );
        }
    }

    #[test]
    fn out_of_order_events_are_rejected() {
        let mut state = connected_state();
        state.begin_translation().expect("request");
        assert_eq!(
            state.apply_translation_event(TranslationEvent::Completed { sequence: 1 }),
            Err(StateError::UnexpectedFirstEvent)
        );
    }

    #[test]
    fn repeated_started_event_is_rejected() {
        let mut state = connected_state();
        state.begin_translation().expect("request");
        state
            .apply_translation_event(TranslationEvent::Started { sequence: 0 })
            .expect("started");
        assert_eq!(
            state.apply_translation_event(TranslationEvent::Started { sequence: 1 }),
            Err(StateError::UnexpectedStartedEvent)
        );
    }

    #[test]
    fn stream_error_waits_for_cancelled_terminal() {
        let mut state = connected_state();
        state.begin_translation().expect("request");
        state
            .apply_translation_event(TranslationEvent::Started { sequence: 0 })
            .expect("started");
        state.record_stream_error("The core stream violated the event contract.");
        assert_eq!(state.status(), AppStatus::Cancelling);
        state
            .apply_translation_event(TranslationEvent::Cancelled { sequence: 1 })
            .expect("cancelled");
        assert_eq!(state.status(), AppStatus::Cancelled);
        assert_eq!(
            state.error_text().as_deref(),
            Some("Internal: The core stream violated the event contract.")
        );
    }

    #[test]
    fn diagnostics_reflect_theme_and_locale_without_content() {
        let mut state = connected_state();
        state.set_theme(ThemePreference::Dark);
        state.set_locale(UiLocale::SimplifiedChinese);
        let diagnostics = state.diagnostics_text();
        assert!(diagnostics.contains("Onboarding: Starting"));
        assert!(diagnostics.contains("Theme: Dark"));
        assert!(diagnostics.contains("Locale: zh-CN"));
        assert!(!diagnostics.contains("Hello"));
    }

    #[test]
    fn supported_ui_locales_have_unique_bcp47_tags_and_rtl_metadata() {
        let tags = UiLocale::ALL
            .iter()
            .map(|locale| locale.language_tag())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(tags.len(), UiLocale::ALL.len());
        assert!(UiLocale::Arabic.is_rtl());
        assert!(!UiLocale::Hindi.is_rtl());
        assert_eq!(UiLocale::from_index(999), UiLocale::English);
    }

    #[test]
    fn successful_connection_atomically_replaces_profile_and_models() {
        let mut state = connected_state();
        let previous_provider = state
            .provider_id()
            .expect("active provider")
            .as_str()
            .to_owned();
        let next_profile = profile(
            "local-session",
            "Local session provider",
            "http://127.0.0.1:11434/v1/",
            Some("local-model"),
        );

        state
            .begin_provider_connection(next_profile.clone())
            .expect("begin connection");

        assert_eq!(state.status(), AppStatus::Connecting);
        assert_eq!(
            state.provider_id().map(ProviderProfileId::as_str),
            Some(previous_provider.as_str())
        );
        assert_eq!(state.models()[0].id, "fake-translator");
        assert_eq!(
            state
                .pending_provider()
                .map(ProviderProfile::id)
                .map(ProviderProfileId::as_str),
            Some("local-session")
        );
        assert_eq!(state.begin_translation(), Err(StateError::Connecting));

        state
            .provider_connected(next_profile, vec![discovered_model("local-model")])
            .expect("provider connected");

        assert_eq!(state.status(), AppStatus::Ready);
        assert_eq!(
            state.provider_id().map(ProviderProfileId::as_str),
            Some("local-session")
        );
        assert_eq!(state.selected_model(), Some("local-model"));
        assert_eq!(state.models(), &[discovered_model("local-model")]);
        assert!(state.pending_provider().is_none());
    }

    #[test]
    fn saved_connection_commits_runtime_and_non_secret_profiles_atomically() {
        let mut state = state_with_profile_storage();
        let runtime = profile_with_secret(
            "remembered-provider",
            "Remembered provider",
            "http://127.0.0.1:11434/v1/",
            Some("remembered-model"),
            Some(SecretRef::new(SecretRefNamespace::Session)),
        );
        let saved = profile(
            "remembered-provider",
            "Remembered provider",
            "http://127.0.0.1:11434/v1/",
            Some("remembered-model"),
        );

        state
            .begin_provider_connection_with_persistence(runtime.clone(), true)
            .expect("begin saved connection");
        assert!(state.pending_provider_will_be_saved());
        assert_eq!(
            state.provider_connected_with_saved_profile(
                runtime.clone(),
                vec![discovered_model("remembered-model")],
                None,
            ),
            Err(StateError::InvalidSavedProfile)
        );
        assert_eq!(state.status(), AppStatus::Connecting);
        assert_eq!(state.pending_provider(), Some(&runtime));
        assert!(state.pending_provider_will_be_saved());

        state
            .provider_connected_with_saved_profile(
                runtime.clone(),
                vec![discovered_model("remembered-model")],
                Some(saved.clone()),
            )
            .expect("commit saved connection");

        assert_eq!(state.active_provider(), Some(&runtime));
        assert_eq!(state.saved_profile(), Some(&saved));
        assert!(state.active_provider_is_saved());
        assert!(!state.pending_provider_will_be_saved());
        assert_eq!(state.selected_model(), Some("remembered-model"));
    }

    #[test]
    fn persistent_connections_create_update_sort_and_activate_only_the_matching_profile() {
        let mut state = AppState::default();
        let beta = profile(
            "beta-provider",
            "Beta provider",
            "http://127.0.0.1:11435/v1/",
            Some("beta-model"),
        );
        state
            .restore_saved_profiles(vec![beta.clone()], Some(beta.id().clone()))
            .expect("restore beta");
        let alpha = profile(
            "alpha-provider",
            "Alpha provider",
            "http://127.0.0.1:11434/v1/",
            Some("alpha-model"),
        );
        state
            .begin_provider_connection_with_persistence(alpha.clone(), true)
            .expect("begin alpha connection");
        state
            .provider_connected_with_saved_profile(
                alpha.clone(),
                vec![discovered_model("alpha-model")],
                Some(alpha.clone()),
            )
            .expect("create alpha");

        assert_eq!(state.saved_profiles(), &[alpha.clone(), beta.clone()]);
        assert_eq!(state.selected_saved_profile(), Some(&alpha));
        assert_eq!(state.persisted_active_profile_id(), Some(alpha.id()));
        assert_eq!(state.active_saved_profile_id(), Some(alpha.id()));

        let updated_alpha = profile(
            "alpha-provider",
            "Zulu provider",
            "http://127.0.0.1:11436/v1/",
            Some("updated-model"),
        );
        state
            .begin_provider_connection_with_persistence(updated_alpha.clone(), true)
            .expect("begin alpha update");
        state
            .provider_connected_with_saved_profile(
                updated_alpha.clone(),
                vec![discovered_model("updated-model")],
                Some(updated_alpha.clone()),
            )
            .expect("update alpha");

        assert_eq!(
            state.saved_profiles(),
            &[beta.clone(), updated_alpha.clone()]
        );
        assert_eq!(state.selected_saved_profile(), Some(&updated_alpha));
        assert_eq!(
            state.persisted_active_profile_id(),
            Some(updated_alpha.id())
        );
        assert_eq!(state.active_provider(), Some(&updated_alpha));
    }

    #[test]
    fn session_connection_preserves_all_saved_profiles_and_persisted_default() {
        let mut state = AppState::default();
        let alpha = profile(
            "alpha-provider",
            "Alpha provider",
            "http://127.0.0.1:11434/v1/",
            Some("alpha-model"),
        );
        let beta = profile(
            "beta-provider",
            "Beta provider",
            "http://127.0.0.1:11435/v1/",
            Some("beta-model"),
        );
        state
            .restore_saved_profiles(vec![alpha.clone(), beta.clone()], Some(alpha.id().clone()))
            .expect("restore profiles");
        state
            .select_saved_profile(Some(beta.id()))
            .expect("select beta");
        let original_profiles = state.saved_profiles().to_vec();
        let session = profile(
            "session-provider",
            "Session provider",
            "http://127.0.0.1:11436/v1/",
            Some("session-model"),
        );

        state
            .begin_provider_connection(session.clone())
            .expect("begin session connection");
        state
            .provider_connected(session.clone(), vec![discovered_model("session-model")])
            .expect("connect session");

        assert_eq!(state.saved_profiles(), original_profiles);
        assert_eq!(state.selected_saved_profile(), Some(&beta));
        assert_eq!(state.persisted_active_profile_id(), Some(alpha.id()));
        assert!(state.active_saved_profile_id().is_none());
        assert_eq!(state.active_provider(), Some(&session));
    }

    #[test]
    fn saved_model_confirmation_updates_the_active_id_not_the_form_selection() {
        let mut state = AppState::default();
        let alpha = profile(
            "alpha-provider",
            "Alpha provider",
            "http://127.0.0.1:11434/v1/",
            Some("first-model"),
        );
        let beta = profile(
            "beta-provider",
            "Beta provider",
            "http://127.0.0.1:11435/v1/",
            Some("beta-model"),
        );
        state
            .restore_saved_profiles(vec![alpha.clone(), beta.clone()], Some(alpha.id().clone()))
            .expect("restore profiles");
        state
            .begin_provider_connection_with_persistence(alpha.clone(), true)
            .expect("begin alpha connection");
        state
            .provider_connected_with_saved_profile(
                alpha.clone(),
                vec![
                    discovered_model("first-model"),
                    discovered_model("second-model"),
                ],
                Some(alpha.clone()),
            )
            .expect("connect alpha");
        state
            .select_saved_profile(Some(beta.id()))
            .expect("display beta");
        state
            .begin_model_selection("second-model")
            .expect("begin model selection");
        let updated_alpha = alpha
            .clone()
            .with_selected_model(Some("second-model".to_owned()))
            .expect("updated alpha");

        state
            .confirm_model_selection_with_saved_profile(
                alpha.id(),
                "second-model",
                Some(updated_alpha.clone()),
            )
            .expect("confirm alpha model");

        assert_eq!(state.selected_saved_profile(), Some(&beta));
        assert_eq!(
            state
                .saved_profiles()
                .iter()
                .find(|profile| profile.id() == alpha.id()),
            Some(&updated_alpha)
        );
        assert_eq!(
            state
                .saved_profiles()
                .iter()
                .find(|profile| profile.id() == beta.id()),
            Some(&beta)
        );
        assert_eq!(state.selected_model(), Some("second-model"));
    }

    #[test]
    fn session_switch_preserves_the_restart_profile() {
        let mut state = AppState::default();
        let saved = profile(
            "saved-provider",
            "Saved provider",
            "http://127.0.0.1:11434/v1/",
            Some("saved-model"),
        );
        state
            .restore_saved_profile(saved.clone())
            .expect("restore saved profile");
        let session = profile(
            "session-provider",
            "Session provider",
            "http://127.0.0.1:11435/v1/",
            Some("session-model"),
        );

        state
            .begin_provider_connection(session.clone())
            .expect("begin session switch");
        state
            .provider_connected(session.clone(), vec![discovered_model("session-model")])
            .expect("commit session switch");

        assert_eq!(state.active_provider(), Some(&session));
        assert!(!state.active_provider_is_saved());
        assert_eq!(state.saved_profile(), Some(&saved));
        assert_eq!(
            state
                .saved_profile()
                .and_then(ProviderProfile::selected_model),
            Some("saved-model")
        );
    }

    #[test]
    fn exact_delete_correlation_preserves_runtime_and_makes_deleted_connection_session_only() {
        let mut state = AppState::default();
        let beta = profile(
            "beta-provider",
            "Beta provider",
            "http://127.0.0.1:11435/v1/",
            Some("beta-model"),
        );
        state
            .restore_saved_profiles(vec![beta.clone()], Some(beta.id().clone()))
            .expect("restore beta");
        let alpha = profile(
            "alpha-provider",
            "Alpha provider",
            "http://127.0.0.1:11434/v1/",
            Some("first-model"),
        );
        state
            .begin_provider_connection_with_persistence(alpha.clone(), true)
            .expect("begin alpha connection");
        state
            .provider_connected_with_saved_profile(
                alpha.clone(),
                vec![
                    discovered_model("first-model"),
                    discovered_model("second-model"),
                ],
                Some(alpha.clone()),
            )
            .expect("connect alpha");
        state
            .select_saved_profile(Some(beta.id()))
            .expect("display beta");
        state
            .begin_profile_deletion(alpha.id())
            .expect("begin alpha deletion");

        assert_eq!(
            state.confirm_profile_deletion(beta.id()),
            Err(StateError::UnexpectedProfileDeletion)
        );
        assert_eq!(state.pending_profile_deletion(), Some(alpha.id()));
        assert_eq!(state.saved_profiles().len(), 2);
        assert_eq!(
            state.profile_deletion_failed(
                beta.id(),
                TranslationError::new(ErrorKind::Persistence, "A stale deletion failed."),
            ),
            Err(StateError::UnexpectedProfileDeletion)
        );
        state
            .profile_deletion_failed(
                alpha.id(),
                TranslationError::new(ErrorKind::Persistence, "The profile could not be deleted."),
            )
            .expect("reject alpha deletion");
        assert_eq!(state.saved_profiles().len(), 2);
        assert!(state.active_provider_is_saved());

        state
            .begin_profile_deletion(alpha.id())
            .expect("retry alpha deletion");
        state
            .confirm_profile_deletion(alpha.id())
            .expect("delete alpha");

        assert_eq!(state.saved_profiles(), std::slice::from_ref(&beta));
        assert_eq!(state.selected_saved_profile(), Some(&beta));
        assert!(state.persisted_active_profile_id().is_none());
        assert!(state.active_saved_profile_id().is_none());
        assert!(!state.active_provider_is_saved());
        assert_eq!(state.active_provider(), Some(&alpha));
        assert_eq!(state.selected_model(), Some("first-model"));
        assert_eq!(state.status(), AppStatus::Ready);

        state
            .begin_model_selection("second-model")
            .expect("begin session model selection");
        state
            .confirm_model_selection("second-model")
            .expect("confirm session model selection");
        assert_eq!(state.saved_profiles(), &[beta]);
        state.set_source_text("Hello");
        assert_eq!(
            state.begin_translation().expect("session request").model_id,
            "second-model"
        );
    }

    #[test]
    fn unavailable_storage_rejects_persistence_but_allows_session_connection() {
        let mut state = AppState::default();
        let persistent = profile(
            "persistent-provider",
            "Persistent provider",
            "http://127.0.0.1:11434/v1/",
            None,
        );

        assert_eq!(
            state.begin_provider_connection_with_persistence(persistent.clone(), true),
            Err(StateError::ProfileStorageUnavailable)
        );
        state.profile_storage_unavailable(TranslationError::new(
            ErrorKind::Persistence,
            "Profile storage could not be opened.",
        ));
        assert_eq!(state.onboarding_stage(), OnboardingStage::Starting);
        state.mark_worker_ready();
        assert_eq!(
            state.profile_storage_status(),
            ProfileStorageStatus::Unavailable
        );
        assert_eq!(state.status(), AppStatus::Failed);
        assert_eq!(state.onboarding_stage(), OnboardingStage::ConfigureProvider);
        assert_eq!(
            state.begin_profile_deletion(persistent.id()),
            Err(StateError::ProfileStorageUnavailable)
        );

        state
            .begin_provider_connection(persistent.clone())
            .expect("begin session connection");
        state
            .provider_connected(persistent.clone(), vec![discovered_model("session-model")])
            .expect("connect session");
        assert_eq!(state.active_provider(), Some(&persistent));
        assert!(!state.active_provider_is_saved());
        assert!(state.saved_profiles().is_empty());
        assert!(state.persisted_active_profile_id().is_none());
    }

    #[test]
    fn runtime_storage_failure_downgrades_saved_connection_without_losing_the_session() {
        let mut state = state_with_profile_storage();
        state.mark_worker_ready();
        let saved = profile(
            "saved-provider",
            "Saved provider",
            "http://127.0.0.1:11434/v1/",
            Some("first-model"),
        );
        state
            .begin_provider_connection_with_persistence(saved.clone(), true)
            .expect("begin saved connection");
        state
            .provider_connected_with_saved_profile(
                saved.clone(),
                vec![
                    discovered_model("first-model"),
                    discovered_model("second-model"),
                ],
                Some(saved.clone()),
            )
            .expect("commit saved connection");
        state
            .begin_model_selection("second-model")
            .expect("begin persistent model selection");
        state
            .model_selection_failed(
                "second-model",
                TranslationError::new(
                    ErrorKind::Persistence,
                    "The saved model could not be updated.",
                ),
            )
            .expect("reject persistent model selection");

        state.profile_storage_unavailable(TranslationError::new(
            ErrorKind::Persistence,
            "Profile storage became unavailable after a write failed.",
        ));

        assert_eq!(state.status(), AppStatus::Ready);
        assert_eq!(state.active_provider(), Some(&saved));
        assert_eq!(state.selected_model(), Some("first-model"));
        assert!(state.saved_profiles().is_empty());
        assert!(!state.active_provider_is_saved());
        assert!(state.persisted_active_profile_id().is_none());
        assert_eq!(
            state.profile_storage_status(),
            ProfileStorageStatus::Unavailable
        );
        state
            .begin_model_selection("second-model")
            .expect("begin session model selection");
        state
            .confirm_model_selection("second-model")
            .expect("confirm session model selection");
        state.set_source_text("Hello");
        assert_eq!(
            state.begin_translation().expect("session request").model_id,
            "second-model"
        );
    }

    #[test]
    fn multi_profile_diagnostics_expose_only_counts_and_booleans() {
        let mut state = AppState::default();
        let alpha = profile(
            "PRIVATE_ALPHA_ID",
            "PRIVATE_ALPHA_NAME",
            "http://127.0.0.1:11434/v1/PRIVATE_ALPHA_ENDPOINT/",
            Some("PRIVATE_ALPHA_MODEL"),
        );
        let beta = profile(
            "PRIVATE_BETA_ID",
            "PRIVATE_BETA_NAME",
            "http://127.0.0.1:11435/v1/PRIVATE_BETA_ENDPOINT/",
            None,
        );
        state
            .restore_saved_profiles(vec![alpha.clone(), beta], Some(alpha.id().clone()))
            .expect("restore private profiles");

        let diagnostics = state.diagnostics_text();

        assert!(diagnostics.contains("Onboarding: Starting"));
        assert!(diagnostics.contains("Profile storage: Available"));
        assert!(diagnostics.contains("Saved profiles: 2"));
        assert!(diagnostics.contains("Persisted active profile: Yes"));
        for private_value in [
            "PRIVATE_ALPHA_ID",
            "PRIVATE_ALPHA_NAME",
            "PRIVATE_ALPHA_ENDPOINT",
            "PRIVATE_ALPHA_MODEL",
            "PRIVATE_BETA_ID",
            "PRIVATE_BETA_NAME",
            "PRIVATE_BETA_ENDPOINT",
        ] {
            assert!(!diagnostics.contains(private_value));
        }
    }

    #[test]
    fn stale_connection_result_preserves_active_and_pending_state() {
        let mut state = connected_state();
        let pending = profile(
            "pending-provider",
            "Pending provider",
            "http://127.0.0.1:11434/v1/",
            None,
        );
        state
            .begin_provider_connection(pending.clone())
            .expect("begin connection");
        let stale_profile = profile(
            "stale-provider",
            "Stale provider",
            "http://127.0.0.1:11435/v1/",
            Some("stale-model"),
        );

        assert_eq!(
            state.provider_connected(stale_profile, vec![discovered_model("stale-model")]),
            Err(StateError::UnexpectedProviderConnection)
        );

        assert_eq!(state.status(), AppStatus::Connecting);
        assert_eq!(
            state.provider_id().map(ProviderProfileId::as_str),
            Some("local-fake-provider")
        );
        assert_eq!(state.models(), &[discovered_model("fake-translator")]);
        assert_eq!(state.selected_model(), Some("fake-translator"));
        assert_eq!(state.pending_provider(), Some(&pending));
    }

    #[test]
    fn stale_connection_failure_preserves_active_and_pending_state() {
        let mut state = connected_state();
        let pending = profile(
            "pending-provider",
            "Pending provider",
            "http://127.0.0.1:11434/v1/",
            None,
        );
        state
            .begin_provider_connection(pending.clone())
            .expect("begin connection");
        let stale_profile = profile(
            "stale-provider",
            "Stale provider",
            "http://127.0.0.1:11435/v1/",
            None,
        );

        assert_eq!(
            state.provider_connection_failed(
                &stale_profile,
                TranslationError::new(ErrorKind::Network, "A stale connection failed."),
            ),
            Err(StateError::UnexpectedProviderConnection)
        );

        assert_eq!(state.status(), AppStatus::Connecting);
        assert_eq!(
            state.provider_id().map(ProviderProfileId::as_str),
            Some("local-fake-provider")
        );
        assert_eq!(state.models(), &[discovered_model("fake-translator")]);
        assert_eq!(state.selected_model(), Some("fake-translator"));
        assert_eq!(state.pending_provider(), Some(&pending));
        assert_eq!(state.error_text(), None);
    }

    #[test]
    fn same_id_different_endpoint_failure_is_stale_and_exact_failure_clears_save_intent() {
        let mut state = connected_state();
        let pending = profile(
            "shared-provider-id",
            "Pending provider",
            "http://127.0.0.1:11434/v1/",
            None,
        );
        state
            .begin_provider_connection_with_persistence(pending.clone(), true)
            .expect("begin saved connection");
        let mismatched_profile = profile(
            "shared-provider-id",
            "Pending provider",
            "http://127.0.0.1:11435/v1/",
            None,
        );

        assert_eq!(
            state.provider_connection_failed(
                &mismatched_profile,
                TranslationError::new(ErrorKind::Network, "A stale connection failed."),
            ),
            Err(StateError::UnexpectedProviderConnection)
        );
        assert_eq!(state.pending_provider(), Some(&pending));
        assert!(state.pending_provider_will_be_saved());

        state
            .provider_connection_failed(
                &pending,
                TranslationError::new(ErrorKind::Network, "The connection failed."),
            )
            .expect("reject exact connection");
        assert!(state.pending_provider().is_none());
        assert!(!state.pending_provider_will_be_saved());
        assert_eq!(state.status(), AppStatus::Ready);
    }

    #[test]
    fn saved_model_confirmation_requires_an_exact_persisted_counterpart() {
        let mut state = state_with_profile_storage();
        let runtime = profile_with_secret(
            "saved-provider",
            "Saved provider",
            "http://127.0.0.1:11434/v1/",
            Some("first-model"),
            Some(SecretRef::new(SecretRefNamespace::Session)),
        );
        let saved = profile(
            "saved-provider",
            "Saved provider",
            "http://127.0.0.1:11434/v1/",
            Some("first-model"),
        );
        state
            .begin_provider_connection_with_persistence(runtime.clone(), true)
            .expect("begin saved connection");
        state
            .provider_connected_with_saved_profile(
                runtime,
                vec![
                    discovered_model("first-model"),
                    discovered_model("second-model"),
                ],
                Some(saved.clone()),
            )
            .expect("commit saved connection");
        state
            .begin_model_selection("second-model")
            .expect("begin model selection");

        assert_eq!(
            state.confirm_model_selection_with_saved_profile(
                saved.id(),
                "second-model",
                Some(saved.clone()),
            ),
            Err(StateError::InvalidSavedProfile)
        );
        assert_eq!(state.selected_model(), Some("first-model"));
        assert_eq!(state.pending_model_selection(), Some("second-model"));
        assert_eq!(state.saved_profile(), Some(&saved));

        let updated_saved = saved
            .clone()
            .with_selected_model(Some("second-model".to_owned()))
            .expect("updated saved model");
        state
            .confirm_model_selection_with_saved_profile(
                saved.id(),
                "second-model",
                Some(updated_saved.clone()),
            )
            .expect("confirm saved model");

        assert_eq!(state.selected_model(), Some("second-model"));
        assert_eq!(state.pending_model_selection(), None);
        assert_eq!(state.saved_profile(), Some(&updated_saved));
        assert_eq!(
            state
                .active_provider()
                .and_then(ProviderProfile::selected_model),
            Some("second-model")
        );
        assert!(state.active_provider_is_saved());
    }

    #[test]
    fn unavailable_saved_model_requires_a_new_deliberate_selection() {
        let mut state = state_with_profile_storage();
        let profile = profile(
            "saved-provider",
            "Saved provider",
            "http://127.0.0.1:11434/v1/",
            Some("removed-model"),
        );
        state
            .begin_provider_connection_with_persistence(profile.clone(), true)
            .expect("begin connection");
        let connected = profile
            .with_selected_model(None)
            .expect("normalized provider profile");
        state
            .provider_connected_with_saved_profile(
                connected.clone(),
                vec![discovered_model("new-model")],
                Some(connected.clone()),
            )
            .expect("provider connected");
        state.set_source_text("Hello");

        assert_eq!(state.selected_model(), None);
        assert_eq!(state.active_provider(), Some(&connected));
        assert_eq!(state.saved_profile(), Some(&connected));
        assert_eq!(state.begin_translation(), Err(StateError::MissingModel));
        state.select_model("new-model").expect("select model");
        assert_eq!(
            state.begin_translation().expect("request").model_id,
            "new-model"
        );
    }

    #[test]
    fn failed_connection_preserves_previous_profile_and_models() {
        let mut state = connected_state();
        state.mark_worker_ready();
        let unavailable = profile(
            "unavailable",
            "Unavailable provider",
            "http://127.0.0.1:9/v1/",
            None,
        );
        state
            .begin_provider_connection(unavailable.clone())
            .expect("begin connection");
        assert_eq!(state.onboarding_stage(), OnboardingStage::Connecting);

        state
            .provider_connection_failed(
                &unavailable,
                TranslationError::new(ErrorKind::Network, "The provider could not be reached."),
            )
            .expect("provider failure");

        assert_eq!(state.status(), AppStatus::Ready);
        assert_eq!(state.onboarding_stage(), OnboardingStage::Ready);
        assert_eq!(
            state.provider_id().map(ProviderProfileId::as_str),
            Some("local-fake-provider")
        );
        assert_eq!(state.selected_model(), Some("fake-translator"));
        assert_eq!(state.models().len(), 1);
        assert!(state.pending_provider().is_none());
        assert_eq!(
            state.error_text().as_deref(),
            Some("Network: The provider could not be reached.")
        );
        let request = state
            .begin_translation()
            .expect("previous provider request");
        assert_eq!(request.model_id, "fake-translator");
    }

    #[test]
    fn provider_change_is_rejected_while_translation_is_active() {
        let mut state = connected_state();
        state.begin_translation().expect("request");

        let result = state.begin_provider_connection(profile(
            "local-session",
            "Local session provider",
            "http://127.0.0.1:11434/v1/",
            None,
        ));

        assert_eq!(result, Err(StateError::Busy));
        assert_eq!(
            state.provider_id().map(ProviderProfileId::as_str),
            Some("local-fake-provider")
        );
        assert!(state.pending_provider().is_none());
    }

    #[test]
    fn disabled_profile_is_rejected_before_connection() {
        let mut state = connected_state();
        let disabled = profile(
            "disabled-provider",
            "Disabled provider",
            "http://127.0.0.1:11434/v1/",
            None,
        )
        .with_enabled(false);

        let result = state.begin_provider_connection(disabled);

        assert_eq!(result, Err(StateError::InvalidProfile));
        assert_eq!(state.status(), AppStatus::Ready);
        assert_eq!(
            state.provider_id().map(ProviderProfileId::as_str),
            Some("local-fake-provider")
        );
        assert!(state.pending_provider().is_none());
    }

    #[test]
    fn rejected_connection_does_not_corrupt_active_translation() {
        let mut state = connected_state();
        state.begin_translation().expect("request");
        state
            .apply_translation_event(TranslationEvent::Started { sequence: 0 })
            .expect("started");

        state.provider_failed(TranslationError::new(
            ErrorKind::Internal,
            "A provider cannot be changed while a translation is running.",
        ));

        assert_eq!(state.status(), AppStatus::Translating);
        assert_eq!(
            state.provider_id().map(ProviderProfileId::as_str),
            Some("local-fake-provider")
        );
        state
            .apply_translation_event(TranslationEvent::Completed { sequence: 1 })
            .expect("completed");
        assert_eq!(state.status(), AppStatus::Completed);
    }

    #[test]
    fn operation_failure_terminates_state_and_retains_partial_output() {
        let mut state = connected_state();
        state.begin_translation().expect("request");
        state
            .apply_translation_event(TranslationEvent::Started { sequence: 0 })
            .expect("started");
        state
            .apply_translation_event(TranslationEvent::TextDelta {
                sequence: 1,
                text: "partial".to_owned(),
            })
            .expect("delta");

        state.record_operation_failure(TranslationError::new(
            ErrorKind::Internal,
            "The core event stream ended without a terminal event.",
        ));

        assert_eq!(state.status(), AppStatus::Failed);
        assert_eq!(state.output(), "partial");
        assert!(state.has_partial_output());
        assert_eq!(
            state.error_text().as_deref(),
            Some("Internal: The core event stream ended without a terminal event.")
        );
    }

    #[test]
    fn diagnostics_omit_endpoint_source_model_and_secret_reference() {
        let mut state = connected_state();
        state.mark_worker_ready();
        state.set_source_text("SOURCE_SENTINEL");
        let secret_ref = SecretRef::new(SecretRefNamespace::Session);
        let secret_reference = secret_ref.as_str().to_owned();
        let runtime_profile = ProviderProfile::new(
            ProviderProfileId::parse("session-provider").expect("profile ID"),
            "Session provider",
            "openai-compatible",
            "openai_chat_completions",
            "http://127.0.0.1:11434/v1/ENDPOINT_SENTINEL/",
            Some(secret_ref),
        )
        .expect("profile")
        .with_selected_model(Some("MODEL_SENTINEL".to_owned()))
        .expect("selected model");
        let saved_profile = profile(
            "session-provider",
            "Session provider",
            "http://127.0.0.1:11434/v1/ENDPOINT_SENTINEL/",
            Some("MODEL_SENTINEL"),
        );
        state
            .begin_provider_connection_with_persistence(runtime_profile.clone(), true)
            .expect("begin connection");
        state
            .provider_connected_with_saved_profile(
                runtime_profile,
                vec![discovered_model("MODEL_SENTINEL")],
                Some(saved_profile),
            )
            .expect("provider connected");

        let diagnostics = state.diagnostics_text();

        assert!(diagnostics.contains("Onboarding: Ready"));
        assert!(diagnostics.contains("Provider: Yes"));
        assert!(diagnostics.contains("Provider saved: Yes"));
        assert!(diagnostics.contains("Saved profile: Yes"));
        assert!(diagnostics.contains("Saved model: Yes"));
        assert!(diagnostics.contains("Model selected: Yes"));
        assert!(!diagnostics.contains("session-provider"));
        assert!(!diagnostics.contains("127.0.0.1"));
        assert!(!diagnostics.contains("ENDPOINT_SENTINEL"));
        assert!(!diagnostics.contains("SOURCE_SENTINEL"));
        assert!(!diagnostics.contains("MODEL_SENTINEL"));
        assert!(!diagnostics.contains(&secret_reference));
    }
}
