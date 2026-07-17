use linguamesh_domain::{
    ErrorKind, ModelDescriptor, TranslationError, TranslationEvent, TranslationRequest,
};
pub use linguamesh_domain::{ProviderProfile, ProviderProfileId};
use linguamesh_protocol::PROTOCOL_VERSION;
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
    /// 记录简体中文偏好并等待运行时本地化接入。
    SimplifiedChinese,
}

impl UiLocale {
    /// 返回界面中显示的名称。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::SimplifiedChinese => "Simplified Chinese",
        }
    }

    /// 返回稳定的语言标签。
    #[must_use]
    pub const fn language_tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::SimplifiedChinese => "zh-CN",
        }
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

/// 保存与工具包无关的原生界面状态。
#[derive(Clone)]
pub struct AppState {
    active_provider: Option<ProviderProfile>,
    pending_provider: Option<ProviderProfile>,
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
            active_provider: None,
            pending_provider: None,
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
    /// 返回当前状态。
    #[must_use]
    pub const fn status(&self) -> AppStatus {
        self.status
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

    /// 开始连接一个已验证的规范提供商配置。
    pub fn begin_provider_connection(
        &mut self,
        profile: ProviderProfile,
    ) -> Result<(), StateError> {
        if !profile.enabled() {
            return Err(StateError::InvalidProfile);
        }
        if matches!(self.status, AppStatus::Translating | AppStatus::Cancelling) {
            return Err(StateError::Busy);
        }
        if self.pending_model_selection.is_some() {
            return Err(StateError::ModelSelectionPending);
        }
        if self.status == AppStatus::Connecting {
            return Err(StateError::Connecting);
        }
        self.pending_provider = Some(profile);
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
        if self.pending_provider.as_ref() != Some(&profile) {
            return Err(StateError::UnexpectedProviderConnection);
        }

        let selected_model = profile
            .selected_model()
            .filter(|selected| models.iter().any(|model| model.id == *selected))
            .map(str::to_owned);

        self.active_provider = Some(profile);
        self.pending_provider = None;
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
        profile_id: &ProviderProfileId,
        error: TranslationError,
    ) -> Result<(), StateError> {
        if self.pending_provider.as_ref().map(ProviderProfile::id) != Some(profile_id) {
            return Err(StateError::UnexpectedProviderConnection);
        }
        self.provider_failed(error);
        Ok(())
    }

    /// 记录连接失败并保留上一个可用配置。
    pub fn provider_failed(&mut self, error: TranslationError) {
        self.pending_provider = None;
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
        if self.pending_model_selection.as_deref() != Some(model_id) {
            return Err(StateError::UnexpectedModelSelection);
        }
        self.select_model(model_id)
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

    /// 构建不包含源文本或凭据的诊断摘要。
    #[must_use]
    pub fn diagnostics_text(&self) -> String {
        format!(
            "Core protocol: {PROTOCOL_VERSION}\nProvider: {}\nModel selected: {}\nModel selection pending: {}\nStatus: {}\nTheme: {}\nLocale: {}\nOutput bytes: {}",
            self.active_provider
                .as_ref()
                .map_or("None", |profile| profile.id().as_str()),
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
            self.status.label(),
            self.theme.label(),
            self.locale.language_tag(),
            self.output.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppState, AppStatus, ProviderProfile, ProviderProfileId, StateError, ThemePreference,
        UiLocale,
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
        ProviderProfile::new(
            ProviderProfileId::parse(id).expect("profile ID"),
            display_name,
            "openai-compatible",
            "openai_chat_completions",
            endpoint,
            None,
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
        state.select_model("fake-translator").expect("select model");
        state.set_source_text("Hello");
        state
    }

    #[test]
    fn default_state_has_no_active_or_pending_provider() {
        let mut state = AppState::default();

        assert_eq!(state.status(), AppStatus::Disconnected);
        assert!(state.active_provider().is_none());
        assert!(state.provider_id().is_none());
        assert!(state.pending_provider().is_none());
        assert!(state.models().is_empty());
        assert_eq!(state.selected_model(), None);
        assert_eq!(state.pending_model_selection(), None);
        assert!(state.diagnostics_text().contains("Provider: None"));

        state.set_source_text("Hello");
        assert_eq!(state.begin_translation(), Err(StateError::MissingProvider));
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
        state.models.push(discovered_model("fake-slow-translator"));

        state
            .begin_model_selection("fake-slow-translator")
            .expect("begin model selection");

        assert_eq!(
            state.pending_model_selection(),
            Some("fake-slow-translator")
        );
        assert_eq!(state.selected_model(), Some("fake-translator"));
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
        assert!(diagnostics.contains("Theme: Dark"));
        assert!(diagnostics.contains("Locale: zh-CN"));
        assert!(!diagnostics.contains("Hello"));
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
        let stale_id = ProviderProfileId::parse("stale-provider").expect("stale profile ID");

        assert_eq!(
            state.provider_connection_failed(
                &stale_id,
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
    fn unavailable_saved_model_requires_a_new_deliberate_selection() {
        let mut state = AppState::default();
        let profile = profile(
            "saved-provider",
            "Saved provider",
            "http://127.0.0.1:11434/v1/",
            Some("removed-model"),
        );
        state
            .begin_provider_connection(profile.clone())
            .expect("begin connection");
        state
            .provider_connected(profile, vec![discovered_model("new-model")])
            .expect("provider connected");
        state.set_source_text("Hello");

        assert_eq!(state.selected_model(), None);
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
        let unavailable = profile(
            "unavailable",
            "Unavailable provider",
            "http://127.0.0.1:9/v1/",
            None,
        );
        state
            .begin_provider_connection(unavailable.clone())
            .expect("begin connection");

        state
            .provider_connection_failed(
                unavailable.id(),
                TranslationError::new(ErrorKind::Network, "The provider could not be reached."),
            )
            .expect("provider failure");

        assert_eq!(state.status(), AppStatus::Ready);
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
        state.set_source_text("SOURCE_SENTINEL");
        let secret_ref = SecretRef::new(SecretRefNamespace::SecretService);
        let secret_reference = secret_ref.as_str().to_owned();
        let profile = ProviderProfile::new(
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
        state
            .begin_provider_connection(profile.clone())
            .expect("begin connection");
        state
            .provider_connected(profile, vec![discovered_model("MODEL_SENTINEL")])
            .expect("provider connected");

        let diagnostics = state.diagnostics_text();

        assert!(diagnostics.contains("Provider: session-provider"));
        assert!(diagnostics.contains("Model selected: Yes"));
        assert!(!diagnostics.contains("127.0.0.1"));
        assert!(!diagnostics.contains("ENDPOINT_SENTINEL"));
        assert!(!diagnostics.contains("SOURCE_SENTINEL"));
        assert!(!diagnostics.contains("MODEL_SENTINEL"));
        assert!(!diagnostics.contains(&secret_reference));
    }
}
