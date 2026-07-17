use linguamesh_domain::{
    ErrorKind, ModelDescriptor, TranslationError, TranslationEvent, TranslationRequest,
};
use linguamesh_protocol::PROTOCOL_VERSION;
use std::error::Error;
use std::fmt;

/// 当前检查点提供的本地提供商标识。
pub const LOCAL_FAKE_PROVIDER_ID: &str = "local-fake-provider";

/// 当前检查点提供的内建提供商端点标识。
pub const LOCAL_FAKE_PROVIDER_ENDPOINT: &str = "embedded://fake-provider";

/// 描述不含凭据的会话内提供商配置。
#[derive(Clone, Eq, PartialEq)]
pub struct ProviderProfile {
    id: String,
    display_name: String,
    endpoint: String,
}

impl ProviderProfile {
    /// 创建不含凭据的提供商配置。
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        display_name: impl Into<String>,
        endpoint: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            endpoint: endpoint.into(),
        }
    }

    /// 创建开发模式的内建假提供商配置。
    #[must_use]
    pub fn local_fake() -> Self {
        Self::new(
            LOCAL_FAKE_PROVIDER_ID,
            "Local fake provider",
            LOCAL_FAKE_PROVIDER_ENDPOINT,
        )
    }

    /// 返回稳定提供商标识。
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 返回用户可见名称。
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 返回仅用于连接的端点。
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
}

/// 描述原生客户端的可见操作状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppStatus {
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
    /// 提供商配置缺少必需字段。
    InvalidProfile,
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
            Self::InvalidProfile => {
                formatter.write_str("Provider ID, display name, and endpoint must not be empty.")
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
    active_provider: ProviderProfile,
    pending_provider: Option<ProviderProfile>,
    models: Vec<ModelDescriptor>,
    selected_model: Option<String>,
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
            active_provider: ProviderProfile::local_fake(),
            pending_provider: None,
            models: Vec::new(),
            selected_model: None,
            source_text: String::new(),
            source_locale: None,
            target_locale: "zh-CN".to_owned(),
            output: String::new(),
            partial_output: false,
            status: AppStatus::Connecting,
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
    pub fn provider_id(&self) -> &str {
        self.active_provider.id()
    }

    /// 返回当前活动提供商。
    #[must_use]
    pub const fn active_provider(&self) -> &ProviderProfile {
        &self.active_provider
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

    /// 开始连接一个不含凭据的提供商。
    pub fn begin_provider_connection(
        &mut self,
        profile: ProviderProfile,
    ) -> Result<(), StateError> {
        if profile.id().trim().is_empty()
            || profile.display_name().trim().is_empty()
            || profile.endpoint().trim().is_empty()
        {
            return Err(StateError::InvalidProfile);
        }
        if matches!(self.status, AppStatus::Translating | AppStatus::Cancelling) {
            return Err(StateError::Busy);
        }
        if self.status == AppStatus::Connecting {
            return Err(StateError::Connecting);
        }
        self.pending_provider = Some(profile);
        self.status = AppStatus::Connecting;
        self.error = None;
        Ok(())
    }

    /// 仅在发现成功后原子提交提供商和模型。
    pub fn provider_connected(&mut self, models: Vec<ModelDescriptor>) {
        if let Some(profile) = self.pending_provider.take() {
            self.active_provider = profile;
        }
        self.selected_model = models.first().map(|model| model.id.clone());
        self.models = models;
        self.status = AppStatus::Ready;
        self.error = None;
    }

    /// 记录连接失败并保留上一个可用配置。
    pub fn provider_failed(&mut self, error: TranslationError) {
        self.pending_provider = None;
        if !matches!(self.status, AppStatus::Translating | AppStatus::Cancelling) {
            self.status = if self.models.is_empty() {
                AppStatus::Failed
            } else {
                AppStatus::Ready
            };
        }
        self.error = Some(error);
    }

    /// 选择已发现模型。
    pub fn select_model(&mut self, model_id: &str) -> Result<(), StateError> {
        if self.models.iter().any(|model| model.id == model_id) {
            self.selected_model = Some(model_id.to_owned());
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
                ErrorKind::Internal => "Internal",
            };
            format!("{category}: {}", error.message)
        })
    }

    /// 构建不包含源文本或凭据的诊断摘要。
    #[must_use]
    pub fn diagnostics_text(&self) -> String {
        format!(
            "Core protocol: {PROTOCOL_VERSION}\nProvider: {}\nModel: {}\nStatus: {}\nTheme: {}\nLocale: {}\nOutput bytes: {}",
            self.active_provider.id(),
            self.selected_model.as_deref().unwrap_or("None"),
            self.status.label(),
            self.theme.label(),
            self.locale.language_tag(),
            self.output.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, AppStatus, ProviderProfile, StateError, ThemePreference, UiLocale};
    use linguamesh_domain::{
        ErrorKind, ModelDescriptor, ModelSource, TranslationError, TranslationEvent,
    };

    fn connected_state() -> AppState {
        let mut state = AppState::default();
        state.provider_connected(vec![ModelDescriptor {
            id: "fake-translator".to_owned(),
            display_name: "Fake Translator".to_owned(),
            source: ModelSource::Discovered,
        }]);
        state.set_source_text("Hello");
        state
    }

    #[test]
    fn discovered_model_is_selected_and_request_uses_state() {
        let mut state = connected_state();
        state.set_source_locale(Some("en".to_owned()));
        let request = state.begin_translation().expect("request");
        assert_eq!(request.model_id, "fake-translator");
        assert_eq!(request.source_locale.as_deref(), Some("en"));
        assert_eq!(request.target_locale, "zh-CN");
        assert_eq!(state.status(), AppStatus::Translating);
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
        let previous_provider = state.provider_id().to_owned();
        let next_profile = ProviderProfile::new(
            "local-session",
            "Local session provider",
            "http://127.0.0.1:11434/v1/",
        );

        state
            .begin_provider_connection(next_profile)
            .expect("begin connection");

        assert_eq!(state.status(), AppStatus::Connecting);
        assert_eq!(state.provider_id(), previous_provider);
        assert_eq!(state.models()[0].id, "fake-translator");
        assert_eq!(
            state.pending_provider().map(ProviderProfile::id),
            Some("local-session")
        );
        assert_eq!(state.begin_translation(), Err(StateError::Connecting));

        state.provider_connected(vec![ModelDescriptor {
            id: "local-model".to_owned(),
            display_name: "Local Model".to_owned(),
            source: ModelSource::Discovered,
        }]);

        assert_eq!(state.status(), AppStatus::Ready);
        assert_eq!(state.provider_id(), "local-session");
        assert_eq!(state.selected_model(), Some("local-model"));
        assert!(state.pending_provider().is_none());
    }

    #[test]
    fn failed_connection_preserves_previous_profile_and_models() {
        let mut state = connected_state();
        state
            .begin_provider_connection(ProviderProfile::new(
                "unavailable",
                "Unavailable provider",
                "http://127.0.0.1:9/v1/",
            ))
            .expect("begin connection");

        state.provider_failed(TranslationError::new(
            ErrorKind::Network,
            "The provider could not be reached.",
        ));

        assert_eq!(state.status(), AppStatus::Ready);
        assert_eq!(state.provider_id(), "local-fake-provider");
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

        let result = state.begin_provider_connection(ProviderProfile::new(
            "local-session",
            "Local session provider",
            "http://127.0.0.1:11434/v1/",
        ));

        assert_eq!(result, Err(StateError::Busy));
        assert_eq!(state.provider_id(), "local-fake-provider");
        assert!(state.pending_provider().is_none());
    }

    #[test]
    fn invalid_profile_is_rejected_before_connection() {
        let mut state = connected_state();

        let result = state.begin_provider_connection(ProviderProfile::new(
            " ",
            "Incomplete provider",
            "http://127.0.0.1:11434/v1/",
        ));

        assert_eq!(result, Err(StateError::InvalidProfile));
        assert_eq!(state.status(), AppStatus::Ready);
        assert_eq!(state.provider_id(), "local-fake-provider");
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
        assert_eq!(state.provider_id(), "local-fake-provider");
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
    fn diagnostics_omit_endpoint_source_and_secret_sentinel() {
        let mut state = connected_state();
        state.set_source_text("SOURCE_SENTINEL");
        state
            .begin_provider_connection(ProviderProfile::new(
                "session-provider",
                "Session provider",
                "http://127.0.0.1:11434/v1/SECRET_SENTINEL",
            ))
            .expect("begin connection");
        state.provider_connected(vec![ModelDescriptor {
            id: "session-model".to_owned(),
            display_name: "Session Model".to_owned(),
            source: ModelSource::Discovered,
        }]);

        let diagnostics = state.diagnostics_text();

        assert!(diagnostics.contains("Provider: session-provider"));
        assert!(!diagnostics.contains("127.0.0.1"));
        assert!(!diagnostics.contains("SOURCE_SENTINEL"));
        assert!(!diagnostics.contains("SECRET_SENTINEL"));
    }
}
