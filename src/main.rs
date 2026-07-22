use adw::prelude::*;
use gtk::glib;
use linguamesh_document::{
    DEFAULT_SUBTITLE_MAX_LINE_CHARS, DEFAULT_SUBTITLE_MAX_READING_SPEED, DocumentFormat,
    DocumentJobState, DocumentSegmentKind, DocumentWarning, DocumentWarningKind,
};
use linguamesh_domain::{
    ErrorKind, FileLease, Glossary, GlossaryEntry, MAX_GLOSSARY_CSV_BYTES,
    MAX_ROUTING_IDENTIFIER_BYTES, MAX_ROUTING_PROFILE_JSON_BYTES, OperationId, ProviderProfileId,
    RoutingCandidate, RoutingConstraints, RoutingMode, RoutingPreference, RoutingProfile,
    SecretRef, SecretRefNamespace, SecretValue, TranslationError, TranslationEvent,
    TranslationPreset, TranslationPrivacyMode, TranslationQualityMode, UsageRecord, UsageSource,
};
use linguamesh_engine::core_compatibility;
use linguamesh_linux::file_import;
use linguamesh_linux::localization;
use linguamesh_linux::model::{
    AppState, AppStatus, OnboardingStage, ProfileStorageStatus, ProviderProfile,
    RoutingDecisionSummary, StateError, ThemePreference, UiLocale, move_routing_profile_id,
    move_routing_profile_id_before, ordered_routing_profile_ids, routing_mode_for_selection,
};
use linguamesh_linux::secret_service;
use linguamesh_linux::worker::{
    CoreWorker, PersistenceIntent, WorkerCommand, WorkerCommandHandle, WorkerEvent,
};
use linguamesh_provider_catalog::{ProviderCatalog, ProviderPreset};
use linguamesh_storage::{
    DocumentJobSnapshot, RoutingProfileRecord, TranslationHistoryEntry, TranslationMemoryEntry,
};
use std::cell::{Cell, RefCell};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

const SOURCE_LOCALES: [Option<&str>; 3] = [None, Some("en"), Some("zh-CN")];
const TARGET_LOCALES: [&str; 3] = ["zh-CN", "en", "ja"];
const MAX_EVENTS_PER_TICK: usize = 64;
const PROFILE_ID_GENERATION_ATTEMPTS: usize = 8;
const CUSTOM_PROVIDER_PRESET_ID: &str = "custom-openai-compatible";
const OLLAMA_PROVIDER_PRESET_ID: &str = "ollama";
const ANTHROPIC_PROVIDER_PRESET_ID: &str = "anthropic";
const GEMINI_PROVIDER_PRESET_ID: &str = "gemini";
const AZURE_PROVIDER_PRESET_ID: &str = "azure-openai";
const RESPONSES_PROVIDER_PRESET_ID: &str = "openai-responses";
const OPENAI_ADAPTER_TYPE: &str = "openai_chat_completions";
const OLLAMA_ADAPTER_TYPE: &str = "ollama_chat";
const ANTHROPIC_ADAPTER_TYPE: &str = "anthropic_messages";
const GEMINI_ADAPTER_TYPE: &str = "gemini_generate_content";
const AZURE_ADAPTER_TYPE: &str = "azure_openai_chat";
const RESPONSES_ADAPTER_TYPE: &str = "openai_responses";
const DEFAULT_PROVIDER_NAME: &str = "Local OpenAI-compatible provider";
const DEFAULT_OLLAMA_PROVIDER_NAME: &str = "Local Ollama provider";
const DEFAULT_ANTHROPIC_PROVIDER_NAME: &str = "Anthropic Messages provider";
const DEFAULT_GEMINI_PROVIDER_NAME: &str = "Google Gemini provider";
const DEFAULT_AZURE_PROVIDER_NAME: &str = "Azure OpenAI provider";
const DEFAULT_RESPONSES_PROVIDER_NAME: &str = "OpenAI Responses provider";
const DEFAULT_PROVIDER_ENDPOINT: &str = "http://127.0.0.1:11434/v1/";
const DEFAULT_OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434/api/";
const DEFAULT_ANTHROPIC_ENDPOINT: &str = "https://api.anthropic.com/v1/";
const DEFAULT_GEMINI_ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta/";
const DEFAULT_AZURE_ENDPOINT: &str = "https://resource.openai.azure.com/";
const DEFAULT_RESPONSES_ENDPOINT: &str = "https://api.openai.com/v1/";

// 返回核心目录中与 Linux 下拉框位置对应的稳定预设标识。
fn catalog_preset_id(index: u32) -> &'static str {
    match index {
        1 => OLLAMA_PROVIDER_PRESET_ID,
        2 => ANTHROPIC_PROVIDER_PRESET_ID,
        3 => GEMINI_PROVIDER_PRESET_ID,
        4 => AZURE_PROVIDER_PRESET_ID,
        5 => RESPONSES_PROVIDER_PRESET_ID,
        _ => "generic-openai-compatible",
    }
}

// 只缓存编译进核心的无秘密提供商目录，避免每次刷新控件时重复解析 JSON。
fn bundled_provider_catalog() -> Option<&'static ProviderCatalog> {
    static CATALOG: OnceLock<Option<ProviderCatalog>> = OnceLock::new();
    CATALOG
        .get_or_init(|| ProviderCatalog::bundled().ok())
        .as_ref()
}

// 返回指定 Linux 预设对应的核心目录条目。
fn catalog_preset(index: u32) -> Option<&'static ProviderPreset> {
    let id = catalog_preset_id(index);
    bundled_provider_catalog()?
        .providers
        .iter()
        .find(|preset| preset.id == id)
}

// 检查 Linux 映射的适配器和模型发现策略是否仍与核心目录一致。
fn validate_provider_preset_catalog() -> Result<(), String> {
    for index in 0..6 {
        let (_, adapter, _, _) = provider_preset_config(index);
        let preset = catalog_preset(index).ok_or_else(|| {
            format!(
                "missing provider catalog preset: {}",
                catalog_preset_id(index)
            )
        })?;
        if preset.adapter != adapter {
            return Err(format!(
                "provider catalog adapter mismatch for {}: expected {}, received {}",
                preset.id, adapter, preset.adapter
            ));
        }
    }
    Ok(())
}

#[derive(Clone)]
struct UiBindings {
    application: adw::Application,
    window: adw::ApplicationWindow,
    workspace: gtk::Box,
    onboarding: gtk::Box,
    onboarding_title: gtk::Label,
    onboarding_detail: gtk::Label,
    provider_title: gtk::Label,
    provider_note: gtk::Label,
    saved_profile: gtk::DropDown,
    provider_preset: gtk::DropDown,
    provider_name: gtk::Entry,
    provider_endpoint: gtk::Entry,
    manual_model_row: gtk::Box,
    manual_model: gtk::Entry,
    provider_credential: gtk::PasswordEntry,
    remember_profile: gtk::CheckButton,
    remove_saved_profile: gtk::Button,
    connect: gtk::Button,
    test_connection: gtk::Button,
    active_provider: gtk::Label,
    model: gtk::DropDown,
    source_locale: gtk::DropDown,
    target_locale: gtk::DropDown,
    quality_mode: gtk::DropDown,
    translation_preset: gtk::DropDown,
    glossary: gtk::Entry,
    import_glossary: gtk::Button,
    export_glossary: gtk::Button,
    incognito: gtk::CheckButton,
    history_enabled: gtk::CheckButton,
    history: gtk::Button,
    clear_history: gtk::Button,
    memory_enabled: gtk::CheckButton,
    memory: gtk::Button,
    clear_memory: gtk::Button,
    routing_profiles: gtk::Button,
    fallback_enabled: gtk::CheckButton,
    fallback_profile_label: gtk::Label,
    fallback_profile: gtk::DropDown,
    theme: gtk::DropDown,
    locale: gtk::DropDown,
    source: gtk::TextBuffer,
    output: gtk::TextBuffer,
    source_view: gtk::TextView,
    output_view: gtk::TextView,
    source_label: gtk::Label,
    output_label: gtk::Label,
    source_metrics: gtk::Label,
    output_metrics: gtk::Label,
    translate: gtk::Button,
    retry_translation: gtk::Button,
    export_output: gtk::Button,
    open_output: gtk::Button,
    open_source: gtk::Button,
    ocr_enabled: gtk::CheckButton,
    document_jobs: gtk::Button,
    stop: gtk::Button,
    pause_document: gtk::Button,
    resume_document: gtk::Button,
    retry_document: gtk::Button,
    status: gtk::Label,
    progress: gtk::ProgressBar,
    partial: gtk::Label,
    error: gtk::Label,
    locale_note: gtk::Label,
    diagnostics_panel: gtk::Expander,
    diagnostics: gtk::Label,
    profile_selection_guard: Rc<Cell<bool>>,
    provider_preset_guard: Rc<Cell<bool>>,
    provider_preset_previous: Rc<Cell<u32>>,
    draft_profile_id: Rc<RefCell<Option<ProviderProfileId>>>,
    source_uri: Rc<RefCell<Option<String>>>,
    output_uri: Rc<RefCell<Option<String>>>,
    fallback_profile_ids: Rc<RefCell<Vec<Option<ProviderProfileId>>>>,
    selected_routing_profile_id: Rc<RefCell<Option<String>>>,
    document_job_id: Rc<RefCell<Option<String>>>,
    document_job_guard: Rc<Cell<bool>>,
    document_job_state: Rc<Cell<Option<DocumentJobState>>>,
    document_progress: Rc<Cell<Option<(usize, usize)>>>,
    document_warnings: Rc<RefCell<Vec<DocumentWarning>>>,
    ocr_pending: Rc<Cell<bool>>,
    connection_test_notice: Rc<Cell<bool>>,
    connection_test_model_count: Rc<Cell<Option<usize>>>,
    connection_test_profile_id: Rc<RefCell<Option<String>>>,
    export_notice: Rc<Cell<bool>>,
    report_export_notice: Rc<Cell<bool>>,
    fallback_notice: Rc<Cell<bool>>,
    fallback_approval: Rc<Cell<bool>>,
    glossary_notice: Rc<Cell<bool>>,
    glossary_from_csv: Rc<Cell<bool>>,
    history_notice: Rc<Cell<bool>>,
    history_export_notice: Rc<Cell<bool>>,
    history_warning: Rc<Cell<bool>>,
    history_clear_pending: Rc<Cell<bool>>,
    history_policy_guard: Rc<Cell<bool>>,
    history_policy_pending: Rc<Cell<bool>>,
    history_policy_notice: Rc<Cell<Option<bool>>>,
    memory_notice: Rc<Cell<bool>>,
    memory_export_notice: Rc<Cell<bool>>,
    memory_warning: Rc<Cell<bool>>,
    memory_clear_pending: Rc<Cell<bool>>,
    memory_policy_guard: Rc<Cell<bool>>,
    memory_policy_pending: Rc<Cell<bool>>,
    memory_policy_notice: Rc<Cell<Option<bool>>>,
    source_drop_target: gtk::DropTarget,
}

// 将有限的原生提供商预设映射到稳定的核心适配器配置。
fn provider_preset_config(index: u32) -> (&'static str, &'static str, &'static str, &'static str) {
    match index {
        1 => (
            OLLAMA_PROVIDER_PRESET_ID,
            OLLAMA_ADAPTER_TYPE,
            DEFAULT_OLLAMA_PROVIDER_NAME,
            DEFAULT_OLLAMA_ENDPOINT,
        ),
        2 => (
            ANTHROPIC_PROVIDER_PRESET_ID,
            ANTHROPIC_ADAPTER_TYPE,
            DEFAULT_ANTHROPIC_PROVIDER_NAME,
            DEFAULT_ANTHROPIC_ENDPOINT,
        ),
        3 => (
            GEMINI_PROVIDER_PRESET_ID,
            GEMINI_ADAPTER_TYPE,
            DEFAULT_GEMINI_PROVIDER_NAME,
            DEFAULT_GEMINI_ENDPOINT,
        ),
        4 => (
            AZURE_PROVIDER_PRESET_ID,
            AZURE_ADAPTER_TYPE,
            DEFAULT_AZURE_PROVIDER_NAME,
            DEFAULT_AZURE_ENDPOINT,
        ),
        5 => (
            RESPONSES_PROVIDER_PRESET_ID,
            RESPONSES_ADAPTER_TYPE,
            DEFAULT_RESPONSES_PROVIDER_NAME,
            DEFAULT_RESPONSES_ENDPOINT,
        ),
        _ => (
            CUSTOM_PROVIDER_PRESET_ID,
            OPENAI_ADAPTER_TYPE,
            DEFAULT_PROVIDER_NAME,
            DEFAULT_PROVIDER_ENDPOINT,
        ),
    }
}

// 将持久化的预设标识还原为界面下拉框索引。
fn provider_preset_index(preset_id: &str) -> u32 {
    match preset_id {
        OLLAMA_PROVIDER_PRESET_ID => 1,
        ANTHROPIC_PROVIDER_PRESET_ID => 2,
        GEMINI_PROVIDER_PRESET_ID => 3,
        AZURE_PROVIDER_PRESET_ID => 4,
        RESPONSES_PROVIDER_PRESET_ID => 5,
        _ => 0,
    }
}

// 根据活动界面语言生成提供商预设标签。
fn provider_preset_labels(locale: UiLocale) -> [String; 6] {
    [
        localization::text(locale, "provider.preset.openai", "OpenAI-compatible"),
        localization::text(locale, "provider.preset.ollama", "Ollama (native /api)"),
        localization::text(locale, "provider.preset.anthropic", "Anthropic Messages"),
        localization::text(locale, "provider.preset.gemini", "Google Gemini"),
        localization::text(locale, "provider.preset.azure_openai", "Azure OpenAI"),
        localization::text(
            locale,
            "provider.preset.openai_responses",
            "OpenAI Responses",
        ),
    ]
}

// 根据界面语言生成翻译质量模式标签。
fn quality_mode_labels(locale: UiLocale) -> [String; 3] {
    [
        localization::text(locale, "quality.mode.fast", "Fast"),
        localization::text(locale, "quality.mode.balanced", "Balanced"),
        localization::text(locale, "quality.mode.best", "Best"),
    ]
}

// 将质量模式映射到稳定的下拉框索引。
fn quality_mode_selection(mode: TranslationQualityMode) -> u32 {
    match mode {
        TranslationQualityMode::Fast => 0,
        TranslationQualityMode::Balanced => 1,
        TranslationQualityMode::Best => 2,
    }
}

// 将下拉框索引还原为核心质量模式。
fn quality_mode_for_selection(selection: u32) -> TranslationQualityMode {
    match selection {
        0 => TranslationQualityMode::Fast,
        2 => TranslationQualityMode::Best,
        _ => TranslationQualityMode::Balanced,
    }
}

// 根据界面语言生成内置翻译预设标签。
fn translation_preset_labels(locale: UiLocale) -> [String; 3] {
    [
        localization::text(locale, "translation.preset.general", "General"),
        localization::text(locale, "translation.preset.technical", "Technical"),
        localization::text(locale, "translation.preset.marketing", "Marketing"),
    ]
}

// 将内置翻译预设映射到稳定的下拉框索引。
fn translation_preset_selection(preset: &TranslationPreset) -> u32 {
    match preset.id() {
        "technical" => 1,
        "marketing" => 2,
        _ => 0,
    }
}

// 将下拉框索引还原为经过 Core 校验的内置翻译预设。
fn translation_preset_for_selection(selection: u32) -> TranslationPreset {
    match selection {
        1 => TranslationPreset::technical(),
        2 => TranslationPreset::marketing(),
        _ => TranslationPreset::general(),
    }
}

// 判断预设是否要求用户在连接前提供手工模型或部署名。
fn preset_requires_manual_model(index: u32) -> bool {
    catalog_preset(index).map_or(index == 2 || index == 4, |preset| {
        preset.model_listing == "manual"
    })
}

// 根据预设显示与协议匹配的端点提示。
fn provider_endpoint_tooltip(locale: UiLocale, preset_index: u32) -> String {
    if preset_index == 1 {
        localization::text(
            locale,
            "tooltip.endpoint.ollama",
            "HTTPS or loopback HTTP native Ollama /api endpoint",
        )
    } else if preset_index == 2 {
        localization::text(
            locale,
            "tooltip.endpoint.anthropic",
            "HTTPS Anthropic Messages /v1 endpoint",
        )
    } else if preset_index == 3 {
        localization::text(
            locale,
            "tooltip.endpoint.gemini",
            "HTTPS or loopback HTTP Gemini Generate Content /v1beta endpoint",
        )
    } else if preset_index == 4 {
        localization::text(
            locale,
            "tooltip.endpoint.azure_openai",
            "HTTPS Azure OpenAI resource endpoint; enter the deployment name as the model",
        )
    } else if preset_index == 5 {
        localization::text(
            locale,
            "tooltip.endpoint.openai_responses",
            "HTTPS OpenAI Responses /v1 endpoint with typed streaming events",
        )
    } else {
        localization::text(
            locale,
            "tooltip.endpoint",
            "HTTPS or loopback HTTP OpenAI-compatible base endpoint",
        )
    }
}

// 根据界面语言生成内置提供商的默认显示名称。
fn localized_provider_default_name(locale: UiLocale, preset_index: u32) -> String {
    if preset_index == 1 {
        localization::text(
            locale,
            "profile.default_ollama_name",
            DEFAULT_OLLAMA_PROVIDER_NAME,
        )
    } else if preset_index == 2 {
        localization::text(
            locale,
            "profile.default_anthropic_name",
            DEFAULT_ANTHROPIC_PROVIDER_NAME,
        )
    } else if preset_index == 3 {
        localization::text(
            locale,
            "profile.default_gemini_name",
            DEFAULT_GEMINI_PROVIDER_NAME,
        )
    } else if preset_index == 4 {
        localization::text(
            locale,
            "profile.default_azure_openai_name",
            DEFAULT_AZURE_PROVIDER_NAME,
        )
    } else if preset_index == 5 {
        localization::text(
            locale,
            "profile.default_openai_responses_name",
            DEFAULT_RESPONSES_PROVIDER_NAME,
        )
    } else {
        localization::text(locale, "profile.default_name", DEFAULT_PROVIDER_NAME)
    }
}

// 判断端点是否仍是预设提供的默认值，以避免覆盖用户自定义地址。
fn endpoint_matches_preset_default(endpoint: &str, preset_index: u32) -> bool {
    let endpoint = endpoint.trim();
    if preset_index == 1 {
        endpoint == DEFAULT_OLLAMA_ENDPOINT
            || (endpoint.starts_with("http://127.0.0.1:") && endpoint.ends_with("/api/"))
    } else if preset_index == 2 {
        endpoint == DEFAULT_ANTHROPIC_ENDPOINT
    } else if preset_index == 3 {
        endpoint == DEFAULT_GEMINI_ENDPOINT
    } else if preset_index == 4 {
        endpoint == DEFAULT_AZURE_ENDPOINT
    } else if preset_index == 5 {
        endpoint == DEFAULT_RESPONSES_ENDPOINT
    } else {
        endpoint == DEFAULT_PROVIDER_ENDPOINT
            || (endpoint.starts_with("http://127.0.0.1:") && endpoint.ends_with("/v1/"))
    }
}

// 仅为需要手工指定模型的预设显示对应的模型提示。
fn provider_model_tooltip(locale: UiLocale, preset_index: u32) -> String {
    if preset_index == 2 {
        localization::text(
            locale,
            "tooltip.model.anthropic",
            "Enter the Anthropic model ID before connecting; model discovery is manual for this preset",
        )
    } else if preset_index == 4 {
        localization::text(
            locale,
            "tooltip.model.azure_openai",
            "Enter the Azure OpenAI deployment name before connecting; model discovery is manual for this preset",
        )
    } else if preset_index == 5 {
        localization::text(
            locale,
            "tooltip.model.openai_responses",
            "Models are discovered from the OpenAI Responses-compatible endpoint",
        )
    } else {
        localization::text(
            locale,
            "option.model.manual",
            "Enter a model ID manually...",
        )
    }
}

struct EditorBindings {
    editors: gtk::Paned,
    source: gtk::TextBuffer,
    output: gtk::TextBuffer,
    source_view: gtk::TextView,
    output_view: gtk::TextView,
    source_label: gtk::Label,
    output_label: gtk::Label,
    source_metrics: gtk::Label,
    output_metrics: gtk::Label,
}

fn main() -> glib::ExitCode {
    let application = adw::Application::builder()
        .application_id("dev.linguamesh.LinguaMesh")
        .build();
    let file_dialog_fixture = std::env::var_os("LINGUAMESH_TEST_FILE_DIALOG").is_some();
    let file_drop_fixture = std::env::var_os("LINGUAMESH_TEST_FILE_DROP").is_some();
    application.connect_activate(move |application| {
        build_ui(application, file_dialog_fixture, file_drop_fixture);
    });
    application.run()
}

// 仅为隔离的辅助功能测试读取初始界面语言，普通启动始终使用英文默认值。
fn test_locale_override() -> UiLocale {
    let Ok(value) = std::env::var("LINGUAMESH_TEST_LOCALE") else {
        return UiLocale::English;
    };
    UiLocale::ALL
        .into_iter()
        .find(|locale| locale.language_tag().eq_ignore_ascii_case(&value))
        .unwrap_or(UiLocale::English)
}

fn build_ui(application: &adw::Application, file_dialog_fixture: bool, file_drop_fixture: bool) {
    if let Some(window) = application.active_window() {
        window.present();
        return;
    }
    if let Err(error) = validate_provider_preset_catalog() {
        eprintln!("Provider catalog validation failed: {error}");
        return;
    }
    let initial_locale = test_locale_override();
    let state = Rc::new(RefCell::new(AppState::default()));
    if std::env::var_os("LINGUAMESH_TEST_LOCALE").is_some() {
        state.borrow_mut().set_locale(initial_locale);
    }
    let database_path = glib::user_data_dir()
        .join("dev.linguamesh.LinguaMesh")
        .join("linguamesh.sqlite3");
    let worker = Rc::new(CoreWorker::spawn_with_database(database_path));
    let (window, bindings, theme, locale) = create_window(application);
    if std::env::var_os("LINGUAMESH_TEST_LOCALE").is_some()
        && let Some(index) = UiLocale::ALL
            .iter()
            .position(|locale| *locale == initial_locale)
        && let Ok(index) = u32::try_from(index)
    {
        // 在连接选择回调前同步测试下拉框，避免触发一次额外的状态变更。
        bindings.locale.set_selected(index);
    }
    connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
    connect_action_handlers(&bindings, &state, &worker);
    start_event_pump(&bindings, &state, &worker);

    let shutdown_worker = Rc::clone(&worker);
    window.connect_destroy(move |_| {
        let _ = shutdown_worker.try_send(WorkerCommand::Shutdown);
    });
    refresh_ui(&bindings, &state.borrow());
    window.present();
    if std::env::var_os("LINGUAMESH_TEST_ORCA_ATSPI").is_some() {
        // Orca 烟测在独立窗口中请求生产 Stop 控件的真实 GTK 焦点。
        let focus_window = window.clone();
        let focus_control = bindings.stop.clone().upcast::<gtk::Widget>();
        let warmup_control = bindings.provider_name.clone().upcast::<gtk::Widget>();
        let focus_deadline = Instant::now() + Duration::from_secs(15);
        let focus_start = Instant::now();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let control = if focus_start.elapsed() < Duration::from_secs(3) {
                &warmup_control
            } else {
                &focus_control
            };
            gtk::prelude::GtkWindowExt::set_focus(&focus_window, Some(control));
            if Instant::now() >= focus_deadline {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
    if file_dialog_fixture {
        start_file_dialog_fixture(application, bindings, &state, &worker);
    } else if file_drop_fixture {
        start_file_drop_fixture(application, bindings);
    }
}

fn start_file_dialog_fixture(
    application: &adw::Application,
    bindings: UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &Rc<CoreWorker>,
) {
    let fixture_path = std::env::var("LINGUAMESH_FILE_CHOOSER_FIXTURE")
        .expect("LINGUAMESH_FILE_CHOOSER_FIXTURE must be set");
    let expected = fs::read_to_string(&fixture_path).expect("read file chooser fixture");
    println!("GTK file chooser application fixture requesting the portal dialog.");
    let open_bindings = bindings.clone();
    let open_state = Rc::clone(state);
    let open_worker = Rc::clone(worker);
    glib::timeout_add_local(Duration::from_millis(250), move || {
        begin_source_file_open(&open_bindings, &open_state, &open_worker);
        glib::ControlFlow::Break
    });

    let poll_bindings = bindings;
    let application = application.clone();
    let deadline = Instant::now() + Duration::from_secs(20);
    glib::timeout_add_local(Duration::from_millis(20), move || {
        let contents = poll_bindings.source.text(
            &poll_bindings.source.start_iter(),
            &poll_bindings.source.end_iter(),
            true,
        );
        if contents.as_str() == expected {
            println!(
                "GTK file chooser application fixture passed: portal selection loaded the UTF-8 fixture."
            );
            application.quit();
            glib::ControlFlow::Break
        } else if Instant::now() >= deadline {
            eprintln!(
                "GTK file chooser application fixture timed out while loading the selected file."
            );
            application.quit();
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn start_file_drop_fixture(application: &adw::Application, bindings: UiBindings) {
    let fixture_path = std::env::var("LINGUAMESH_FILE_DROP_FIXTURE")
        .expect("LINGUAMESH_FILE_DROP_FIXTURE must be set");
    let coordinates_path = std::env::var("LINGUAMESH_FILE_DROP_COORDINATES")
        .expect("LINGUAMESH_FILE_DROP_COORDINATES must be set");
    let expected = fs::read_to_string(&fixture_path).expect("read file drop fixture");
    let drag_button = gtk::Button::with_label(&localization::text(
        UiLocale::default(),
        "fixture.drag_file",
        "Drag fixture",
    ));
    drag_button.set_focusable(true);
    drag_button.set_size_request(240, 48);
    let file = gtk::gio::File::for_path(&fixture_path);
    let uri_list = format!("{}\n", file.uri());
    let uri_bytes = gtk::glib::Bytes::from(uri_list.as_bytes());
    let provider = gtk::gdk::ContentProvider::for_bytes("text/uri-list", &uri_bytes);
    let drag_source = gtk::DragSource::new();
    drag_source.set_actions(gtk::gdk::DragAction::COPY);
    drag_source.set_content(Some(&provider));
    drag_source.connect_drag_begin(|_, _| println!("GTK drag-and-drop fixture drag began."));
    drag_source.connect_drag_end(|_, _, _| println!("GTK drag-and-drop fixture drag ended."));
    drag_button.add_controller(drag_source);
    bindings.workspace.prepend(&drag_button);

    let coordinate_button = drag_button.clone();
    let coordinate_target = bindings.source_view.clone();
    let coordinate_window = bindings.window.clone();
    let coordinate_application = application.clone();
    let coordinate_deadline = Instant::now() + Duration::from_secs(5);
    glib::timeout_add_local(Duration::from_millis(50), move || {
        let source_bounds = coordinate_button.compute_bounds(&coordinate_window);
        let target_bounds = coordinate_target.compute_bounds(&coordinate_window);
        if let (Some(source_bounds), Some(target_bounds)) = (source_bounds, target_bounds) {
            let content = format!(
                "{:.0} {:.0} {:.0} {:.0} {:.0} {:.0} {:.0} {:.0}\n",
                source_bounds.x(),
                source_bounds.y(),
                source_bounds.width(),
                source_bounds.height(),
                target_bounds.x(),
                target_bounds.y(),
                target_bounds.width(),
                target_bounds.height(),
            );
            if fs::write(&coordinates_path, content).is_err() {
                eprintln!("GTK drag-and-drop fixture could not write widget coordinates.");
                coordinate_application.quit();
            }
            glib::ControlFlow::Break
        } else if Instant::now() >= coordinate_deadline {
            eprintln!("GTK drag-and-drop fixture could not resolve widget coordinates.");
            coordinate_application.quit();
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });

    let poll_source = bindings.source;
    let poll_application = application.clone();
    let poll_deadline = Instant::now() + Duration::from_secs(20);
    glib::timeout_add_local(Duration::from_millis(20), move || {
        let contents = poll_source.text(&poll_source.start_iter(), &poll_source.end_iter(), true);
        if contents.as_str() == expected {
            println!(
                "GTK drag-and-drop application fixture passed: the source editor loaded the UTF-8 fixture."
            );
            poll_application.quit();
            glib::ControlFlow::Break
        } else if Instant::now() >= poll_deadline {
            eprintln!(
                "GTK drag-and-drop application fixture timed out while loading the dropped file."
            );
            poll_application.quit();
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

#[allow(clippy::too_many_lines)]
fn create_window(
    application: &adw::Application,
) -> (
    adw::ApplicationWindow,
    UiBindings,
    gtk::DropDown,
    gtk::DropDown,
) {
    let display_locale = UiLocale::default();
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title(localization::text(
            display_locale,
            "app.title",
            "LinguaMesh",
        ))
        .default_width(1080)
        .default_height(720)
        .build();
    let header = adw::HeaderBar::new();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);

    let root = create_root();
    let (onboarding, onboarding_title, onboarding_detail) = create_onboarding();
    root.append(&onboarding);
    let (
        provider_session,
        saved_profile,
        provider_preset,
        provider_name,
        provider_endpoint,
        manual_model_row,
        manual_model,
        provider_credential,
        remember_profile,
        remove_saved_profile,
        connect,
        test_connection,
        active_provider,
        provider_title,
        provider_note,
    ) = create_provider_session();
    root.append(&provider_session);
    let (
        controls,
        model,
        source_locale,
        target_locale,
        quality_mode,
        translation_preset,
        glossary,
        import_glossary,
        export_glossary,
        incognito,
        history_enabled,
        history,
        clear_history,
        memory_enabled,
        memory,
        clear_memory,
        routing_profiles,
        theme,
        locale,
    ) = create_controls();
    root.append(&controls);

    let editor_bindings = create_editors();
    let source_drop_target =
        gtk::DropTarget::new(String::static_type(), gtk::gdk::DragAction::COPY);
    source_drop_target.set_types(&[
        String::static_type(),
        gtk::gio::File::static_type(),
        gtk::gdk::FileList::static_type(),
    ]);
    source_drop_target.set_propagation_phase(gtk::PropagationPhase::Capture);
    if std::env::var_os("LINGUAMESH_TEST_FILE_DROP").is_some() {
        source_drop_target.connect_enter(|_, _, _| {
            println!("GTK drag-and-drop fixture entered the source editor.");
            gtk::gdk::DragAction::COPY
        });
        source_drop_target.connect_motion(|_, _, _| {
            println!("GTK drag-and-drop fixture moved over the source editor.");
            gtk::gdk::DragAction::COPY
        });
        source_drop_target.connect_leave(|_| {
            println!("GTK drag-and-drop fixture left the source editor.");
        });
    }
    editor_bindings
        .source_view
        .add_controller(source_drop_target.clone());
    root.append(&editor_bindings.editors);

    let fallback_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let fallback_enabled = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.enable_fallback",
        "Allow approved fallback",
    ));
    // 为回退同意复选框导出稳定的本地化可访问名称。
    fallback_enabled.update_property(&[gtk::accessible::Property::Label(&localization::text(
        display_locale,
        "action.enable_fallback",
        "Allow approved fallback",
    ))]);
    fallback_enabled.set_focusable(true);
    fallback_enabled.set_tooltip_text(Some(&localization::text(
        display_locale,
        "tooltip.fallback",
        "Retry only retryable network failures with this saved provider; document jobs, cancellation, and credential failures never fall back",
    )));
    let fallback_profile_label = gtk::Label::new(Some(&localized_mnemonic(
        display_locale,
        "label.fallback_profile",
        "Fallback provider",
    )));
    fallback_profile_label.set_xalign(0.0);
    let fallback_profile = gtk::DropDown::from_strings(&[&localization::text(
        display_locale,
        "option.fallback.none",
        "No fallback",
    )]);
    // 将回退提供商标签与下拉框建立助记符和可访问关系。
    fallback_profile_label.set_mnemonic_widget(Some(&fallback_profile));
    fallback_profile.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
        fallback_profile_label.upcast_ref(),
    ])]);
    fallback_profile.set_focusable(true);
    fallback_profile.set_sensitive(false);
    fallback_profile.set_hexpand(true);
    fallback_row.append(&fallback_enabled);
    fallback_row.append(&fallback_profile_label);
    fallback_row.append(&fallback_profile);
    root.append(&fallback_row);

    let action_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let open_source = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.open_source",
        "Open text file",
    ));
    open_source.set_focusable(true);
    open_source.set_tooltip_text(Some(&localization::text(
        display_locale,
        "tooltip.open_source",
        "Load a UTF-8 text file into the source editor",
    )));
    let ocr_enabled = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        display_locale,
        "settings.enable_ocr",
        "Enable OCR for image-only PDF",
    ));
    ocr_enabled.set_focusable(true);
    ocr_enabled.set_tooltip_text(Some(&localization::text(
        display_locale,
        "tooltip.enable_ocr",
        "When enabled, use the optional Tesseract plugin for image-only PDF pages and import page-marked text",
    )));
    let document_jobs = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.document_jobs",
        "Document jobs",
    ));
    document_jobs.set_focusable(true);
    document_jobs.set_tooltip_text(Some(&localization::text(
        display_locale,
        "tooltip.document_jobs",
        "Inspect persisted document jobs and their progress",
    )));
    let translate = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.translate",
        "Translate",
    ));
    translate.add_css_class("suggested-action");
    translate.set_focusable(true);
    let retry_translation = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.retry_translation",
        "Retry translation",
    ));
    retry_translation.set_focusable(true);
    retry_translation.set_tooltip_text(Some(&localization::text(
        display_locale,
        "tooltip.retry_translation",
        "Retry the last failed or cancelled text translation",
    )));
    let retry_translation_label = localization::text(
        display_locale,
        "action.retry_translation",
        "Retry translation",
    );
    retry_translation
        .update_property(&[gtk::accessible::Property::Label(&retry_translation_label)]);
    let export_output = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.export_output",
        "Export translation",
    ));
    export_output.set_focusable(true);
    export_output.set_tooltip_text(Some(&localization::text(
        display_locale,
        "tooltip.export_output",
        "Save the translated output to a new file",
    )));
    let open_output = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.open_output",
        "Open exported output",
    ));
    open_output.set_focusable(true);
    open_output.set_tooltip_text(Some(&localization::text(
        display_locale,
        "tooltip.open_output",
        "Open the most recently exported translation output",
    )));
    let stop = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "accessibility.stop_translation",
        "Stop translation",
    ));
    stop.add_css_class("destructive-action");
    stop.set_focusable(true);
    stop.update_property(&[gtk::accessible::Property::Label(&localization::text(
        display_locale,
        "accessibility.stop_translation",
        "Stop translation",
    ))]);
    let pause_document = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.pause_document",
        "Pause document",
    ));
    let resume_document = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.resume_document",
        "Resume document",
    ));
    let retry_document = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.retry_document",
        "Retry document",
    ));
    action_row.append(&open_source);
    action_row.append(&ocr_enabled);
    action_row.append(&document_jobs);
    action_row.append(&translate);
    action_row.append(&retry_translation);
    action_row.append(&export_output);
    action_row.append(&open_output);
    action_row.append(&pause_document);
    action_row.append(&resume_document);
    action_row.append(&retry_document);
    action_row.append(&stop);
    let status = gtk::Label::new(None);
    status.set_accessible_role(gtk::AccessibleRole::Status);
    status.set_xalign(0.0);
    status.set_hexpand(true);
    action_row.append(&status);
    let progress = gtk::ProgressBar::new();
    progress.set_accessible_role(gtk::AccessibleRole::ProgressBar);
    progress.set_show_text(true);
    progress.set_hexpand(true);
    progress.set_visible(false);
    progress.update_property(&[gtk::accessible::Property::Label(&localized_template(
        display_locale,
        "status.document_progress",
        "{completed} of {total} segments translated",
        &[("{completed}", "0"), ("{total}", "0")],
    ))]);
    action_row.append(&progress);
    let partial = gtk::Label::new(None);
    partial.add_css_class("dim-label");
    action_row.append(&partial);
    root.append(&action_row);

    let error = gtk::Label::new(None);
    error.set_accessible_role(gtk::AccessibleRole::Alert);
    error.set_xalign(0.0);
    error.set_wrap(true);
    error.add_css_class("error");
    root.append(&error);
    let locale_note = gtk::Label::new(None);
    locale_note.set_xalign(0.0);
    locale_note.set_wrap(true);
    locale_note.add_css_class("dim-label");
    root.append(&locale_note);
    let diagnostics = gtk::Label::new(None);
    diagnostics.set_xalign(0.0);
    diagnostics.set_selectable(true);
    let diagnostics_panel = gtk::Expander::builder()
        .label(localization::text(
            display_locale,
            "diagnostics.title",
            "Diagnostics",
        ))
        .child(&diagnostics)
        .build();
    root.append(&diagnostics_panel);

    toolbar.set_content(Some(&root));
    window.set_content(Some(&toolbar));
    let window_binding = window.clone();
    let bindings = UiBindings {
        application: application.clone(),
        window: window_binding,
        workspace: root,
        onboarding,
        onboarding_title,
        onboarding_detail,
        provider_title,
        provider_note,
        saved_profile,
        provider_preset,
        provider_name,
        provider_endpoint,
        manual_model_row,
        manual_model,
        provider_credential,
        remember_profile,
        remove_saved_profile,
        connect,
        test_connection,
        active_provider,
        model,
        source_locale,
        target_locale,
        quality_mode,
        translation_preset,
        glossary,
        import_glossary,
        export_glossary,
        incognito,
        history_enabled,
        history,
        clear_history,
        memory_enabled,
        memory,
        clear_memory,
        routing_profiles,
        fallback_enabled,
        fallback_profile_label,
        fallback_profile,
        theme: theme.clone(),
        locale: locale.clone(),
        source: editor_bindings.source,
        output: editor_bindings.output,
        source_view: editor_bindings.source_view,
        output_view: editor_bindings.output_view,
        source_label: editor_bindings.source_label,
        output_label: editor_bindings.output_label,
        source_metrics: editor_bindings.source_metrics,
        output_metrics: editor_bindings.output_metrics,
        translate,
        retry_translation,
        export_output,
        open_output,
        open_source,
        ocr_enabled,
        document_jobs,
        stop,
        pause_document,
        resume_document,
        retry_document,
        status,
        progress,
        partial,
        error,
        locale_note,
        diagnostics_panel,
        diagnostics,
        profile_selection_guard: Rc::new(Cell::new(false)),
        provider_preset_guard: Rc::new(Cell::new(false)),
        provider_preset_previous: Rc::new(Cell::new(0)),
        draft_profile_id: Rc::new(RefCell::new(None)),
        source_uri: Rc::new(RefCell::new(None)),
        output_uri: Rc::new(RefCell::new(None)),
        fallback_profile_ids: Rc::new(RefCell::new(vec![None])),
        selected_routing_profile_id: Rc::new(RefCell::new(None)),
        document_job_id: Rc::new(RefCell::new(None)),
        document_job_guard: Rc::new(Cell::new(false)),
        document_job_state: Rc::new(Cell::new(None)),
        document_progress: Rc::new(Cell::new(None)),
        document_warnings: Rc::new(RefCell::new(Vec::new())),
        ocr_pending: Rc::new(Cell::new(false)),
        connection_test_notice: Rc::new(Cell::new(false)),
        connection_test_model_count: Rc::new(Cell::new(None)),
        connection_test_profile_id: Rc::new(RefCell::new(None)),
        export_notice: Rc::new(Cell::new(false)),
        report_export_notice: Rc::new(Cell::new(false)),
        fallback_notice: Rc::new(Cell::new(false)),
        fallback_approval: Rc::new(Cell::new(false)),
        glossary_notice: Rc::new(Cell::new(false)),
        glossary_from_csv: Rc::new(Cell::new(false)),
        history_notice: Rc::new(Cell::new(false)),
        history_export_notice: Rc::new(Cell::new(false)),
        history_warning: Rc::new(Cell::new(false)),
        history_clear_pending: Rc::new(Cell::new(false)),
        history_policy_guard: Rc::new(Cell::new(false)),
        history_policy_pending: Rc::new(Cell::new(false)),
        history_policy_notice: Rc::new(Cell::new(None)),
        memory_notice: Rc::new(Cell::new(false)),
        memory_export_notice: Rc::new(Cell::new(false)),
        memory_warning: Rc::new(Cell::new(false)),
        memory_clear_pending: Rc::new(Cell::new(false)),
        memory_policy_guard: Rc::new(Cell::new(false)),
        memory_policy_pending: Rc::new(Cell::new(false)),
        memory_policy_notice: Rc::new(Cell::new(None)),
        source_drop_target,
    };
    install_provider_focus_traversal(
        &window,
        vec![
            bindings.saved_profile.clone().upcast::<gtk::Widget>(),
            bindings
                .remove_saved_profile
                .clone()
                .upcast::<gtk::Widget>(),
            bindings.provider_preset.clone().upcast::<gtk::Widget>(),
            bindings.provider_name.clone().upcast::<gtk::Widget>(),
            bindings.provider_endpoint.clone().upcast::<gtk::Widget>(),
            bindings.manual_model.clone().upcast::<gtk::Widget>(),
            bindings.provider_credential.clone().upcast::<gtk::Widget>(),
            bindings.connect.clone().upcast::<gtk::Widget>(),
            bindings.remember_profile.clone().upcast::<gtk::Widget>(),
        ],
    );
    install_keyboard_focus_probe(&window, &bindings, &theme, &locale);
    (window, bindings, theme, locale)
}

#[allow(clippy::too_many_lines)]
fn install_keyboard_focus_probe(
    window: &adw::ApplicationWindow,
    bindings: &UiBindings,
    theme: &gtk::DropDown,
    locale: &gtk::DropDown,
) {
    let Ok(log_path) = std::env::var("LINGUAMESH_KEYBOARD_FOCUS_LOG") else {
        return;
    };
    let Ok(file) = fs::File::create(&log_path) else {
        eprintln!("Keyboard focus fixture could not create the focus log.");
        return;
    };
    let log = Rc::new(RefCell::new(std::io::BufWriter::new(file)));
    let widgets = [
        (
            "saved_profile",
            bindings.saved_profile.clone().upcast::<gtk::Widget>(),
        ),
        (
            "provider_preset",
            bindings.provider_preset.clone().upcast::<gtk::Widget>(),
        ),
        (
            "provider_name",
            bindings.provider_name.clone().upcast::<gtk::Widget>(),
        ),
        (
            "provider_endpoint",
            bindings.provider_endpoint.clone().upcast::<gtk::Widget>(),
        ),
        (
            "manual_model",
            bindings.manual_model.clone().upcast::<gtk::Widget>(),
        ),
        (
            "provider_credential",
            bindings.provider_credential.clone().upcast::<gtk::Widget>(),
        ),
        (
            "remember_profile",
            bindings.remember_profile.clone().upcast::<gtk::Widget>(),
        ),
        ("connect", bindings.connect.clone().upcast::<gtk::Widget>()),
        ("model", bindings.model.clone().upcast::<gtk::Widget>()),
        (
            "source_locale",
            bindings.source_locale.clone().upcast::<gtk::Widget>(),
        ),
        (
            "target_locale",
            bindings.target_locale.clone().upcast::<gtk::Widget>(),
        ),
        (
            "glossary",
            bindings.glossary.clone().upcast::<gtk::Widget>(),
        ),
        (
            "import_glossary",
            bindings.import_glossary.clone().upcast::<gtk::Widget>(),
        ),
        (
            "export_glossary",
            bindings.export_glossary.clone().upcast::<gtk::Widget>(),
        ),
        (
            "incognito",
            bindings.incognito.clone().upcast::<gtk::Widget>(),
        ),
        (
            "history_enabled",
            bindings.history_enabled.clone().upcast::<gtk::Widget>(),
        ),
        ("history", bindings.history.clone().upcast::<gtk::Widget>()),
        (
            "clear_history",
            bindings.clear_history.clone().upcast::<gtk::Widget>(),
        ),
        (
            "memory_enabled",
            bindings.memory_enabled.clone().upcast::<gtk::Widget>(),
        ),
        ("memory", bindings.memory.clone().upcast::<gtk::Widget>()),
        (
            "clear_memory",
            bindings.clear_memory.clone().upcast::<gtk::Widget>(),
        ),
        ("theme", theme.clone().upcast::<gtk::Widget>()),
        ("locale", locale.clone().upcast::<gtk::Widget>()),
        (
            "source_editor",
            bindings.source_view.clone().upcast::<gtk::Widget>(),
        ),
        (
            "output_editor",
            bindings.output_view.clone().upcast::<gtk::Widget>(),
        ),
        (
            "open_source",
            bindings.open_source.clone().upcast::<gtk::Widget>(),
        ),
        (
            "ocr_enabled",
            bindings.ocr_enabled.clone().upcast::<gtk::Widget>(),
        ),
        (
            "document_jobs",
            bindings.document_jobs.clone().upcast::<gtk::Widget>(),
        ),
        (
            "translate",
            bindings.translate.clone().upcast::<gtk::Widget>(),
        ),
        (
            "export_output",
            bindings.export_output.clone().upcast::<gtk::Widget>(),
        ),
        (
            "open_output",
            bindings.open_output.clone().upcast::<gtk::Widget>(),
        ),
        ("stop", bindings.stop.clone().upcast::<gtk::Widget>()),
    ];
    for (name, widget) in widgets {
        widget.set_widget_name(name);
        let name = name.to_owned();
        let log = Rc::clone(&log);
        widget.connect_has_focus_notify(move |current| {
            if current.has_focus() {
                let mut log = log.borrow_mut();
                let _ = writeln!(log, "{name}");
                let _ = log.flush();
            }
        });
    }
    let initial_focus = bindings.provider_name.clone();
    let focus_state_widgets = [
        (
            "provider_name",
            bindings.provider_name.clone().upcast::<gtk::Widget>(),
        ),
        (
            "provider_endpoint",
            bindings.provider_endpoint.clone().upcast::<gtk::Widget>(),
        ),
        (
            "provider_credential",
            bindings.provider_credential.clone().upcast::<gtk::Widget>(),
        ),
        (
            "remember_profile",
            bindings.remember_profile.clone().upcast::<gtk::Widget>(),
        ),
        ("connect", bindings.connect.clone().upcast::<gtk::Widget>()),
    ];
    let ready_logged = Rc::new(Cell::new(false));
    let ready_log = Rc::clone(&log);
    let focus_window = window.clone();
    let focus_workspace = bindings.workspace.clone();
    let focus_start_path = std::env::var_os("LINGUAMESH_KEYBOARD_FOCUS_START");
    let focus_coordinates_path = std::env::var_os("LINGUAMESH_KEYBOARD_FOCUS_COORDINATES");
    let expect_rtl = std::env::var_os("LINGUAMESH_KEYBOARD_FOCUS_EXPECT_RTL").is_some();
    let focus_request_logged = Cell::new(false);
    let focus_attempt_logged = Cell::new(false);
    let mut focus_deadline = None;
    glib::timeout_add_local(Duration::from_millis(50), move || {
        if initial_focus.is_sensitive() && !ready_logged.get() {
            let mut log = ready_log.borrow_mut();
            for (name, widget) in &focus_state_widgets {
                let _ = writeln!(
                    log,
                    "__state__ {name} focusable={} sensitive={} visible={} mapped={}",
                    widget.is_focusable(),
                    widget.is_sensitive(),
                    widget.is_visible(),
                    widget.is_mapped()
                );
            }
            // RTL 键盘夹具必须确认生产工作区方向已切换，避免只验证控件焦点而遗漏布局状态。
            if expect_rtl && focus_workspace.direction() == gtk::TextDirection::Rtl {
                let _ = writeln!(log, "__rtl__");
                let _ = log.flush();
            }
            let _ = writeln!(log, "__ready__");
            let _ = log.flush();
            ready_logged.set(true);
            // 为夹具提供真实鼠标点击所需的 provider 输入框窗口内坐标。
            if let (Some(path), Some(bounds)) = (
                focus_coordinates_path.as_ref(),
                initial_focus.compute_bounds(&focus_window),
            ) {
                let content = format!(
                    "{:.0} {:.0} {:.0} {:.0}\n",
                    bounds.x(),
                    bounds.y(),
                    bounds.width(),
                    bounds.height()
                );
                let _ = fs::write(path, content);
            }
        }
        if !ready_logged.get() {
            return glib::ControlFlow::Continue;
        }
        // 仅在键盘夹具请求后重新抓取焦点，确保测试从真实 provider 输入框开始。
        let focus_requested = focus_start_path
            .as_ref()
            .is_some_and(|path| fs::metadata(path).is_ok());
        if !focus_requested {
            return glib::ControlFlow::Continue;
        }
        if !focus_request_logged.get() {
            let mut log = ready_log.borrow_mut();
            let _ = writeln!(log, "__focus_requested__");
            let _ = log.flush();
            focus_request_logged.set(true);
        }
        if focus_deadline.is_none() {
            focus_deadline = Some(Instant::now() + Duration::from_secs(5));
            focus_window.present();
        }
        gtk::prelude::RootExt::set_focus(&focus_window, Some(&initial_focus));
        let grabbed = initial_focus.grab_focus_without_selecting();
        let focused = grabbed && initial_focus.has_focus();
        if !focus_attempt_logged.get() {
            let mut log = ready_log.borrow_mut();
            let _ = writeln!(
                log,
                "__focus_attempt__ grabbed={grabbed} has_focus={} active={}",
                initial_focus.has_focus(),
                focus_window.is_active()
            );
            let _ = log.flush();
            focus_attempt_logged.set(true);
        }
        if focused || focus_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
    gtk::prelude::GtkWindowExt::set_focus(window, Some(&bindings.provider_name));
}

fn create_onboarding() -> (gtk::Box, gtk::Label, gtk::Label) {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 4);
    section.add_css_class("card");
    section.set_margin_top(4);
    section.set_margin_bottom(4);
    section.set_margin_start(4);
    section.set_margin_end(4);
    let title = gtk::Label::new(None);
    title.set_accessible_role(gtk::AccessibleRole::Heading);
    title.set_xalign(0.0);
    title.add_css_class("heading");
    let detail = gtk::Label::new(None);
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    detail.add_css_class("dim-label");
    section.append(&title);
    section.append(&detail);
    (section, title, detail)
}

fn create_root() -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_accessible_role(gtk::AccessibleRole::Main);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root
}

fn create_editors() -> EditorBindings {
    let locale = UiLocale::default();
    let source = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    let source_view = gtk::TextView::builder()
        .buffer(&source)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    source_view.set_accessible_role(gtk::AccessibleRole::TextBox);
    source_view.set_focusable(true);
    let source_accessible =
        localization::text(locale, "accessibility.source_content", "Text to translate");
    source_view.update_property(&[
        gtk::accessible::Property::Label(&source_accessible),
        gtk::accessible::Property::MultiLine(true),
        gtk::accessible::Property::ReadOnly(false),
    ]);
    let output = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    let output_view = gtk::TextView::builder()
        .buffer(&output)
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
    output_view.set_accessible_role(gtk::AccessibleRole::TextBox);
    output_view.set_focusable(true);
    let output_accessible = localization::text(
        locale,
        "accessibility.translation_output",
        "Streamed translation output",
    );
    output_view.update_property(&[
        gtk::accessible::Property::Label(&output_accessible),
        gtk::accessible::Property::MultiLine(true),
        gtk::accessible::Property::ReadOnly(true),
    ]);
    let editors = gtk::Paned::new(gtk::Orientation::Horizontal);
    editors.set_wide_handle(true);
    let source_label = localized_mnemonic(locale, "field.source_text", "Source text");
    let output_label = localized_mnemonic(locale, "field.translation", "Translation");
    let (source_panel, source_label, source_metrics) = editor_panel(&source_label, &source_view);
    let (output_panel, output_label, output_metrics) = editor_panel(&output_label, &output_view);
    editors.set_start_child(Some(&source_panel));
    editors.set_end_child(Some(&output_panel));
    editors.set_vexpand(true);
    EditorBindings {
        editors,
        source,
        output,
        source_view,
        output_view,
        source_label,
        output_label,
        source_metrics,
        output_metrics,
    }
}

#[allow(clippy::too_many_lines, clippy::type_complexity)]
fn create_provider_session() -> (
    gtk::Box,
    gtk::DropDown,
    gtk::DropDown,
    gtk::Entry,
    gtk::Entry,
    gtk::Box,
    gtk::Entry,
    gtk::PasswordEntry,
    gtk::CheckButton,
    gtk::Button,
    gtk::Button,
    gtk::Button,
    gtk::Label,
    gtk::Label,
    gtk::Label,
) {
    let locale = UiLocale::default();
    let section = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let title_text = localization::text(locale, "section.provider_profiles", "Provider profiles");
    let title = gtk::Label::new(Some(&title_text));
    title.set_accessible_role(gtk::AccessibleRole::Heading);
    title.set_xalign(0.0);
    title.add_css_class("heading");
    section.append(&title);

    let note_text = localization::text(
        locale,
        "profile.storage_note",
        "Names, endpoints, model preferences, and credentials can be remembered through Secret Service. Credentials are cleared from this form immediately and remain session-only when secure storage is unavailable. Removing a saved profile does not disconnect its current session.",
    );
    let note = gtk::Label::new(Some(&note_text));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("dim-label");
    section.append(&note);

    let profile_actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let new_profile = localization::text(locale, "profile.new", "New profile...");
    let saved_profile = gtk::DropDown::from_strings(&[new_profile.as_str()]);
    saved_profile.set_hexpand(true);
    saved_profile.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.saved_profile",
        "Choose a saved non-secret profile or create a new profile",
    )));
    let remove_saved_profile = gtk::Button::with_label(&localization::text(
        locale,
        "action.remove_profile",
        "Remove saved profile",
    ));
    remove_saved_profile.set_focusable(true);
    remove_saved_profile.add_css_class("destructive-action");
    remove_saved_profile.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.remove_profile",
        "Remove the selected saved profile without disconnecting its current session",
    )));
    profile_actions.append(&labeled_control(
        &localized_mnemonic(locale, "label.saved_profile", "Saved profile"),
        saved_profile.upcast_ref::<gtk::Widget>(),
    ));
    profile_actions.append(&remove_saved_profile);
    section.append(&profile_actions);

    let preset_labels = provider_preset_labels(locale);
    let preset_label_refs = preset_labels.iter().map(String::as_str).collect::<Vec<_>>();
    let provider_preset = gtk::DropDown::from_strings(&preset_label_refs);
    provider_preset.set_hexpand(true);
    provider_preset.set_focusable(true);
    provider_preset.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.provider_preset",
        "Choose the provider protocol used for model discovery and streaming",
    )));
    section.append(&labeled_control(
        &localized_mnemonic(locale, "label.provider_preset", "Provider preset"),
        provider_preset.upcast_ref::<gtk::Widget>(),
    ));

    let fields = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let default_provider_name =
        localization::text(locale, "profile.default_name", DEFAULT_PROVIDER_NAME);
    let provider_name = gtk::Entry::builder()
        .text(&default_provider_name)
        .hexpand(true)
        .build();
    provider_name.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.provider_name",
        "Session-only provider display name",
    )));
    let provider_endpoint = gtk::Entry::builder()
        .text(DEFAULT_PROVIDER_ENDPOINT)
        .hexpand(true)
        .build();
    provider_endpoint.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.endpoint",
        "HTTPS or loopback HTTP OpenAI-compatible base endpoint",
    )));
    let manual_model = gtk::Entry::new();
    manual_model.set_hexpand(true);
    manual_model.set_placeholder_text(Some(&localization::text(
        locale,
        "option.model.manual",
        "Enter a model ID manually...",
    )));
    manual_model.set_tooltip_text(Some(&provider_model_tooltip(locale, 0)));
    let manual_model_row = labeled_control(
        &localized_mnemonic(locale, "field.model", "Model"),
        manual_model.upcast_ref::<gtk::Widget>(),
    );
    manual_model_row.set_visible(false);
    let provider_credential = gtk::PasswordEntry::builder()
        .show_peek_icon(true)
        .hexpand(true)
        .build();
    provider_credential.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.credential",
        "Optional credential; it is kept in memory unless remembered through Secret Service",
    )));
    let remember_profile = gtk::CheckButton::with_label(&localization::text(
        locale,
        "option.remember_profile",
        "Remember profile, model, and credential in Secret Service",
    ));
    remember_profile.set_focusable(true);
    remember_profile.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.remember_profile",
        "Save non-secret profile data and the credential through Secret Service",
    )));
    let connect =
        gtk::Button::with_mnemonic(&localized_mnemonic(locale, "action.connect", "Connect"));
    let test_connection = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.test_connection",
        "Test connection",
    ));
    test_connection.set_focusable(true);
    test_connection.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.test_connection",
        "Check the provider model endpoint without switching or saving the active profile",
    )));
    connect.set_focusable(true);
    fields.append(&labeled_control(
        &localized_mnemonic(locale, "provider.name", "Provider name"),
        provider_name.upcast_ref::<gtk::Widget>(),
    ));
    fields.append(&labeled_control(
        &localized_mnemonic(
            locale,
            "label.endpoint_loopback",
            "Endpoint (loopback example)",
        ),
        provider_endpoint.upcast_ref::<gtk::Widget>(),
    ));
    fields.append(&manual_model_row);
    fields.append(&labeled_control(
        &localized_mnemonic(
            locale,
            "label.credential",
            "Credential (optional; secure when remembered)",
        ),
        provider_credential.upcast_ref::<gtk::Widget>(),
    ));
    fields.append(&test_connection);
    fields.append(&connect);
    section.append(&fields);
    section.append(&remember_profile);
    let active_provider = gtk::Label::new(None);
    active_provider.set_xalign(0.0);
    active_provider.set_wrap(true);
    section.append(&active_provider);
    (
        section,
        saved_profile,
        provider_preset,
        provider_name,
        provider_endpoint,
        manual_model_row,
        manual_model,
        provider_credential,
        remember_profile,
        remove_saved_profile,
        connect,
        test_connection,
        active_provider,
        title,
        note,
    )
}

// 为 provider 表单提供稳定的 Tab 与 Shift+Tab 焦点顺序。
fn install_provider_focus_traversal(
    window: &adw::ApplicationWindow,
    focus_order: Vec<gtk::Widget>,
) {
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let focus_root = window.clone();
    controller.connect_key_pressed(move |_, key, _, state| {
        if key != gtk::gdk::Key::Tab
            || state.intersects(
                gtk::gdk::ModifierType::CONTROL_MASK
                    | gtk::gdk::ModifierType::ALT_MASK
                    | gtk::gdk::ModifierType::SUPER_MASK,
            )
        {
            return gtk::glib::Propagation::Proceed;
        }
        let reverse = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        let step: isize = if reverse { -1 } else { 1 };
        let mut next: isize = focus_order
            .iter()
            .position(gtk::prelude::WidgetExt::has_focus)
            .map_or(
                if reverse {
                    (focus_order.len() - 1).cast_signed()
                } else {
                    0
                },
                |current| current.cast_signed() + step,
            );
        while let Ok(index) = usize::try_from(next) {
            let Some(widget) = focus_order.get(index) else {
                break;
            };
            if widget.is_visible() && widget.is_sensitive() && widget.is_focusable() && {
                gtk::prelude::RootExt::set_focus(&focus_root, Some(widget));
                widget.grab_focus() && widget.has_focus()
            } {
                return gtk::glib::Propagation::Stop;
            }
            next += step;
        }
        gtk::glib::Propagation::Proceed
    });
    window.add_controller(controller);
}

#[allow(clippy::too_many_lines, clippy::type_complexity)]
fn create_controls() -> (
    gtk::Box,
    gtk::DropDown,
    gtk::DropDown,
    gtk::DropDown,
    gtk::DropDown,
    gtk::DropDown,
    gtk::Entry,
    gtk::Button,
    gtk::Button,
    gtk::CheckButton,
    gtk::CheckButton,
    gtk::Button,
    gtk::Button,
    gtk::CheckButton,
    gtk::Button,
    gtk::Button,
    gtk::Button,
    gtk::DropDown,
    gtk::DropDown,
) {
    let locale = UiLocale::default();
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let model_placeholder = localization::text(locale, "option.model.select", "Select a model...");
    let model = gtk::DropDown::from_strings(&[model_placeholder.as_str()]);
    let source_options = [
        localization::text(locale, "option.source.auto", "Auto"),
        localization::text(locale, "option.source.english", "English"),
        localization::text(locale, "option.source.chinese", "Chinese"),
    ];
    let source_locale = gtk::DropDown::from_strings(
        &source_options
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    let target_options = [
        localization::text(
            locale,
            "option.target.chinese_simplified",
            "Chinese (Simplified)",
        ),
        localization::text(locale, "option.target.english", "English"),
        localization::text(locale, "option.target.japanese", "Japanese"),
    ];
    let target_locale = gtk::DropDown::from_strings(
        &target_options
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    let quality_labels = quality_mode_labels(locale);
    let quality_mode = gtk::DropDown::from_strings(
        &quality_labels
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    quality_mode.set_selected(quality_mode_selection(TranslationQualityMode::Balanced));
    quality_mode.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.quality_mode",
        "Fast uses one direct pass; Balanced adds deterministic structure checks; Best asks for an internal critique and revision.",
    )));
    let translation_preset_labels = translation_preset_labels(locale);
    let translation_preset = gtk::DropDown::from_strings(
        &translation_preset_labels
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    translation_preset.set_selected(0);
    translation_preset.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.translation_preset",
        "Apply a bounded domain, tone, formality, and audience preference to the next request",
    )));
    let glossary = gtk::Entry::new();
    glossary.set_hexpand(true);
    glossary.set_placeholder_text(Some(&localization::text(
        locale,
        "field.glossary.placeholder",
        "source => target; Product Name => Product Name",
    )));
    glossary.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.glossary",
        "Optional semicolon-separated source => target glossary rules; entries stay in memory for this translation.",
    )));
    let import_glossary = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.import_glossary",
        "Import glossary",
    ));
    import_glossary.set_focusable(true);
    import_glossary.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.import_glossary",
        "Load glossary rules from a UTF-8 CSV file",
    )));
    let export_glossary = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.export_glossary",
        "Export glossary",
    ));
    export_glossary.set_focusable(true);
    export_glossary.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.export_glossary",
        "Save the current glossary rules as a UTF-8 CSV file",
    )));
    let incognito = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        locale,
        "settings.incognito",
        "Incognito mode",
    ));
    incognito.set_focusable(true);
    incognito.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.incognito",
        "Do not persist source, output, history, or translation-memory data for this request",
    )));
    let history_enabled = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        locale,
        "settings.save_history",
        "Save translation history",
    ));
    history_enabled.set_focusable(true);
    history_enabled.set_active(true);
    history_enabled.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.save_history",
        "Persist completed standard translations in local history; existing entries are kept when disabled",
    )));
    let clear_history = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.clear_history",
        "Clear history",
    ));
    clear_history.set_focusable(true);
    clear_history.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.clear_history",
        "Delete all locally stored translation history",
    )));
    let history = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.view_history",
        "View history",
    ));
    history.set_focusable(true);
    history.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.view_history",
        "Inspect, export, or delete individual local translation history entries",
    )));
    let memory_enabled = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        locale,
        "settings.save_memory",
        "Save translation memory",
    ));
    memory_enabled.set_focusable(true);
    memory_enabled.set_active(true);
    memory_enabled.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.save_memory",
        "Reuse and persist completed standard translations in local translation memory",
    )));
    let memory = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.view_memory",
        "View translation memory",
    ));
    memory.set_focusable(true);
    memory.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.view_memory",
        "Inspect, export, or delete individual local translation memory entries",
    )));
    let clear_memory = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.clear_memory",
        "Clear translation memory",
    ));
    clear_memory.set_focusable(true);
    clear_memory.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.clear_memory",
        "Delete all locally stored translation memory entries",
    )));
    let routing_profiles = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.routing_profiles",
        "Routing profiles",
    ));
    routing_profiles.set_focusable(true);
    routing_profiles.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.routing_profiles",
        "Create, inspect, and delete non-secret routing planner profiles",
    )));
    let theme_options = [
        localization::text(locale, "theme.system", "System"),
        localization::text(locale, "theme.light", "Light"),
        localization::text(locale, "theme.dark", "Dark"),
    ];
    let theme =
        gtk::DropDown::from_strings(&theme_options.iter().map(String::as_str).collect::<Vec<_>>());
    let locale_labels =
        UiLocale::ALL.map(|displayed_locale| localized_locale_name(locale, displayed_locale));
    let locale =
        gtk::DropDown::from_strings(&locale_labels.iter().map(String::as_str).collect::<Vec<_>>());
    for (label, control) in [
        (
            localized_mnemonic(UiLocale::default(), "field.model", "Model"),
            model.upcast_ref::<gtk::Widget>(),
        ),
        (
            localized_mnemonic(
                UiLocale::default(),
                "label.source_language",
                "Source language",
            ),
            source_locale.upcast_ref::<gtk::Widget>(),
        ),
        (
            localized_mnemonic(
                UiLocale::default(),
                "settings.target_language",
                "Target language",
            ),
            target_locale.upcast_ref::<gtk::Widget>(),
        ),
        (
            localized_mnemonic(UiLocale::default(), "label.quality_mode", "Quality mode"),
            quality_mode.upcast_ref::<gtk::Widget>(),
        ),
        (
            localized_mnemonic(
                UiLocale::default(),
                "label.translation_preset",
                "Translation preset",
            ),
            translation_preset.upcast_ref::<gtk::Widget>(),
        ),
        (
            localized_mnemonic(UiLocale::default(), "field.glossary", "Glossary"),
            glossary.upcast_ref::<gtk::Widget>(),
        ),
        (
            localization::text(UiLocale::default(), "settings.theme", "Theme"),
            theme.upcast_ref::<gtk::Widget>(),
        ),
        (
            localization::text(
                UiLocale::default(),
                "settings.ui_language",
                "Interface language",
            ),
            locale.upcast_ref::<gtk::Widget>(),
        ),
    ] {
        controls.append(&labeled_control(&label, control));
    }
    controls.append(&import_glossary);
    controls.append(&export_glossary);
    controls.append(&incognito);
    controls.append(&history_enabled);
    controls.append(&history);
    controls.append(&clear_history);
    controls.append(&memory_enabled);
    controls.append(&memory);
    controls.append(&clear_memory);
    controls.append(&routing_profiles);
    (
        controls,
        model,
        source_locale,
        target_locale,
        quality_mode,
        translation_preset,
        glossary,
        import_glossary,
        export_glossary,
        incognito,
        history_enabled,
        history,
        clear_history,
        memory_enabled,
        memory,
        clear_memory,
        routing_profiles,
        theme,
        locale,
    )
}

fn labeled_control(label: &str, control: &gtk::Widget) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let label = gtk::Label::with_mnemonic(label);
    label.set_xalign(0.0);
    label.add_css_class("caption");
    control.set_focusable(true);
    label.set_mnemonic_widget(Some(control));
    control.update_relation(&[gtk::accessible::Relation::LabelledBy(&[label.upcast_ref()])]);
    container.append(&label);
    container.append(control);
    container
}

fn set_labeled_control_label(control: &gtk::Widget, text: &str) {
    if let Some(label) = control
        .parent()
        .and_then(|parent| parent.first_child())
        .and_then(|child| child.downcast::<gtk::Label>().ok())
    {
        label.set_label(text);
    }
}

// 为本地化标签保留 GTK 助记符入口。
fn localized_mnemonic(locale: UiLocale, key: &str, fallback: &str) -> String {
    format!("_{}", localization::text(locale, key, fallback))
}

// 替换本地化模板中的非敏感运行时占位符。
fn localized_template(
    locale: UiLocale,
    key: &str,
    fallback: &str,
    replacements: &[(&str, &str)],
) -> String {
    let mut value = localization::text(locale, key, fallback);
    for (placeholder, replacement) in replacements {
        value = value.replace(placeholder, replacement);
    }
    value
}

fn parse_glossary(
    text: &str,
    source_locale: Option<&str>,
    target_locale: &str,
) -> Result<Option<Glossary>, TranslationError> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let mut entries = Vec::new();
    for rule in text.split(';').filter(|rule| !rule.trim().is_empty()) {
        let Some((source_term, target_term)) = rule.split_once("=>") else {
            return Err(TranslationError::new(
                ErrorKind::InvalidConfiguration,
                "Glossary entries must use the form source => target.",
            ));
        };
        let mut entry =
            GlossaryEntry::new(source_term.trim(), target_term.trim()).map_err(|_| {
                TranslationError::new(
                    ErrorKind::InvalidConfiguration,
                    "Glossary entries contain invalid or credential-like data.",
                )
            })?;
        if let Some(source_locale) = source_locale {
            entry = entry.with_source_locale(source_locale);
        }
        entry = entry.with_target_locale(target_locale);
        entries.push(entry);
    }
    Glossary::new(entries).map(Some).map_err(|_| {
        TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "Glossary entries conflict.",
        )
    })
}

fn current_document_options(
    bindings: &UiBindings,
    state: &mut AppState,
) -> Result<(Option<String>, String, Option<Glossary>), TranslationError> {
    let source_locale =
        SOURCE_LOCALES[bindings.source_locale.selected() as usize].map(str::to_owned);
    let target_locale = TARGET_LOCALES[bindings.target_locale.selected() as usize].to_owned();
    let parsed_glossary = if bindings.glossary_from_csv.get() {
        state.glossary().cloned().map(Some).ok_or_else(|| {
            TranslationError::new(
                ErrorKind::InvalidConfiguration,
                localization::text(
                    state.locale(),
                    "error.glossary_import",
                    "The glossary CSV could not be imported.",
                ),
            )
        })
    } else {
        parse_glossary(
            bindings.glossary.text().as_str(),
            source_locale.as_deref(),
            &target_locale,
        )
    }?;
    state.set_source_locale(source_locale.clone());
    state.set_target_locale(&target_locale);
    state.set_glossary(parsed_glossary.clone());
    Ok((source_locale, target_locale, parsed_glossary))
}

fn document_progress(snapshot: &DocumentJobSnapshot) -> (usize, usize) {
    let total = snapshot
        .job
        .segments
        .iter()
        .filter(|segment| segment.kind == linguamesh_document::DocumentSegmentKind::Prose)
        .count();
    (total.saturating_sub(snapshot.job.pending_count()), total)
}

fn refresh_dropdown_labels(dropdown: &gtk::DropDown, labels: &[String]) {
    if let Some(model) = dropdown
        .model()
        .and_then(|model| model.downcast::<gtk::StringList>().ok())
    {
        let replacements = labels.iter().map(String::as_str).collect::<Vec<_>>();
        model.splice(0, model.n_items(), &replacements);
    }
}

fn localized_locale_name(locale: UiLocale, displayed_locale: UiLocale) -> String {
    let (key, fallback) = match displayed_locale {
        UiLocale::English => ("locale.name.en", "English"),
        UiLocale::SimplifiedChinese => ("locale.name.zh_hans", "Simplified Chinese"),
        UiLocale::TraditionalChinese => ("locale.name.zh_hant", "Traditional Chinese"),
        UiLocale::Spanish => ("locale.name.es", "Spanish"),
        UiLocale::French => ("locale.name.fr", "French"),
        UiLocale::German => ("locale.name.de", "German"),
        UiLocale::Japanese => ("locale.name.ja", "Japanese"),
        UiLocale::Korean => ("locale.name.ko", "Korean"),
        UiLocale::BrazilianPortuguese => ("locale.name.pt_br", "Portuguese (Brazil)"),
        UiLocale::Russian => ("locale.name.ru", "Russian"),
        UiLocale::Arabic => ("locale.name.ar", "Arabic"),
        UiLocale::Hindi => ("locale.name.hi", "Hindi"),
        UiLocale::PseudoAccentedEnglish => ("locale.name.en_xa", "Pseudo English (Accented)"),
        UiLocale::PseudoRtlArabic => ("locale.name.ar_xb", "Pseudo Arabic (RTL)"),
    };
    localization::text(locale, key, fallback)
}

// 刷新动态模型列表的占位项，同时保留核心返回的模型名称。
fn refresh_model_placeholder(dropdown: &gtk::DropDown, locale: UiLocale) {
    let Some(model) = dropdown
        .model()
        .and_then(|model| model.downcast::<gtk::StringList>().ok())
    else {
        return;
    };
    if model.n_items() == 0 {
        return;
    }
    let mut labels = (0..model.n_items())
        .filter_map(|index| model.string(index))
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    labels[0] = localization::text(locale, "option.model.select", "Select a model...");
    refresh_dropdown_labels(dropdown, &labels);
}

// 刷新保存配置列表的新增占位项，同时保留用户配置名称。
fn refresh_profile_placeholder(dropdown: &gtk::DropDown, locale: UiLocale) {
    let Some(model) = dropdown
        .model()
        .and_then(|model| model.downcast::<gtk::StringList>().ok())
    else {
        return;
    };
    if model.n_items() == 0 {
        return;
    }
    let mut labels = (0..model.n_items())
        .filter_map(|index| model.string(index))
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    labels[0] = localization::text(locale, "profile.new", "New profile...");
    refresh_dropdown_labels(dropdown, &labels);
}

fn generate_custom_provider_id(
    saved_profiles: &[ProviderProfile],
) -> Result<ProviderProfileId, TranslationError> {
    for _ in 0..PROFILE_ID_GENERATION_ATTEMPTS {
        let candidate = format!("profile-{}", glib::uuid_string_random());
        let profile_id = ProviderProfileId::parse(candidate).map_err(|error| {
            TranslationError::new(
                ErrorKind::InvalidConfiguration,
                format!("The generated provider profile ID is invalid: {error}"),
            )
        })?;
        if saved_profiles
            .iter()
            .all(|profile| profile.id() != &profile_id)
        {
            return Ok(profile_id);
        }
    }
    Err(TranslationError::new(
        ErrorKind::Internal,
        "A unique provider profile ID could not be generated.",
    ))
}

fn ensure_draft_profile_id(
    bindings: &UiBindings,
    state: &AppState,
) -> Result<ProviderProfileId, TranslationError> {
    if let Some(profile_id) = bindings.draft_profile_id.borrow().as_ref() {
        return Ok(profile_id.clone());
    }
    let profile_id = generate_custom_provider_id(state.saved_profiles())?;
    bindings.draft_profile_id.replace(Some(profile_id.clone()));
    Ok(profile_id)
}

fn profile_dropdown_label(profile: &ProviderProfile) -> String {
    format!("{} · {}", profile.display_name(), profile.id().as_str())
}

fn rebuild_saved_profile_dropdown(bindings: &UiBindings, state: &AppState) {
    let mut labels = vec![localization::text(
        state.locale(),
        "profile.new",
        "New profile...",
    )];
    labels.extend(state.saved_profiles().iter().map(profile_dropdown_label));
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let selected = state
        .selected_saved_profile_id()
        .and_then(|selected_id| {
            state
                .saved_profiles()
                .iter()
                .position(|profile| profile.id() == selected_id)
        })
        .and_then(|index| u32::try_from(index + 1).ok())
        .unwrap_or(0);
    bindings.profile_selection_guard.set(true);
    let profile_list = gtk::StringList::new(&label_refs);
    bindings.saved_profile.set_model(Some(&profile_list));
    bindings.saved_profile.set_selected(selected);
    bindings.profile_selection_guard.set(false);
}

// 重建回退配置列表，仅保留本地已保存且不同于当前主配置的候选项。
fn rebuild_fallback_profile_dropdown(bindings: &UiBindings, state: &AppState) {
    let selected_id = bindings
        .fallback_profile_ids
        .borrow()
        .get(usize::try_from(bindings.fallback_profile.selected()).unwrap_or(0))
        .cloned()
        .flatten();
    let mut labels = vec![localization::text(
        state.locale(),
        "option.fallback.none",
        "No fallback",
    )];
    let mut profile_ids = vec![None];
    for profile in state.saved_profiles() {
        if state.active_saved_profile_id() != Some(profile.id()) {
            labels.push(profile_dropdown_label(profile));
            profile_ids.push(Some(profile.id().clone()));
        }
    }
    let selected = selected_id
        .as_ref()
        .and_then(|id| {
            profile_ids
                .iter()
                .position(|candidate| candidate.as_ref() == Some(id))
        })
        .and_then(|index| u32::try_from(index).ok())
        .unwrap_or(0);
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    let profile_list = gtk::StringList::new(&label_refs);
    bindings.fallback_profile.set_model(Some(&profile_list));
    bindings.fallback_profile.set_selected(selected);
    bindings.fallback_profile_ids.replace(profile_ids);
}

fn selected_fallback_profile_id(bindings: &UiBindings) -> Option<ProviderProfileId> {
    bindings
        .fallback_profile_ids
        .borrow()
        .get(usize::try_from(bindings.fallback_profile.selected()).ok()?)
        .cloned()
        .flatten()
}

fn show_saved_profile_in_form(bindings: &UiBindings, profile: &ProviderProfile) {
    let preset_index = provider_preset_index(profile.preset_id());
    bindings.provider_preset_guard.set(true);
    bindings.provider_preset.set_selected(preset_index);
    bindings.provider_preset_previous.set(preset_index);
    bindings.provider_preset_guard.set(false);
    bindings.provider_name.set_text(profile.display_name());
    bindings.provider_endpoint.set_text(profile.base_endpoint());
    bindings
        .manual_model
        .set_text(profile.selected_model().unwrap_or_default());
    bindings
        .manual_model_row
        .set_visible(preset_requires_manual_model(preset_index));
    bindings.provider_credential.set_text("");
    bindings.remember_profile.set_active(true);
    bindings.draft_profile_id.replace(None);
}

fn show_new_profile_in_form(
    bindings: &UiBindings,
    state: &AppState,
) -> Result<(), TranslationError> {
    let profile_id = generate_custom_provider_id(state.saved_profiles())?;
    bindings.draft_profile_id.replace(Some(profile_id));
    bindings.provider_preset_guard.set(true);
    bindings.provider_preset.set_selected(0);
    bindings.provider_preset_previous.set(0);
    bindings.provider_preset_guard.set(false);
    bindings
        .provider_name
        .set_text(&localized_provider_default_name(state.locale(), 0));
    bindings
        .provider_endpoint
        .set_text(DEFAULT_PROVIDER_ENDPOINT);
    bindings.manual_model.set_text("");
    bindings.manual_model_row.set_visible(false);
    bindings.provider_credential.set_text("");
    bindings.remember_profile.set_active(false);
    Ok(())
}

fn custom_provider_profile(
    profile_id: ProviderProfileId,
    display_name: String,
    preset_id: String,
    adapter_type: String,
    endpoint: String,
    secret_ref: Option<SecretRef>,
    selected_model: Option<String>,
) -> Result<ProviderProfile, TranslationError> {
    ProviderProfile::new(
        profile_id,
        display_name,
        preset_id,
        adapter_type,
        endpoint,
        secret_ref,
    )
    .and_then(|profile| profile.with_selected_model(selected_model))
    .map_err(|error| {
        TranslationError::new(
            ErrorKind::InvalidConfiguration,
            format!("The provider profile is invalid: {error}"),
        )
    })
}

fn editor_panel(label: &str, editor: &gtk::TextView) -> (gtk::Box, gtk::Label, gtk::Label) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let label = gtk::Label::with_mnemonic(label);
    label.set_xalign(0.0);
    label.add_css_class("heading");
    label.set_mnemonic_widget(Some(editor));
    editor.update_relation(&[gtk::accessible::Relation::LabelledBy(&[label.upcast_ref()])]);
    let scroller = gtk::ScrolledWindow::builder()
        .child(editor)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();
    scroller.set_vexpand(true);
    let metrics = gtk::Label::new(Some(&text_metrics_label(UiLocale::default(), "")));
    metrics.set_xalign(0.0);
    metrics.add_css_class("dim-label");
    container.append(&label);
    container.append(&scroller);
    container.append(&metrics);
    (container, label, metrics)
}

// 根据文本内容生成非敏感的字符数和近似 token 数提示。
fn text_metrics_label(locale: UiLocale, text: &str) -> String {
    let characters = text.chars().count().to_string();
    let estimated_tokens = text.len().saturating_add(3) / 4;
    let estimated_tokens_text = estimated_tokens.to_string();
    localized_template(
        locale,
        "status.text_metrics",
        "Characters: {characters} · Estimated tokens: {tokens}",
        &[
            ("{characters}", &characters),
            ("{tokens}", &estimated_tokens_text),
        ],
    )
}

// 根据核心归一化记录生成不含文本和定价假设的 usage 提示。
fn usage_label(locale: UiLocale, usage: Option<&UsageRecord>) -> Option<String> {
    let usage = usage?;
    let source = match usage.source {
        UsageSource::ProviderReported => localization::text(
            locale,
            "usage.source.provider_reported",
            "provider reported",
        ),
        UsageSource::LocallyEstimated => localization::text(
            locale,
            "usage.source.locally_estimated",
            "locally estimated",
        ),
        UsageSource::Unknown => localization::text(locale, "usage.source.unknown", "unknown"),
    };
    Some(match usage.total_tokens {
        Some(total_tokens) => localized_template(
            locale,
            "status.usage",
            "Usage: {total_tokens} tokens ({source})",
            &[
                ("{total_tokens}", &total_tokens.to_string()),
                ("{source}", &source),
            ],
        ),
        None => localized_template(
            locale,
            "status.usage_unknown",
            "Usage: unavailable ({source})",
            &[("{source}", &source)],
        ),
    })
}

// 将文本指标和已完成请求的 usage 信息组合到译文面板。
fn output_metrics_label(locale: UiLocale, text: &str, usage: Option<&UsageRecord>) -> String {
    let metrics = text_metrics_label(locale, text);
    usage_label(locale, usage)
        .map(|usage| format!("{metrics}\n{usage}"))
        .unwrap_or(metrics)
}

// 读取 GTK 文本缓冲区内容，供计数提示使用而不影响翻译状态。
fn text_buffer_contents(buffer: &gtk::TextBuffer) -> String {
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, true).to_string()
}

// 同步源文本和译文的计数提示，并随界面语言刷新其模板。
fn refresh_text_metrics(bindings: &UiBindings, locale: UiLocale, usage: Option<&UsageRecord>) {
    bindings.source_metrics.set_label(&text_metrics_label(
        locale,
        &text_buffer_contents(&bindings.source),
    ));
    bindings.output_metrics.set_label(&output_metrics_label(
        locale,
        &text_buffer_contents(&bindings.output),
        usage,
    ));
}

fn confirmed_model_index(state: &AppState) -> u32 {
    state
        .selected_model()
        .and_then(|confirmed| {
            state
                .models()
                .iter()
                .position(|model| model.id == confirmed)
        })
        .and_then(|index| u32::try_from(index + 1).ok())
        .unwrap_or(0)
}

fn connect_profile_selection_handler(bindings: &UiBindings, state: &Rc<RefCell<AppState>>) {
    let profile_bindings = bindings.clone();
    let profile_state = Rc::clone(state);
    bindings
        .saved_profile
        .connect_selected_notify(move |drop_down| {
            if profile_bindings.profile_selection_guard.get() {
                return;
            }
            let selected = drop_down.selected() as usize;
            let profile_id = selected.checked_sub(1).and_then(|index| {
                profile_state
                    .borrow()
                    .saved_profiles()
                    .get(index)
                    .map(|profile| profile.id().clone())
            });
            let selection_result = profile_state
                .borrow_mut()
                .select_saved_profile(profile_id.as_ref());
            if let Err(error) = selection_result {
                profile_state
                    .borrow_mut()
                    .record_client_error(error.to_string());
                rebuild_saved_profile_dropdown(&profile_bindings, &profile_state.borrow());
                refresh_ui(&profile_bindings, &profile_state.borrow());
                return;
            }
            let form_result = match profile_id.as_ref() {
                Some(profile_id) => {
                    let profile = profile_state
                        .borrow()
                        .saved_profiles()
                        .iter()
                        .find(|profile| profile.id() == profile_id)
                        .cloned();
                    match profile {
                        Some(profile) => {
                            show_saved_profile_in_form(&profile_bindings, &profile);
                            Ok(())
                        }
                        None => Err(TranslationError::new(
                            ErrorKind::Internal,
                            localization::text(
                                profile_state.borrow().locale(),
                                "error.profile_unavailable",
                                "The selected saved profile is unavailable.",
                            ),
                        )),
                    }
                }
                None => show_new_profile_in_form(&profile_bindings, &profile_state.borrow()),
            };
            if let Err(error) = form_result {
                profile_state.borrow_mut().provider_failed(error);
            }
            refresh_ui(&profile_bindings, &profile_state.borrow());
        });
}

// 预设切换只更新仍为默认值的字段，保留用户明确输入的名称和端点。
fn connect_provider_preset_handler(bindings: &UiBindings) {
    let preset_bindings = bindings.clone();
    bindings
        .provider_preset
        .connect_selected_notify(move |drop_down| {
            if preset_bindings.provider_preset_guard.get() {
                return;
            }
            let selected = drop_down.selected();
            let previous = preset_bindings.provider_preset_previous.replace(selected);
            if selected == previous {
                return;
            }
            let locale = UiLocale::from_index(preset_bindings.locale.selected() as usize);
            let (_, _, _, default_endpoint) = provider_preset_config(selected);
            if endpoint_matches_preset_default(&preset_bindings.provider_endpoint.text(), previous)
            {
                preset_bindings.provider_endpoint.set_text(default_endpoint);
            }
            let (_, _, previous_name, _) = provider_preset_config(previous);
            let localized_previous_name = localized_provider_default_name(locale, previous);
            if preset_bindings.provider_name.text().trim() == previous_name
                || preset_bindings.provider_name.text().trim() == localized_previous_name
            {
                preset_bindings
                    .provider_name
                    .set_text(&localized_provider_default_name(locale, selected));
            }
            preset_bindings
                .provider_endpoint
                .set_tooltip_text(Some(&provider_endpoint_tooltip(locale, selected)));
            preset_bindings
                .manual_model_row
                .set_visible(preset_requires_manual_model(selected));
            preset_bindings
                .manual_model
                .set_sensitive(preset_requires_manual_model(selected));
            preset_bindings
                .manual_model
                .set_tooltip_text(Some(&provider_model_tooltip(locale, selected)));
        });
}

#[allow(clippy::too_many_lines)]
fn connect_selection_handlers(
    bindings: &UiBindings,
    theme: &gtk::DropDown,
    locale: &gtk::DropDown,
    state: &Rc<RefCell<AppState>>,
    worker: &Rc<CoreWorker>,
) {
    connect_profile_selection_handler(bindings, state);
    connect_provider_preset_handler(bindings);

    let quality_bindings = bindings.clone();
    let quality_state = Rc::clone(state);
    bindings
        .quality_mode
        .connect_selected_notify(move |drop_down| {
            let mode = quality_mode_for_selection(drop_down.selected());
            quality_state.borrow_mut().set_quality_mode(mode);
            refresh_ui(&quality_bindings, &quality_state.borrow());
        });

    let preset_bindings = bindings.clone();
    let preset_state = Rc::clone(state);
    bindings
        .translation_preset
        .connect_selected_notify(move |drop_down| {
            let preset = translation_preset_for_selection(drop_down.selected());
            preset_state.borrow_mut().set_translation_preset(preset);
            refresh_ui(&preset_bindings, &preset_state.borrow());
        });

    let model_bindings = bindings.clone();
    let model_state = Rc::clone(state);
    let model_worker = Rc::clone(worker);
    bindings.model.connect_selected_notify(move |drop_down| {
        let selected = drop_down.selected() as usize;
        if selected == 0 {
            let confirmed = confirmed_model_index(&model_state.borrow());
            if confirmed != 0 {
                drop_down.set_selected(confirmed);
            }
            return;
        }
        let model_id = model_state
            .borrow()
            .models()
            .get(selected - 1)
            .map(|model| model.id.clone());
        if let Some(model_id) = model_id {
            let profile_id = model_state.borrow().provider_id().cloned();
            {
                let state = model_state.borrow();
                if state.selected_model() == Some(model_id.as_str())
                    || state.pending_model_selection() == Some(model_id.as_str())
                {
                    return;
                }
            }
            let selection_result = model_state.borrow_mut().begin_model_selection(&model_id);
            let restore_selection = if let Err(error) = selection_result {
                model_state
                    .borrow_mut()
                    .record_client_error(error.to_string());
                true
            } else {
                let worker_error = match profile_id {
                    Some(profile_id) => model_worker
                        .try_send(WorkerCommand::SelectModel {
                            profile_id,
                            model_id: model_id.clone(),
                        })
                        .err()
                        .map(|error| TranslationError::new(ErrorKind::Internal, error.to_string())),
                    None => Some(TranslationError::new(
                        ErrorKind::Internal,
                        localization::text(
                            model_state.borrow().locale(),
                            "error.active_provider_unavailable",
                            "The active provider ID is unavailable.",
                        ),
                    )),
                };
                let restore_selection = worker_error.is_some();
                if let Some(worker_error) = worker_error
                    && let Err(state_error) = model_state
                        .borrow_mut()
                        .model_selection_failed(&model_id, worker_error)
                {
                    model_state
                        .borrow_mut()
                        .record_client_error(state_error.to_string());
                }
                restore_selection
            };
            if restore_selection {
                let confirmed = confirmed_model_index(&model_state.borrow());
                drop_down.set_selected(confirmed);
            }
            refresh_ui(&model_bindings, &model_state.borrow());
        }
    });
    let theme_bindings = bindings.clone();
    let theme_state = Rc::clone(state);
    theme.connect_selected_notify(move |drop_down| {
        let preference = match drop_down.selected() {
            1 => ThemePreference::Light,
            2 => ThemePreference::Dark,
            _ => ThemePreference::System,
        };
        theme_state.borrow_mut().set_theme(preference);
        let scheme = match preference {
            ThemePreference::System => adw::ColorScheme::Default,
            ThemePreference::Light => adw::ColorScheme::ForceLight,
            ThemePreference::Dark => adw::ColorScheme::ForceDark,
        };
        adw::StyleManager::default().set_color_scheme(scheme);
        refresh_ui(&theme_bindings, &theme_state.borrow());
    });
    let locale_bindings = bindings.clone();
    let locale_state = Rc::clone(state);
    locale.connect_selected_notify(move |drop_down| {
        let selected = UiLocale::from_index(drop_down.selected() as usize);
        locale_state.borrow_mut().set_locale(selected);
        refresh_ui(&locale_bindings, &locale_state.borrow());
    });
}

// 在普通文本请求可能触发回退前要求用户再次确认内容边界。
fn show_fallback_approval_dialog(bindings: &UiBindings, state: &Rc<RefCell<AppState>>) {
    let locale = state.borrow().locale();
    let dialog = gtk::Window::builder()
        .application(&bindings.application)
        .transient_for(&bindings.window)
        .modal(true)
        .title(localization::text(
            locale,
            "action.enable_fallback",
            "Allow approved fallback",
        ))
        .default_width(520)
        .default_height(220)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let message = gtk::Label::new(Some(&format!(
        "{}\n\n{}",
        localization::text(
            locale,
            "status.fallback_selected",
            "The approved fallback provider was selected; content may be sent there.",
        ),
        localization::text(
            locale,
            "tooltip.fallback",
            "Retry only retryable network failures with this saved provider; document jobs, cancellation, and credential failures never fall back",
        )
    )));
    message.set_xalign(0.0);
    message.set_wrap(true);
    message.set_focusable(true);
    root.append(&message);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let approve =
        gtk::Button::with_mnemonic(&localized_mnemonic(locale, "action.translate", "Translate"));
    approve.set_focusable(true);
    let cancel = gtk::Button::with_mnemonic(&localized_mnemonic(locale, "action.close", "Close"));
    cancel.set_focusable(true);
    actions.append(&approve);
    actions.append(&cancel);
    root.append(&actions);
    dialog.set_child(Some(&root));

    let approve_bindings = bindings.clone();
    let approve_dialog = dialog.clone();
    approve.connect_clicked(move |_| {
        approve_bindings.fallback_approval.set(true);
        approve_dialog.close();
        approve_bindings.translate.emit_clicked();
    });
    let cancel_dialog = dialog.clone();
    cancel.connect_clicked(move |_| cancel_dialog.close());
    dialog.present();
}

// Secret Service 持久化交互失败时，明确引导用户改用仅会话凭据而不静默改变意图。
fn show_secret_storage_session_fallback(bindings: &UiBindings, state: &Rc<RefCell<AppState>>) {
    let locale = state.borrow().locale();
    let error_text = state
        .borrow()
        .localized_error_text(locale)
        .unwrap_or_else(|| {
            localization::text(
                locale,
                "error.storage.secure_unavailable",
                "Secure credential storage is unavailable.",
            )
        });
    let dialog = gtk::Window::builder()
        .application(&bindings.application)
        .transient_for(&bindings.window)
        .modal(true)
        .title(localization::text(
            locale,
            "error.storage.secure_unavailable",
            "Secure credential storage is unavailable.",
        ))
        .default_width(520)
        .default_height(240)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let message = gtk::Label::new(Some(&format!(
        "{}\n\n{}",
        error_text,
        localization::text(
            locale,
            "error.storage.session_only",
            "Profile storage is unavailable; use session-only mode.",
        )
    )));
    message.set_xalign(0.0);
    message.set_wrap(true);
    message.set_focusable(true);
    root.append(&message);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let session_only = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "error.storage.session_only",
        "Profile storage is unavailable; use session-only mode.",
    ));
    session_only.set_focusable(true);
    let close = gtk::Button::with_mnemonic(&localized_mnemonic(locale, "action.close", "Close"));
    close.set_focusable(true);
    actions.append(&session_only);
    actions.append(&close);
    root.append(&actions);
    dialog.set_child(Some(&root));

    let fallback_bindings = bindings.clone();
    let fallback_dialog = dialog.clone();
    session_only.connect_clicked(move |_| {
        fallback_bindings.remember_profile.set_active(false);
        fallback_bindings.provider_credential.grab_focus();
        fallback_dialog.close();
    });
    let close_dialog = dialog.clone();
    close.connect_clicked(move |_| close_dialog.close());
    dialog.present();
}

// 只有用户尚未确认且开启了回退时才显示一次发送前提示。
#[must_use]
fn fallback_confirmation_needed(enabled: bool, approved: bool) -> bool {
    enabled && !approved
}

// 四个原生操作共享同一状态与工作线程绑定，集中注册可保持生命周期一致。
#[allow(clippy::too_many_lines)]
fn connect_action_handlers(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &Rc<CoreWorker>,
) {
    let glossary_edit_state = Rc::clone(&bindings.glossary_from_csv);
    bindings.glossary.connect_changed(move |_| {
        glossary_edit_state.set(false);
    });

    let source_job_bindings = bindings.clone();
    let source_job_guard = Rc::clone(&bindings.document_job_guard);
    bindings.source.connect_changed(move |_| {
        if !source_job_guard.get() {
            *source_job_bindings.document_job_id.borrow_mut() = None;
            source_job_bindings.document_warnings.borrow_mut().clear();
        }
    });

    let source_metrics_bindings = bindings.clone();
    let source_metrics_state = Rc::clone(state);
    bindings.source.connect_changed(move |buffer| {
        let locale = source_metrics_state.borrow().locale();
        source_metrics_bindings
            .source_metrics
            .set_label(&text_metrics_label(locale, &text_buffer_contents(buffer)));
    });

    let incognito_bindings = bindings.clone();
    let incognito_state = Rc::clone(state);
    bindings.incognito.connect_toggled(move |button| {
        let mut state = incognito_state.borrow_mut();
        state.set_privacy_mode(if button.is_active() {
            TranslationPrivacyMode::Incognito
        } else {
            TranslationPrivacyMode::Standard
        });
        refresh_ui(&incognito_bindings, &state);
    });

    let history_policy_bindings = bindings.clone();
    let history_policy_state = Rc::clone(state);
    let history_policy_worker = Rc::clone(worker);
    let history_policy_guard = Rc::clone(&bindings.history_policy_guard);
    bindings.history_enabled.connect_toggled(move |button| {
        if history_policy_guard.get() {
            return;
        }
        let enabled = button.is_active();
        if !history_policy_state.borrow().profile_storage_available()
            || history_policy_bindings.history_policy_pending.get()
        {
            history_policy_guard.set(true);
            button.set_active(history_policy_state.borrow().translation_history_enabled());
            history_policy_guard.set(false);
            return;
        }
        history_policy_bindings.history_policy_pending.set(true);
        history_policy_bindings.history_policy_notice.set(None);
        refresh_ui(&history_policy_bindings, &history_policy_state.borrow());
        if let Err(error) =
            history_policy_worker.try_send(WorkerCommand::SetTranslationHistoryEnabled { enabled })
        {
            history_policy_bindings.history_policy_pending.set(false);
            history_policy_guard.set(true);
            button.set_active(history_policy_state.borrow().translation_history_enabled());
            history_policy_guard.set(false);
            history_policy_state
                .borrow_mut()
                .record_client_error(error.to_string());
            refresh_ui(&history_policy_bindings, &history_policy_state.borrow());
        }
    });

    let memory_policy_bindings = bindings.clone();
    let memory_policy_state = Rc::clone(state);
    let memory_policy_worker = Rc::clone(worker);
    let memory_policy_guard = Rc::clone(&bindings.memory_policy_guard);
    bindings.memory_enabled.connect_toggled(move |button| {
        if memory_policy_guard.get() {
            return;
        }
        let enabled = button.is_active();
        if !memory_policy_state.borrow().profile_storage_available()
            || memory_policy_bindings.memory_policy_pending.get()
        {
            memory_policy_guard.set(true);
            button.set_active(memory_policy_state.borrow().translation_memory_enabled());
            memory_policy_guard.set(false);
            return;
        }
        memory_policy_bindings.memory_policy_pending.set(true);
        memory_policy_bindings.memory_policy_notice.set(None);
        refresh_ui(&memory_policy_bindings, &memory_policy_state.borrow());
        if let Err(error) =
            memory_policy_worker.try_send(WorkerCommand::SetTranslationMemoryEnabled { enabled })
        {
            memory_policy_bindings.memory_policy_pending.set(false);
            memory_policy_guard.set(true);
            button.set_active(memory_policy_state.borrow().translation_memory_enabled());
            memory_policy_guard.set(false);
            memory_policy_state
                .borrow_mut()
                .record_client_error(error.to_string());
            refresh_ui(&memory_policy_bindings, &memory_policy_state.borrow());
        }
    });

    let clear_history_bindings = bindings.clone();
    let clear_history_state = Rc::clone(state);
    let clear_history_worker = Rc::clone(worker);
    bindings.clear_history.connect_clicked(move |_| {
        if clear_history_bindings.history_clear_pending.get()
            || !clear_history_state.borrow().profile_storage_available()
        {
            return;
        }
        clear_history_bindings.history_clear_pending.set(true);
        refresh_ui(&clear_history_bindings, &clear_history_state.borrow());
        if let Err(error) = clear_history_worker.try_send(WorkerCommand::ClearTranslationHistory) {
            clear_history_bindings.history_clear_pending.set(false);
            clear_history_state
                .borrow_mut()
                .record_client_error(error.to_string());
            refresh_ui(&clear_history_bindings, &clear_history_state.borrow());
        }
    });

    let history_bindings = bindings.clone();
    let history_state = Rc::clone(state);
    let history_worker = Rc::clone(worker);
    bindings.history.connect_clicked(move |_| {
        if !history_state.borrow().profile_storage_available() {
            return;
        }
        if let Err(error) = history_worker.try_send(WorkerCommand::ListTranslationHistory) {
            history_state
                .borrow_mut()
                .record_client_error(error.to_string());
            refresh_ui(&history_bindings, &history_state.borrow());
        }
    });

    let clear_memory_bindings = bindings.clone();
    let clear_memory_state = Rc::clone(state);
    let clear_memory_worker = Rc::clone(worker);
    bindings.clear_memory.connect_clicked(move |_| {
        if clear_memory_bindings.memory_clear_pending.get()
            || !clear_memory_state.borrow().profile_storage_available()
        {
            return;
        }
        clear_memory_bindings.memory_clear_pending.set(true);
        refresh_ui(&clear_memory_bindings, &clear_memory_state.borrow());
        if let Err(error) = clear_memory_worker.try_send(WorkerCommand::ClearTranslationMemory) {
            clear_memory_bindings.memory_clear_pending.set(false);
            clear_memory_state
                .borrow_mut()
                .record_client_error(error.to_string());
            refresh_ui(&clear_memory_bindings, &clear_memory_state.borrow());
        }
    });

    let memory_bindings = bindings.clone();
    let memory_state = Rc::clone(state);
    let memory_worker = Rc::clone(worker);
    bindings.memory.connect_clicked(move |_| {
        if !memory_state.borrow().profile_storage_available() {
            return;
        }
        if let Err(error) = memory_worker.try_send(WorkerCommand::ListTranslationMemory) {
            memory_state
                .borrow_mut()
                .record_client_error(error.to_string());
            refresh_ui(&memory_bindings, &memory_state.borrow());
        }
    });

    let routing_bindings = bindings.clone();
    let routing_state = Rc::clone(state);
    let routing_worker = Rc::clone(worker);
    bindings.routing_profiles.connect_clicked(move |_| {
        if !routing_state.borrow().profile_storage_available() {
            return;
        }
        if let Err(error) = routing_worker.command_handle().list_routing_profiles() {
            routing_state
                .borrow_mut()
                .record_client_error(error.to_string());
            refresh_ui(&routing_bindings, &routing_state.borrow());
        }
    });

    let import_bindings = bindings.clone();
    let import_state = Rc::clone(state);
    bindings.import_glossary.connect_clicked(move |_| {
        begin_glossary_import(&import_bindings, &import_state);
    });

    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    bindings.export_glossary.connect_clicked(move |_| {
        begin_glossary_export(&export_bindings, &export_state);
    });

    let open_bindings = bindings.clone();
    let open_state = Rc::clone(state);
    let open_worker = Rc::clone(worker);
    bindings.open_source.connect_clicked(move |_| {
        begin_source_file_open(&open_bindings, &open_state, &open_worker);
    });
    let jobs_bindings = bindings.clone();
    let jobs_state = Rc::clone(state);
    let jobs_worker = Rc::clone(worker);
    bindings.document_jobs.connect_clicked(move |_| {
        if let Err(error) = jobs_worker.command_handle().list_document_jobs() {
            jobs_state
                .borrow_mut()
                .record_client_error(error.to_string());
            refresh_ui(&jobs_bindings, &jobs_state.borrow());
        }
    });
    let drop_bindings = bindings.clone();
    let drop_state = Rc::clone(state);
    let drop_worker = Rc::clone(worker);
    bindings
        .source_drop_target
        .connect_drop(move |_, value, _, _| {
            if !source_import_allowed(&drop_state.borrow()) {
                return false;
            }
            if let Ok(uri_list) = value.get::<String>() {
                let Some(uri) = uri_list.lines().find(|line| !line.is_empty()) else {
                    return false;
                };
                let file = gtk::gio::File::for_uri(uri);
                load_source_file(&file, &drop_bindings, &drop_state, &drop_worker);
                true
            } else if let Ok(file) = value.get::<gtk::gio::File>() {
                load_source_file(&file, &drop_bindings, &drop_state, &drop_worker);
                true
            } else if let Ok(file_list) = value.get::<gtk::gdk::FileList>() {
                let Some(file) = file_list.files().into_iter().next() else {
                    return false;
                };
                load_source_file(&file, &drop_bindings, &drop_state, &drop_worker);
                true
            } else {
                false
            }
        });

    let test_bindings = bindings.clone();
    let test_state = Rc::clone(state);
    let test_worker = Rc::clone(worker);
    bindings.test_connection.connect_clicked(move |_| {
        if !test_state.borrow().worker_ready() {
            return;
        }
        test_bindings.connection_test_notice.set(false);
        test_bindings.connection_test_model_count.set(None);
        test_bindings.connection_test_profile_id.replace(None);
        let display_name = test_bindings.provider_name.text().trim().to_owned();
        let endpoint = test_bindings.provider_endpoint.text().trim().to_owned();
        let credential_text = test_bindings.provider_credential.text();
        let has_credential = !credential_text.is_empty();
        let session_secret = has_credential.then(|| SecretValue::new(credential_text.as_str()));
        test_bindings.provider_credential.set_text("");
        drop(credential_text);
        let mut state = test_state.borrow_mut();
        let locale = state.locale();
        if display_name.is_empty() {
            state.record_client_error(localization::text(
                locale,
                "error.provider_name_required",
                "Enter a provider name.",
            ));
            refresh_ui(&test_bindings, &state);
            return;
        }
        if endpoint.is_empty() {
            state.record_client_error(localization::text(
                locale,
                "error.provider_endpoint_required",
                "Enter a provider endpoint.",
            ));
            refresh_ui(&test_bindings, &state);
            return;
        }
        let preset_index = test_bindings.provider_preset.selected();
        let (preset_id, adapter_type, _, _) = provider_preset_config(preset_index);
        let manual_model = test_bindings.manual_model.text().trim().to_owned();
        if preset_requires_manual_model(preset_index) && manual_model.is_empty() {
            state.record_client_error(localization::text(
                locale,
                if preset_index == 4 {
                    "error.azure_openai_deployment_required"
                } else {
                    "error.anthropic_model_required"
                },
                if preset_index == 4 {
                    "Enter an Azure OpenAI deployment name before connecting."
                } else {
                    "Enter an Anthropic model ID before connecting."
                },
            ));
            refresh_ui(&test_bindings, &state);
            return;
        }
        let (profile_id, saved_secret_ref, selected_model) = match state.selected_saved_profile() {
            Some(saved) => (
                saved.id().clone(),
                saved.secret_ref().cloned(),
                if preset_requires_manual_model(preset_index) {
                    Some(manual_model.clone())
                } else {
                    saved.selected_model().map(str::to_owned)
                },
            ),
            None => match ensure_draft_profile_id(&test_bindings, &state) {
                Ok(profile_id) => (
                    profile_id,
                    None,
                    preset_requires_manual_model(preset_index).then_some(manual_model.clone()),
                ),
                Err(error) => {
                    state.record_client_error(error.to_string());
                    refresh_ui(&test_bindings, &state);
                    return;
                }
            },
        };
        let secret_ref = if has_credential {
            Some(SecretRef::new(SecretRefNamespace::Session))
        } else {
            saved_secret_ref
        };
        let profile = match custom_provider_profile(
            profile_id,
            display_name,
            preset_id.to_owned(),
            adapter_type.to_owned(),
            endpoint,
            secret_ref,
            selected_model,
        ) {
            Ok(profile) => profile,
            Err(error) => {
                state.record_client_error(error.to_string());
                refresh_ui(&test_bindings, &state);
                return;
            }
        };
        if let Err(error) = test_worker.try_send(WorkerCommand::TestConnection {
            profile,
            secret: session_secret,
        }) {
            state.record_client_error(error.to_string());
        }
        refresh_ui(&test_bindings, &state);
    });

    let connect_bindings = bindings.clone();
    let connect_state = Rc::clone(state);
    let connect_worker = Rc::clone(worker);
    bindings.connect.connect_clicked(move |_| {
        if !connect_state.borrow().worker_ready() {
            return;
        }
        let display_name = connect_bindings.provider_name.text().trim().to_owned();
        let endpoint = connect_bindings.provider_endpoint.text().trim().to_owned();
        let remember_profile = connect_bindings.remember_profile.is_active();
        let credential_text = connect_bindings.provider_credential.text();
        let has_credential = !credential_text.is_empty();
        let session_secret = has_credential.then(|| SecretValue::new(credential_text.as_str()));
        connect_bindings.provider_credential.set_text("");
        drop(credential_text);
        let mut state = connect_state.borrow_mut();
        if display_name.is_empty() {
            let message = localization::text(
                state.locale(),
                "error.provider_name_required",
                "Enter a provider name.",
            );
            state.provider_failed(TranslationError::new(ErrorKind::InvalidEndpoint, message));
            refresh_ui(&connect_bindings, &state);
            return;
        }
        if endpoint.is_empty() {
            let message = localization::text(
                state.locale(),
                "error.provider_endpoint_required",
                "Enter a provider endpoint.",
            );
            state.provider_failed(TranslationError::new(ErrorKind::InvalidEndpoint, message));
            refresh_ui(&connect_bindings, &state);
            return;
        }
        let preset_index = connect_bindings.provider_preset.selected();
        let (preset_id, adapter_type, _, _) = provider_preset_config(preset_index);
        let preset_id = preset_id.to_owned();
        let adapter_type = adapter_type.to_owned();
        let manual_model = connect_bindings.manual_model.text().trim().to_owned();
        if preset_requires_manual_model(preset_index) && manual_model.is_empty() {
            let message = localization::text(
                state.locale(),
                if preset_index == 4 {
                    "error.azure_openai_deployment_required"
                } else {
                    "error.anthropic_model_required"
                },
                if preset_index == 4 {
                    "Enter an Azure OpenAI deployment name before connecting."
                } else {
                    "Enter an Anthropic model ID before connecting."
                },
            );
            state.provider_failed(TranslationError::new(ErrorKind::ModelUnavailable, message));
            refresh_ui(&connect_bindings, &state);
            return;
        }
        let (profile_id, saved_secret_ref, enabled, selected_model) =
            match state.selected_saved_profile() {
                Some(saved) => (
                    Ok(saved.id().clone()),
                    saved.secret_ref().cloned(),
                    saved.enabled(),
                    if preset_requires_manual_model(preset_index) {
                        Some(manual_model.clone())
                    } else {
                        saved.selected_model().map(str::to_owned)
                    },
                ),
                None => (
                    ensure_draft_profile_id(&connect_bindings, &state),
                    None,
                    true,
                    preset_requires_manual_model(preset_index).then_some(manual_model.clone()),
                ),
            };
        let profile_id = match profile_id {
            Ok(profile_id) => profile_id,
            Err(error) => {
                state.provider_failed(error);
                refresh_ui(&connect_bindings, &state);
                return;
            }
        };
        let persistent_secret_ref = if remember_profile && has_credential {
            let secret_ref = saved_secret_ref
                .clone()
                .filter(SecretRef::is_persistent)
                .unwrap_or_else(|| SecretRef::new(SecretRefNamespace::SecretService));
            if let Some(secret) = session_secret.as_ref()
                && let Err(error) = secret_service::store_secret(&secret_ref, secret)
            {
                state.provider_failed(error);
                refresh_ui(&connect_bindings, &state);
                drop(state);
                show_secret_storage_session_fallback(&connect_bindings, &connect_state);
                return;
            }
            Some(secret_ref)
        } else {
            None
        };
        let profile = match custom_provider_profile(
            profile_id,
            display_name,
            preset_id,
            adapter_type,
            endpoint,
            if let Some(secret_ref) = persistent_secret_ref {
                Some(secret_ref)
            } else if has_credential {
                Some(SecretRef::new(SecretRefNamespace::Session))
            } else {
                saved_secret_ref
            },
            selected_model,
        )
        .map(|profile| profile.with_enabled(enabled))
        {
            Ok(profile) => profile,
            Err(error) => {
                state.provider_failed(error);
                refresh_ui(&connect_bindings, &state);
                return;
            }
        };
        match state.begin_provider_connection_with_persistence(profile.clone(), remember_profile) {
            Ok(()) => {
                if let Err(error) = connect_worker.try_send(WorkerCommand::Connect {
                    profile,
                    secret: session_secret,
                    persistence: if remember_profile {
                        PersistenceIntent::Persistent
                    } else {
                        PersistenceIntent::SessionOnly
                    },
                }) {
                    state.provider_failed(TranslationError::new(
                        ErrorKind::Internal,
                        error.to_string(),
                    ));
                }
                refresh_ui(&connect_bindings, &state);
            }
            Err(StateError::InvalidProfile) => {
                let message = localization::text(
                    state.locale(),
                    "error.profile_disabled",
                    "The selected provider profile is disabled.",
                );
                state.provider_failed(TranslationError::new(
                    ErrorKind::InvalidConfiguration,
                    message,
                ));
                refresh_ui(&connect_bindings, &state);
            }
            Err(error) => {
                state.record_client_error(error.to_string());
                refresh_ui(&connect_bindings, &state);
            }
        }
    });

    let remove_bindings = bindings.clone();
    let remove_state = Rc::clone(state);
    let remove_worker = Rc::clone(worker);
    bindings.remove_saved_profile.connect_clicked(move |_| {
        let profile_id = remove_state.borrow().selected_saved_profile_id().cloned();
        let Some(profile_id) = profile_id else {
            return;
        };
        let mut state = remove_state.borrow_mut();
        match state.begin_profile_deletion(&profile_id) {
            Ok(()) => {
                if let Err(error) = remove_worker.try_send(WorkerCommand::DeleteSavedProfile {
                    profile_id: profile_id.clone(),
                }) {
                    let rollback = state.profile_deletion_failed(
                        &profile_id,
                        TranslationError::new(ErrorKind::Internal, error.to_string()),
                    );
                    if let Err(error) = rollback {
                        state.record_client_error(error.to_string());
                    }
                }
            }
            Err(error) => state.record_client_error(error.to_string()),
        }
        refresh_ui(&remove_bindings, &state);
    });

    let translate_bindings = bindings.clone();
    let translate_state = Rc::clone(state);
    let translate_worker = Rc::clone(worker);
    bindings.translate.connect_clicked(move |_| {
        translate_bindings.export_notice.set(false);
        translate_bindings.report_export_notice.set(false);
        translate_bindings.fallback_notice.set(false);
        *translate_bindings.output_uri.borrow_mut() = None;
        let source = translate_bindings.source.text(
            &translate_bindings.source.start_iter(),
            &translate_bindings.source.end_iter(),
            true,
        );
        let mut state = translate_state.borrow_mut();
        state.set_source_text(source.as_str());
        match current_document_options(&translate_bindings, &mut state) {
            Ok((source_locale, target_locale, glossary)) => {
                let document_job_id = translate_bindings.document_job_id.borrow().clone();
                let fallback_enabled = translate_bindings.fallback_enabled.is_active();
                let routing_profile_id = translate_bindings
                    .selected_routing_profile_id
                    .borrow()
                    .clone();
                let fallback_profile_id = if fallback_enabled {
                    selected_fallback_profile_id(&translate_bindings)
                } else {
                    None
                };
                if let Some(job_id) = document_job_id {
                    if state.is_incognito() {
                        state.record_client_error(
                            "Incognito mode cannot persist document job progress.",
                        );
                    } else {
                        match state.begin_document_translation() {
                            Ok(()) => {
                                let command = match routing_profile_id {
                                    Some(routing_profile_id) => {
                                        WorkerCommand::TranslateDocumentJobWithRouting {
                                            job_id,
                                            source_locale,
                                            target_locale,
                                            glossary,
                                            quality_mode: state.quality_mode(),
                                            translation_preset: state.translation_preset().clone(),
                                            privacy_mode: state.privacy_mode(),
                                            routing_profile_id,
                                        }
                                    }
                                    None => WorkerCommand::TranslateDocumentJob {
                                        job_id,
                                        source_locale,
                                        target_locale,
                                        glossary,
                                        quality_mode: state.quality_mode(),
                                        translation_preset: state.translation_preset().clone(),
                                        privacy_mode: state.privacy_mode(),
                                    },
                                };
                                match translate_worker.try_send(command) {
                                    Ok(()) => {
                                        translate_bindings
                                            .document_job_state
                                            .set(Some(DocumentJobState::Running));
                                    }
                                    Err(error) => state.record_client_error(error.to_string()),
                                }
                            }
                            Err(error) => state.record_client_error(error.to_string()),
                        }
                    }
                } else if routing_profile_id.is_none()
                    && fallback_enabled
                    && fallback_profile_id.is_none()
                {
                    let message = localization::text(
                        state.locale(),
                        "error.fallback_profile_required",
                        "Choose an approved saved fallback provider or turn fallback off.",
                    );
                    state.record_client_error(message);
                } else {
                    if fallback_confirmation_needed(
                        fallback_enabled,
                        translate_bindings.fallback_approval.get(),
                    ) {
                        drop(state);
                        show_fallback_approval_dialog(&translate_bindings, &translate_state);
                        return;
                    }
                    match state.begin_translation() {
                        Ok(request) => {
                            translate_bindings.fallback_approval.set(false);
                            let command = if let Some(routing_profile_id) = routing_profile_id {
                                WorkerCommand::TranslateWithRouting {
                                    request,
                                    routing_profile_id,
                                }
                            } else {
                                fallback_profile_id.map_or(
                                    WorkerCommand::Translate(request.clone()),
                                    |fallback_profile_id| WorkerCommand::TranslateWithFallback {
                                        request,
                                        fallback_profile_id,
                                    },
                                )
                            };
                            if let Err(error) = translate_worker.try_send(command) {
                                state.record_client_error(error.to_string());
                            }
                        }
                        Err(error) => state.record_client_error(error.to_string()),
                    }
                }
            }
            Err(error) => state.record_client_error(error.message),
        }
        refresh_ui(&translate_bindings, &state);
    });

    let retry_button = bindings.translate.clone();
    bindings.retry_translation.connect_clicked(move |_| {
        retry_button.emit_clicked();
    });

    let pause_bindings = bindings.clone();
    let pause_state = Rc::clone(state);
    let pause_worker = Rc::clone(worker);
    bindings.pause_document.connect_clicked(move |_| {
        let Some(job_id) = pause_bindings.document_job_id.borrow().clone() else {
            return;
        };
        if let Err(error) = pause_worker.try_send(WorkerCommand::PauseDocumentJob { job_id }) {
            let mut state = pause_state.borrow_mut();
            state.record_client_error(error.to_string());
            refresh_ui(&pause_bindings, &state);
        }
    });

    let resume_bindings = bindings.clone();
    let resume_state = Rc::clone(state);
    let resume_worker = Rc::clone(worker);
    bindings.resume_document.connect_clicked(move |_| {
        let Some(job_id) = resume_bindings.document_job_id.borrow().clone() else {
            return;
        };
        let mut state = resume_state.borrow_mut();
        let result = state
            .begin_document_translation()
            .map_err(|error| {
                TranslationError::new(ErrorKind::InvalidConfiguration, error.to_string())
            })
            .and_then(|()| {
                resume_worker
                    .try_send(WorkerCommand::ResumeDocumentJob { job_id })
                    .map_err(|error| TranslationError::new(ErrorKind::Internal, error.to_string()))
            });
        match result {
            Ok(()) => resume_bindings
                .document_job_state
                .set(Some(DocumentJobState::Running)),
            Err(error) => state.record_client_error(error.message),
        }
        refresh_ui(&resume_bindings, &state);
    });

    let retry_bindings = bindings.clone();
    let retry_state = Rc::clone(state);
    let retry_worker = Rc::clone(worker);
    bindings.retry_document.connect_clicked(move |_| {
        let Some(job_id) = retry_bindings.document_job_id.borrow().clone() else {
            return;
        };
        let mut state = retry_state.borrow_mut();
        let result = state
            .begin_document_translation()
            .map_err(|error| {
                TranslationError::new(ErrorKind::InvalidConfiguration, error.to_string())
            })
            .and_then(|()| {
                retry_worker
                    .try_send(WorkerCommand::RetryDocumentJob { job_id })
                    .map_err(|error| TranslationError::new(ErrorKind::Internal, error.to_string()))
            });
        match result {
            Ok(()) => retry_bindings
                .document_job_state
                .set(Some(DocumentJobState::Running)),
            Err(error) => state.record_client_error(error.message),
        }
        refresh_ui(&retry_bindings, &state);
    });

    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    let export_worker = Rc::clone(worker);
    bindings.export_output.connect_clicked(move |_| {
        begin_translation_export(&export_bindings, &export_state, &export_worker);
    });

    let open_output_bindings = bindings.clone();
    let open_output_state = Rc::clone(state);
    bindings.open_output.connect_clicked(move |_| {
        let Some(uri) = open_output_bindings.output_uri.borrow().clone() else {
            return;
        };
        if gtk::gio::AppInfo::launch_default_for_uri(&uri, None::<&gtk::gio::AppLaunchContext>)
            .is_err()
        {
            show_file_export_error(
                &open_output_bindings,
                &localization::text(
                    open_output_state.borrow().locale(),
                    "error.output_open",
                    "The exported output could not be opened.",
                ),
            );
        }
    });

    let stop_bindings = bindings.clone();
    let stop_state = Rc::clone(state);
    let stop_worker = Rc::clone(worker);
    bindings.stop.connect_clicked(move |_| {
        let mut state = stop_state.borrow_mut();
        if let Some(job_id) = stop_bindings.document_job_id.borrow().clone() {
            if state.request_cancellation().is_ok()
                && let Err(error) =
                    stop_worker.try_send(WorkerCommand::CancelDocumentJob { job_id })
            {
                state.record_client_error(error.to_string());
            }
        } else {
            let can_cancel = if state.status() == AppStatus::Connecting {
                true
            } else {
                state.request_cancellation().is_ok()
            };
            if can_cancel && let Err(error) = stop_worker.try_send(WorkerCommand::Cancel) {
                state.record_client_error(error.to_string());
            }
        }
        refresh_ui(&stop_bindings, &state);
    });
}

// 通过 GTK 原生文件对话框选择文本文件，读取工作放在 GIO 异步路径中。
fn begin_source_file_open(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &Rc<CoreWorker>,
) {
    let locale = state.borrow().locale();
    let filter_name = localization::text(locale, "file.filter.text", "Text files");
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&filter_name));
    filter.add_mime_type("text/plain");
    filter.add_suffix("txt");
    filter.add_suffix("md");
    filter.add_suffix("markdown");
    filter.add_suffix("srt");
    filter.add_suffix("vtt");
    filter.add_mime_type("text/html");
    filter.add_suffix("html");
    filter.add_suffix("htm");
    filter.add_mime_type("text/csv");
    filter.add_suffix("csv");
    filter.add_mime_type("application/json");
    filter.add_suffix("json");
    filter.add_mime_type("application/vnd.openxmlformats-officedocument.wordprocessingml.document");
    filter.add_suffix("docx");
    filter
        .add_mime_type("application/vnd.openxmlformats-officedocument.presentationml.presentation");
    filter.add_suffix("pptx");
    filter.add_mime_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
    filter.add_suffix("xlsx");
    filter.add_mime_type("application/epub+zip");
    filter.add_suffix("epub");
    filter.add_mime_type("application/pdf");
    filter.add_suffix("pdf");
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog_title = localization::text(locale, "dialog.open_text_file", "Open text file");
    let dialog_accept = localization::text(locale, "dialog.open", "Open");
    let dialog = gtk::FileDialog::builder()
        .title(&dialog_title)
        .accept_label(&dialog_accept)
        .modal(true)
        .build();
    dialog.set_filters(Some(&filters));
    let open_bindings = bindings.clone();
    let open_state = Rc::clone(state);
    let open_worker = Rc::clone(worker);
    dialog.open(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                if std::env::var_os("LINGUAMESH_TEST_FILE_DIALOG").is_some() {
                    println!("GTK file chooser application fixture received the selected file.");
                }
                load_source_file(&file, &open_bindings, &open_state, &open_worker);
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(_) => show_file_import_error(
                &open_bindings,
                &localization::text(
                    open_state.borrow().locale(),
                    "error.file_open",
                    "The selected text file could not be opened.",
                ),
            ),
        },
    );
}

// 通过 GTK 原生文件对话框选择受限的 UTF-8 词汇表 CSV 文件。
fn begin_glossary_import(bindings: &UiBindings, state: &Rc<RefCell<AppState>>) {
    let locale = state.borrow().locale();
    let filter_name = localization::text(locale, "file.filter.csv", "CSV glossary files");
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&filter_name));
    filter.add_mime_type("text/csv");
    filter.add_suffix("csv");
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog_title = localization::text(locale, "dialog.open_glossary", "Import glossary");
    let dialog_accept = localization::text(locale, "dialog.open", "Open");
    let dialog = gtk::FileDialog::builder()
        .title(&dialog_title)
        .accept_label(&dialog_accept)
        .modal(true)
        .build();
    dialog.set_filters(Some(&filters));
    let import_bindings = bindings.clone();
    let import_state = Rc::clone(state);
    dialog.open(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => load_glossary_file(&file, &import_bindings, &import_state),
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(_) => show_file_import_error(
                &import_bindings,
                &localization::text(
                    import_state.borrow().locale(),
                    "error.glossary_import",
                    "The glossary CSV could not be imported.",
                ),
            ),
        },
    );
}

// 将词汇表 CSV 读取限制在领域层上限内，并只把安全的错误文本反馈给界面。
#[allow(clippy::single_match_else)]
fn load_glossary_file(file: &gtk::gio::File, bindings: &UiBindings, state: &Rc<RefCell<AppState>>) {
    let bytes_read = Rc::new(Cell::new(0_usize));
    let too_large = Rc::new(Cell::new(false));
    let read_bytes = Rc::clone(&bytes_read);
    let read_too_large = Rc::clone(&too_large);
    let load_bindings = bindings.clone();
    let load_state = Rc::clone(state);
    file.load_partial_contents_async(
        None::<&gtk::gio::Cancellable>,
        move |chunk| {
            let next = read_bytes.get().saturating_add(chunk.len());
            if next > MAX_GLOSSARY_CSV_BYTES {
                read_too_large.set(true);
                false
            } else {
                read_bytes.set(next);
                true
            }
        },
        move |result| {
            if too_large.get() {
                show_file_import_error(
                    &load_bindings,
                    &localization::text(
                        load_state.borrow().locale(),
                        "error.glossary_import",
                        "The glossary CSV could not be imported.",
                    ),
                );
                return;
            }
            let glossary = match result {
                Ok((contents, _)) => {
                    let bytes = contents.as_ref();
                    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
                    let Ok(text) = std::str::from_utf8(bytes) else {
                        show_file_import_error(
                            &load_bindings,
                            &localization::text(
                                load_state.borrow().locale(),
                                "error.glossary_import",
                                "The glossary CSV could not be imported.",
                            ),
                        );
                        return;
                    };
                    match Glossary::from_csv(text) {
                        Ok(glossary) => glossary,
                        Err(_) => {
                            show_file_import_error(
                                &load_bindings,
                                &localization::text(
                                    load_state.borrow().locale(),
                                    "error.glossary_import",
                                    "The glossary CSV could not be imported.",
                                ),
                            );
                            return;
                        }
                    }
                }
                Err(_) => {
                    show_file_import_error(
                        &load_bindings,
                        &localization::text(
                            load_state.borrow().locale(),
                            "error.glossary_import",
                            "The glossary CSV could not be imported.",
                        ),
                    );
                    return;
                }
            };
            let summary = glossary
                .entries()
                .iter()
                .map(|entry| format!("{} => {}", entry.source_term, entry.target_term))
                .collect::<Vec<_>>()
                .join("; ");
            load_bindings.glossary.set_text(&summary);
            load_bindings.glossary_from_csv.set(true);
            load_bindings.glossary_notice.set(true);
            load_state.borrow_mut().set_glossary(Some(glossary));
            load_bindings.error.set_label("");
            load_bindings.error.set_visible(false);
            refresh_ui(&load_bindings, &load_state.borrow());
        },
    );
}

// 将当前内存中的词汇表以确定性 UTF-8 CSV 写入用户选择的新文件。
#[allow(clippy::single_match_else)]
fn begin_glossary_export(bindings: &UiBindings, state: &Rc<RefCell<AppState>>) {
    let locale = state.borrow().locale();
    let source_locale = SOURCE_LOCALES[bindings.source_locale.selected() as usize];
    let target_locale = TARGET_LOCALES[bindings.target_locale.selected() as usize];
    let glossary = if bindings.glossary_from_csv.get() {
        state.borrow().glossary().cloned()
    } else {
        match parse_glossary(
            bindings.glossary.text().as_str(),
            source_locale,
            target_locale,
        ) {
            Ok(glossary) => glossary,
            Err(_) => {
                show_file_export_error(
                    bindings,
                    &localization::text(
                        locale,
                        "error.glossary_export",
                        "The glossary CSV could not be saved.",
                    ),
                );
                return;
            }
        }
    };
    let Some(glossary) = glossary else {
        show_file_export_error(
            bindings,
            &localization::text(
                locale,
                "error.glossary_export_empty",
                "Enter or import glossary rules before exporting.",
            ),
        );
        return;
    };
    let contents = glossary.to_csv().into_bytes();
    let dialog_title = localization::text(locale, "dialog.export_glossary", "Export glossary");
    let dialog_accept = localization::text(locale, "dialog.save", "Save");
    let dialog = gtk::FileDialog::builder()
        .title(&dialog_title)
        .accept_label(&dialog_accept)
        .modal(true)
        .build();
    dialog.set_initial_name(Some("linguamesh-glossary.csv"));
    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    dialog.save(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                let callback_bindings = export_bindings.clone();
                let callback_state = Rc::clone(&export_state);
                write_new_file_async(&file, contents, move |write_succeeded| {
                    if write_succeeded {
                        callback_bindings.glossary_notice.set(true);
                        refresh_ui(&callback_bindings, &callback_state.borrow());
                    } else {
                        show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.glossary_export",
                                "The glossary CSV could not be saved.",
                            ),
                        );
                    }
                });
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(_) => show_file_export_error(
                &export_bindings,
                &localization::text(
                    export_state.borrow().locale(),
                    "error.glossary_export",
                    "The glossary CSV could not be saved.",
                ),
            ),
        },
    );
}

// 判断导出目标是否通过相同路径、符号链接或硬链接指向源文件。
fn destination_matches_source(source_uri: Option<&str>, destination: &gtk::gio::File) -> bool {
    let Some(source_uri) = source_uri else {
        return false;
    };
    let source = gtk::gio::File::for_uri(source_uri);
    if source.equal(destination) {
        return true;
    }
    let (Some(source_path), Some(destination_path)) = (source.path(), destination.path()) else {
        return false;
    };
    if source_path == destination_path {
        return true;
    }
    if let (Ok(source_path), Ok(destination_path)) = (
        fs::canonicalize(&source_path),
        fs::canonicalize(&destination_path),
    ) && source_path == destination_path
    {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if let (Ok(source_metadata), Ok(destination_metadata)) =
            (fs::metadata(source_path), fs::metadata(destination_path))
        {
            return source_metadata.dev() == destination_metadata.dev()
                && source_metadata.ino() == destination_metadata.ino();
        }
    }
    false
}

// 将源文件名和目标语言规范化为安全、可读且稳定的导出文件名。
fn translation_output_name(source_name: &str, target_locale: &str) -> String {
    let source_path = Path::new(source_name);
    let basename = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("translation.txt");
    let base = source_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(basename);
    let base = if basename
        .strip_prefix('.')
        .is_some_and(|name| !name.is_empty() && !name.contains('.'))
    {
        "translation"
    } else {
        base
    };
    let base = sanitize_output_component(base, "translation");
    let target = target_locale
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    let target = sanitize_output_component(&target, "und");
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map_or_else(
            || "txt".to_owned(),
            |extension| sanitize_output_component(extension, "txt"),
        );
    format!("{base}.{target}.{extension}")
}

// 移除控制字符和路径分隔符，避免用户可控名称逃逸导出目录。
fn sanitize_output_component(value: &str, fallback: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '/' | '\\') {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches(|character| character == '.' || character == ' ');
    if trimmed.is_empty() {
        fallback.to_owned()
    } else {
        trimmed.to_owned()
    }
}

// 为已存在的目标文件分配从 -1 开始的确定性后缀，避免覆盖任何现有文件。
fn collision_safe_output_path(destination: &Path) -> Option<PathBuf> {
    let path_available = |path: &Path| {
        matches!(
            fs::symlink_metadata(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        )
    };
    if path_available(destination) {
        return Some(destination.to_owned());
    }
    let parent = destination.parent()?;
    let stem = destination.file_stem()?.to_str()?;
    let extension = destination
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.is_empty());
    for suffix in 1..=9999 {
        let filename = extension.map_or_else(
            || format!("{stem}-{suffix}"),
            |extension| format!("{stem}-{suffix}.{extension}"),
        );
        let candidate = parent.join(filename);
        if path_available(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExportWriteStrategy {
    LocalAtomicRename,
    ExclusiveCreate,
}

// 非本地 URI 没有可验证的同目录路径，只能使用 GIO 独占创建保持不覆盖语义。
fn export_write_strategy(destination: &gtk::gio::File) -> ExportWriteStrategy {
    match destination.path() {
        Some(path) if path.parent().is_some() => ExportWriteStrategy::LocalAtomicRename,
        _ => ExportWriteStrategy::ExclusiveCreate,
    }
}

// 将 GIO 目标转换为不会覆盖已有本地文件的确定性路径。
fn collision_safe_destination(destination: &gtk::gio::File) -> Option<gtk::gio::File> {
    destination.path().map_or_else(
        || Some(destination.clone()),
        |path| collision_safe_output_path(&path).map(|path| gtk::gio::File::for_path(&path)),
    )
}

// 以独占创建和异步写入保存新文件，避免检查与写入之间的竞争覆盖已有文件。
fn write_contents_to_new_file_async(
    destination: &gtk::gio::File,
    contents: Vec<u8>,
    callback: impl FnOnce(bool) + 'static,
) {
    let destination = destination.clone();
    destination.create_async(
        gtk::gio::FileCreateFlags::NONE,
        glib::Priority::DEFAULT,
        None::<&gtk::gio::Cancellable>,
        move |create_result| match create_result {
            Ok(stream) => stream.write_all_async(
                contents,
                glib::Priority::DEFAULT,
                None::<&gtk::gio::Cancellable>,
                {
                    let close_stream = stream.clone();
                    move |write_result| {
                        let write_succeeded = matches!(write_result, Ok((_, _, None)));
                        close_stream.close_async(
                            glib::Priority::DEFAULT,
                            None::<&gtk::gio::Cancellable>,
                            move |close_result| callback(write_succeeded && close_result.is_ok()),
                        );
                    }
                },
            ),
            Err(_) => callback(false),
        },
    );
}

// 先写入同目录临时文件，再以 GIO 非覆盖移动完成本地输出，避免半成品可见。
fn write_new_file_async(
    destination: &gtk::gio::File,
    contents: Vec<u8>,
    callback: impl FnOnce(bool) + 'static,
) {
    if export_write_strategy(destination) == ExportWriteStrategy::ExclusiveCreate {
        write_contents_to_new_file_async(destination, contents, callback);
        return;
    }
    let Some(destination_path) = destination.path() else {
        write_contents_to_new_file_async(destination, contents, callback);
        return;
    };
    let Some(parent) = destination_path.parent() else {
        write_contents_to_new_file_async(destination, contents, callback);
        return;
    };
    let temporary_path = parent.join(format!(
        ".linguamesh-export-{}.tmp",
        glib::uuid_string_random()
    ));
    let temporary = gtk::gio::File::for_path(temporary_path);
    let destination = destination.clone();
    let temporary_for_write_cleanup = temporary.clone();
    let temporary_for_move_cleanup = temporary.clone();
    let temporary_for_write = temporary.clone();
    write_contents_to_new_file_async(&temporary_for_write, contents, move |write_succeeded| {
        if !write_succeeded {
            temporary_for_write_cleanup.delete_async(
                glib::Priority::DEFAULT,
                None::<&gtk::gio::Cancellable>,
                move |_| callback(false),
            );
            return;
        }
        temporary.move_async(
            &destination,
            gtk::gio::FileCopyFlags::NONE,
            glib::Priority::DEFAULT,
            None::<&gtk::gio::Cancellable>,
            None,
            move |move_result| {
                if move_result.is_ok() {
                    callback(true);
                    return;
                }
                temporary_for_move_cleanup.delete_async(
                    glib::Priority::DEFAULT,
                    None::<&gtk::gio::Cancellable>,
                    move |_| callback(false),
                );
            },
        );
    });
}

// 从已导入 URI 提取原始文件名；纯文本编辑器没有源文件时使用稳定回退名。
fn source_name_for_export(source_uri: Option<&str>) -> String {
    source_uri
        .and_then(|uri| gtk::gio::File::for_uri(uri).basename())
        .map_or_else(
            || "translation.txt".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
}

// 将译文异步写入用户选择的新文件，并拒绝覆盖已导入的源文件。
#[allow(clippy::too_many_lines)]
fn begin_translation_export(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &Rc<CoreWorker>,
) {
    let (locale, target_locale) = {
        let state = state.borrow();
        (state.locale(), state.target_locale().to_owned())
    };
    if let Some(job_id) = bindings.document_job_id.borrow().clone() {
        if let Err(error) = worker.try_send(WorkerCommand::ExportDocumentJob { job_id }) {
            state.borrow_mut().record_client_error(error.to_string());
            refresh_ui(bindings, &state.borrow());
        }
        return;
    }
    let output = bindings
        .output
        .text(
            &bindings.output.start_iter(),
            &bindings.output.end_iter(),
            true,
        )
        .to_string();
    if output.is_empty() {
        show_file_export_error(
            bindings,
            &localization::text(
                locale,
                "error.file_export_empty",
                "Translate some text before exporting the output.",
            ),
        );
        return;
    }
    let dialog_title =
        localization::text(locale, "dialog.export_translation", "Export translation");
    let dialog_accept = localization::text(locale, "dialog.save", "Save");
    let dialog = gtk::FileDialog::builder()
        .title(&dialog_title)
        .accept_label(&dialog_accept)
        .modal(true)
        .build();
    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    let source_uri = bindings.source_uri.borrow().clone();
    let default_name = translation_output_name(
        &source_name_for_export(source_uri.as_deref()),
        &target_locale,
    );
    dialog.set_initial_name(Some(&default_name));
    dialog.save(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                if destination_matches_source(source_uri.as_deref(), &file) {
                    show_file_export_error(
                        &export_bindings,
                        &localization::text(
                            export_state.borrow().locale(),
                            "error.file_export_source",
                            "Choose a different file so the source remains unchanged.",
                        ),
                    );
                    return;
                }
                let Some(file) = collision_safe_destination(&file) else {
                    show_file_export_error(
                        &export_bindings,
                        &localization::text(
                            export_state.borrow().locale(),
                            "error.file_export",
                            "The translated output could not be saved.",
                        ),
                    );
                    return;
                };
                let destination_uri = file.uri().to_string();
                let output_uri = destination_uri.clone();
                let callback_bindings = export_bindings.clone();
                let callback_state = Rc::clone(&export_state);
                write_new_file_async(&file, output.into_bytes(), move |write_succeeded| {
                    if write_succeeded {
                        *callback_bindings.output_uri.borrow_mut() = Some(output_uri.clone());
                        callback_bindings.export_notice.set(true);
                        refresh_ui(&callback_bindings, &callback_state.borrow());
                    } else {
                        show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.file_export",
                                "The translated output could not be saved.",
                            ),
                        );
                    }
                });
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(_) => show_file_export_error(
                &export_bindings,
                &localization::text(
                    export_state.borrow().locale(),
                    "error.file_export",
                    "The translated output could not be saved.",
                ),
            ),
        },
    );
}

// 将报告字段折叠为单行，避免源文件名或 warning 破坏 TSV 结构。
fn report_field(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\t' | '\r' | '\n' => ' ',
            _ => character,
        })
        .collect()
}

// 根据已持久化的文档段生成不含正文的本地 usage 估算。
fn document_usage_report(snapshot: &DocumentJobSnapshot) -> String {
    let source = snapshot
        .job
        .segments
        .iter()
        .map(|segment| segment.source_text.as_str())
        .collect::<String>();
    let translated = snapshot
        .job
        .segments
        .iter()
        .filter_map(|segment| segment.translated_text.as_deref())
        .collect::<String>();
    let usage = UsageRecord::locally_estimated(&source, &translated);
    format!(
        "{{\"source\":\"locally_estimated\",\"input_tokens\":{},\"output_tokens\":{},\"total_tokens\":{}}}",
        usage.input_tokens.unwrap_or_default(),
        usage.output_tokens.unwrap_or_default(),
        usage.total_tokens.unwrap_or_default()
    )
}

// 生成不含凭据、源正文或本地路径的确定性文档翻译报告。
fn document_translation_report(snapshot: &DocumentJobSnapshot, core_version: &str) -> String {
    let options = snapshot.options.as_ref();
    let completed = snapshot
        .job
        .segments
        .iter()
        .filter(|segment| {
            segment.kind == DocumentSegmentKind::Prose && segment.translated_text.is_some()
        })
        .count();
    let skipped = snapshot
        .job
        .segments
        .iter()
        .filter(|segment| segment.kind == DocumentSegmentKind::Verbatim)
        .count();
    let pending = snapshot.job.pending_count();
    let failed = usize::from(snapshot.state == DocumentJobState::Failed);
    let warnings = snapshot.job.warnings().unwrap_or_default();
    let warning_text = if warnings.is_empty() {
        "none".to_owned()
    } else {
        warnings
            .iter()
            .map(|warning| format!("{:?}", warning.kind))
            .collect::<Vec<_>>()
            .join(",")
    };
    let source_locale = options
        .and_then(|options| options.source_locale.as_deref())
        .unwrap_or("auto");
    let target_locale = options.map_or("unknown", |options| options.target_locale.as_str());
    let provider = options.map_or("unavailable", |options| options.provider_id.as_str());
    let model = options.map_or("unavailable", |options| options.model_id.as_str());
    let routing = options
        .and_then(|options| options.routing_profile_id.as_deref())
        .map_or_else(|| "manual".to_owned(), |id| format!("profile:{id}"));
    let preset = options.map_or("unavailable", |options| {
        options.translation_preset.id.as_str()
    });
    let glossary = options
        .and_then(|options| options.glossary.as_ref())
        .map_or_else(
            || "none".to_owned(),
            |glossary| format!("entries:{}", glossary.entries().len()),
        );
    let state = format!("{:?}", snapshot.state);
    let output_target_locale = options.map_or("und", |options| options.target_locale.as_str());
    let output_identifier =
        translation_output_name(&snapshot.job.source_name, output_target_locale);
    let fields = [
        ("report_version", "1".to_owned()),
        ("source_identifier", snapshot.job.source_name.clone()),
        ("output_identifier", output_identifier),
        ("source_locale", source_locale.to_owned()),
        ("target_locale", target_locale.to_owned()),
        ("provider", provider.to_owned()),
        ("model", model.to_owned()),
        ("routing_decision", routing),
        ("translation_preset", preset.to_owned()),
        ("glossary_version", glossary),
        ("application_version", env!("CARGO_PKG_VERSION").to_owned()),
        ("core_version", core_version.to_owned()),
        (
            "prompt_template_version",
            "translation-prompt-v1".to_owned(),
        ),
        ("state", state),
        ("segment_total", snapshot.job.segments.len().to_string()),
        ("completed_count", completed.to_string()),
        ("skipped_count", skipped.to_string()),
        ("pending_count", pending.to_string()),
        ("retried_count", "unknown".to_owned()),
        ("failed_count", failed.to_string()),
        ("warnings", warning_text),
        ("usage", document_usage_report(snapshot)),
        ("start_time_unix_seconds", snapshot.created_at.to_string()),
        (
            "completion_time_unix_seconds",
            snapshot.updated_at.to_string(),
        ),
    ];
    let mut report = String::from("field\tvalue\n");
    for (key, value) in fields {
        report.push_str(key);
        report.push('\t');
        report.push_str(&report_field(&value));
        report.push('\n');
    }
    report
}

// 将选中文档任务的安全报告异步写入用户指定的新文件。
fn begin_document_report_export(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    snapshot: &DocumentJobSnapshot,
) {
    let locale = state.borrow().locale();
    bindings.report_export_notice.set(false);
    let report = document_translation_report(
        snapshot,
        &core_compatibility().map_or_else(
            |_| "unavailable".to_owned(),
            |compatibility| compatibility.core_version,
        ),
    );
    let target_locale = snapshot
        .options
        .as_ref()
        .map_or("und", |options| options.target_locale.as_str());
    let report_name = format!(
        "{}.report.tsv",
        translation_output_name(&snapshot.job.source_name, target_locale)
    );
    let dialog = gtk::FileDialog::builder()
        .title(localization::text(
            locale,
            "dialog.export_translation",
            "Export translation",
        ))
        .accept_label(localization::text(locale, "dialog.save", "Save"))
        .modal(true)
        .build();
    dialog.set_initial_name(Some(&report_name));
    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    let source_uri = bindings.source_uri.borrow().clone();
    dialog.save(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                if destination_matches_source(source_uri.as_deref(), &file) {
                    show_file_export_error(
                        &export_bindings,
                        &localization::text(
                            export_state.borrow().locale(),
                            "error.file_export_source",
                            "Choose a different file so the source remains unchanged.",
                        ),
                    );
                    return;
                }
                let Some(file) = collision_safe_destination(&file) else {
                    show_file_export_error(
                        &export_bindings,
                        &localization::text(
                            export_state.borrow().locale(),
                            "error.file_export",
                            "The translated output could not be saved.",
                        ),
                    );
                    return;
                };
                let callback_bindings = export_bindings.clone();
                let callback_state = Rc::clone(&export_state);
                write_new_file_async(&file, report.clone().into_bytes(), move |write_succeeded| {
                    if write_succeeded {
                        callback_bindings.report_export_notice.set(true);
                        refresh_ui(&callback_bindings, &callback_state.borrow());
                    } else {
                        show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.file_export",
                                "The translated output could not be saved.",
                            ),
                        );
                    }
                });
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(_) => show_file_export_error(
                &export_bindings,
                &localization::text(
                    export_state.borrow().locale(),
                    "error.file_export",
                    "The translated output could not be saved.",
                ),
            ),
        },
    );
}

// 将已完成的二进制文档任务写入新文件，并拒绝覆盖导入源文件。
fn begin_document_binary_export(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    source_name: &str,
    target_locale: &str,
    contents: Vec<u8>,
) {
    let locale = state.borrow().locale();
    let default_name = translation_output_name(source_name, target_locale);
    let dialog = gtk::FileDialog::builder()
        .title(localization::text(
            locale,
            "dialog.export_translation",
            "Export translation",
        ))
        .accept_label(localization::text(locale, "dialog.save", "Save"))
        .modal(true)
        .build();
    dialog.set_initial_name(Some(&default_name));
    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    let source_uri = bindings.source_uri.borrow().clone();
    dialog.save(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                if destination_matches_source(source_uri.as_deref(), &file) {
                    show_file_export_error(
                        &export_bindings,
                        &localization::text(
                            export_state.borrow().locale(),
                            "error.file_export_source",
                            "Choose a different file so the source remains unchanged.",
                        ),
                    );
                    return;
                }
                let Some(file) = collision_safe_destination(&file) else {
                    show_file_export_error(
                        &export_bindings,
                        &localization::text(
                            export_state.borrow().locale(),
                            "error.file_export",
                            "The translated output could not be saved.",
                        ),
                    );
                    return;
                };
                let destination_uri = file.uri().to_string();
                let output_uri = destination_uri.clone();
                let callback_bindings = export_bindings.clone();
                let callback_state = Rc::clone(&export_state);
                write_new_file_async(&file, contents, move |write_succeeded| {
                    if write_succeeded {
                        *callback_bindings.output_uri.borrow_mut() = Some(output_uri.clone());
                        callback_bindings.export_notice.set(true);
                        refresh_ui(&callback_bindings, &callback_state.borrow());
                    } else {
                        show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.file_export",
                                "The translated output could not be saved.",
                            ),
                        );
                    }
                });
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(_) => show_file_export_error(
                &export_bindings,
                &localization::text(
                    export_state.borrow().locale(),
                    "error.file_export",
                    "The translated output could not be saved.",
                ),
            ),
        },
    );
}

// 通过 GIO 的分块异步读取限制内存占用，并在主线程完成 UTF-8 解码。
#[allow(clippy::single_match_else, clippy::too_many_lines)]
fn load_source_file(
    file: &gtk::gio::File,
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &Rc<CoreWorker>,
) {
    let bytes_read = Rc::new(Cell::new(0_usize));
    let too_large = Rc::new(Cell::new(false));
    let source_uri = file.uri().to_string();
    let Ok(source_lease) = FileLease::desktop_path(source_uri.clone()) else {
        show_file_import_error(
            bindings,
            &localization::text(
                state.borrow().locale(),
                "error.file_open",
                "The selected text file could not be opened.",
            ),
        );
        return;
    };
    let source_name = file.basename().map_or_else(
        || "source.txt".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let read_bytes = Rc::clone(&bytes_read);
    let read_too_large = Rc::clone(&too_large);
    let lease_expired = Rc::new(Cell::new(false));
    let read_lease_expired = Rc::clone(&lease_expired);
    let read_lease = source_lease.clone();
    let import_failed = Rc::new(Cell::new(false));
    let load_import_failed = Rc::clone(&import_failed);
    let load_bindings = bindings.clone();
    let load_state = Rc::clone(state);
    let load_worker = Rc::clone(worker);
    load_bindings.export_notice.set(false);
    load_bindings.report_export_notice.set(false);
    *load_bindings.output_uri.borrow_mut() = None;
    file.load_partial_contents_async(
        None::<&gtk::gio::Cancellable>,
        move |chunk| {
            if !read_lease.is_active() {
                read_lease_expired.set(true);
                return false;
            }
            let next = read_bytes.get().saturating_add(chunk.len());
            if next > file_import::MAX_TEXT_FILE_BYTES {
                read_too_large.set(true);
                false
            } else {
                read_bytes.set(next);
                true
            }
        },
        move |result| {
            if lease_expired.get() {
                show_file_import_error(
                    &load_bindings,
                    &localization::text(
                        load_state.borrow().locale(),
                        "error.file_open",
                        "The selected text file could not be opened.",
                    ),
                );
                return;
            }
            if too_large.get() {
                show_file_import_error(
                    &load_bindings,
                    &localization::text(
                        load_state.borrow().locale(),
                        "error.file_too_large",
                        "The selected text file exceeds the 4 MiB limit.",
                    ),
                );
                return;
            }
            match result {
                Ok((contents, _)) => {
                    match file_import::decode_document_job_with_lease(
                        &source_lease,
                        &source_name,
                        contents.as_ref(),
                    ) {
                    Ok(job) => {
                        source_lease.revoke();
                        let warnings = job.warnings().unwrap_or_default();
                        if std::env::var_os("LINGUAMESH_TEST_FILE_DIALOG").is_some() {
                            println!("GTK file chooser application fixture completed the asynchronous GIO read.");
                        }
                        let job_id = OperationId::new().as_str().to_owned();
                        let image_only_pdf = matches!(job.format, DocumentFormat::Pdf)
                            && job.pending_count() == 0
                            && warnings.iter().any(|warning| {
                                warning.kind == DocumentWarningKind::PdfImageOnlyPage
                            });
                        *load_bindings.source_uri.borrow_mut() = Some(source_uri.clone());
                        if image_only_pdf && load_bindings.ocr_enabled.is_active() {
                            load_bindings.ocr_pending.set(true);
                            if let Err(error) = load_worker.try_send(WorkerCommand::OcrDocumentJob {
                                job_id,
                                source_name: source_name.clone(),
                                contents: contents.as_ref().to_vec(),
                            }) {
                                load_bindings.ocr_pending.set(false);
                                load_state.borrow_mut().record_client_error(error.to_string());
                            }
                            refresh_ui(&load_bindings, &load_state.borrow());
                            return;
                        }
                        let text = job.source_text();
                        if let Err(error) = load_worker.try_send(WorkerCommand::CreateDocumentJob {
                            job_id: job_id.clone(),
                            job,
                        }) {
                            load_state.borrow_mut().record_client_error(error.to_string());
                            refresh_ui(&load_bindings, &load_state.borrow());
                            return;
                        }
                        load_bindings.document_job_guard.set(true);
                        load_bindings.source.set_text(&text);
                        load_bindings.document_job_guard.set(false);
                        *load_bindings.document_job_id.borrow_mut() = Some(job_id);
                        *load_bindings.document_warnings.borrow_mut() = warnings;
                        load_bindings.error.set_label("");
                        load_bindings.error.set_visible(false);
                    }
                    Err(error) => {
                        load_import_failed.set(true);
                        let locale = load_state.borrow().locale();
                        let (key, fallback) = match error {
                            file_import::TextImportError::TooLarge => (
                                "error.file_too_large",
                                "The selected text file exceeds the 4 MiB limit.",
                            ),
                            file_import::TextImportError::InvalidUtf8 => (
                                "error.file.invalid_utf8",
                                "The selected file is not valid UTF-8 text.",
                            ),
                            file_import::TextImportError::UnsupportedFormat => (
                                "error.file_open",
                                "The selected document format is not supported.",
                            ),
                            file_import::TextImportError::InvalidStructure
                            | file_import::TextImportError::LeaseExpired => (
                                "error.file_open",
                                "The selected document structure is invalid.",
                            ),
                        };
                        let message = localization::text(locale, key, fallback);
                        show_file_import_error(&load_bindings, &message);
                    }
                    }
                }
                Err(_) => {
                    load_import_failed.set(true);
                    show_file_import_error(
                        &load_bindings,
                        &localization::text(
                            load_state.borrow().locale(),
                            "error.file_read",
                            "The selected text file could not be read.",
                        ),
                    );
                }
            }
            if !load_import_failed.get() {
                refresh_ui(&load_bindings, &load_state.borrow());
            }
        },
    );
}

// 导入错误只显示安全的固定文本，不把路径或文件内容写入诊断状态。
fn show_file_import_error(bindings: &UiBindings, message: &str) {
    bindings.error.set_label(message);
    bindings.error.set_visible(true);
    bindings.error.reset_state(gtk::AccessibleState::Hidden);
}

fn show_file_export_error(bindings: &UiBindings, message: &str) {
    bindings.error.set_label(message);
    bindings.error.set_visible(true);
    bindings.error.reset_state(gtk::AccessibleState::Hidden);
}

// 将队列中的任务选为当前编辑器任务并同步其源文本和状态。
// 将持久化的文档翻译选项恢复到当前编辑状态。
fn restore_document_translation_options(
    state: &Rc<RefCell<AppState>>,
    snapshot: &DocumentJobSnapshot,
) {
    if let Some(options) = snapshot.options.as_ref() {
        let mut state = state.borrow_mut();
        state.set_quality_mode(options.quality_mode);
        state.set_translation_preset(options.translation_preset.clone());
    }
}

fn select_document_job(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    selected: &DocumentJobSnapshot,
) {
    *bindings.document_job_id.borrow_mut() = Some(selected.job_id.clone());
    bindings.document_job_state.set(Some(selected.state));
    bindings
        .document_progress
        .set(Some(document_progress(selected)));
    let source_text = selected.job.source_text();
    bindings.document_job_guard.set(true);
    bindings.source.set_text(&source_text);
    bindings.document_job_guard.set(false);
    *bindings.document_warnings.borrow_mut() = selected.job.warnings().unwrap_or_default();
    restore_document_translation_options(state, selected);
    state.borrow_mut().set_source_text(&source_text);
    refresh_ui(bindings, &state.borrow());
}

// 将核心路由模式映射到可本地化的短标签。
fn localized_routing_mode(locale: UiLocale, mode: RoutingMode) -> String {
    let (key, fallback) = match mode {
        RoutingMode::Manual => ("routing.mode.manual", "Manual"),
        RoutingMode::Ordered => ("routing.mode.ordered", "Ordered"),
        RoutingMode::Automatic => ("routing.mode.automatic", "Automatic"),
    };
    localization::text(locale, key, fallback)
}

// 将核心路由模式映射回 GTK 下拉框的稳定索引。
fn routing_mode_selection(mode: RoutingMode) -> u32 {
    match mode {
        RoutingMode::Manual => 0,
        RoutingMode::Ordered => 1,
        RoutingMode::Automatic => 2,
    }
}

// 将自动路由偏好映射到稳定的 GTK 下拉框索引。
fn routing_preference_selection(preference: RoutingPreference) -> u32 {
    match preference {
        RoutingPreference::None => 0,
        RoutingPreference::Local => 1,
        RoutingPreference::Quality => 2,
        RoutingPreference::Latency => 3,
        RoutingPreference::Cost => 4,
    }
}

// 将 GTK 下拉框索引安全地还原为核心路由偏好。
fn routing_preference_for_selection(selection: u32) -> RoutingPreference {
    match selection {
        1 => RoutingPreference::Local,
        2 => RoutingPreference::Quality,
        3 => RoutingPreference::Latency,
        4 => RoutingPreference::Cost,
        _ => RoutingPreference::None,
    }
}

// 聚合路由编辑器控件，避免恢复函数的参数顺序产生错误。
#[derive(Clone, Copy)]
struct RoutingEditorWidgets<'a> {
    mode: &'a gtk::DropDown,
    preference: &'a gtk::DropDown,
    allow_fallback: &'a gtk::CheckButton,
    local_only: &'a gtk::CheckButton,
    allow_remote: &'a gtk::CheckButton,
    privacy_sensitive: &'a gtk::CheckButton,
    require_streaming: &'a gtk::CheckButton,
    require_document: &'a gtk::CheckButton,
    provider_allowlist: &'a gtk::Entry,
    provider_denylist: &'a gtk::Entry,
    model_allowlist: &'a gtk::Entry,
    model_denylist: &'a gtk::Entry,
    minimum_quality_tier: &'a gtk::Entry,
    max_request_bytes: &'a gtk::Entry,
}

// 聚合路由约束的文本输入框，避免保存时混用控件顺序。
#[derive(Clone, Copy)]
struct RoutingConstraintTextWidgets<'a> {
    provider_allowlist: &'a gtk::Entry,
    provider_denylist: &'a gtk::Entry,
    model_allowlist: &'a gtk::Entry,
    model_denylist: &'a gtk::Entry,
    minimum_quality_tier: &'a gtk::Entry,
    max_request_bytes: &'a gtk::Entry,
}

// 聚合已解析前的路由约束文本，供 GTK 和无界面回归测试共同使用。
#[derive(Clone, Copy)]
struct RoutingConstraintTextValues<'a> {
    provider_allowlist: &'a str,
    provider_denylist: &'a str,
    model_allowlist: &'a str,
    model_denylist: &'a str,
    minimum_quality_tier: &'a str,
    max_request_bytes: &'a str,
}

// 将路由标识列表以稳定的逗号格式展示在编辑器中。
fn routing_identifier_list_text(values: &[String]) -> String {
    values.join(", ")
}

// 解析编辑器中的逗号分隔路由标识列表，并拒绝空项或非法标识。
fn routing_identifier_list_from_text(value: &str) -> Result<Vec<String>, ()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    for item in trimmed.split(',') {
        let item = item.trim();
        if !valid_routing_profile_id(item) {
            return Err(());
        }
        values.push(item.to_owned());
    }
    Ok(values)
}

// 将可选的质量等级或字节上限解析为 Core 约束值。
fn routing_optional_limit_from_text(value: &str) -> Result<Option<usize>, ()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = trimmed.parse::<usize>().map_err(|_| ())?;
    if parsed == 0 {
        return Err(());
    }
    Ok(Some(parsed))
}

// 创建带有本地化标签和提示的路由文本约束输入框。
fn routing_text_entry(
    locale: UiLocale,
    label_key: &str,
    label_fallback: &str,
    tooltip_key: &str,
    tooltip_fallback: &str,
) -> (gtk::Entry, gtk::Box) {
    let entry = gtk::Entry::new();
    entry.set_hexpand(true);
    entry.set_focusable(true);
    entry.set_tooltip_text(Some(&localization::text(
        locale,
        tooltip_key,
        tooltip_fallback,
    )));
    let label = localized_mnemonic(locale, label_key, label_fallback);
    let control = labeled_control(&label, entry.upcast_ref::<gtk::Widget>());
    (entry, control)
}

// 仅更新编辑器暴露的约束，同时保留核心未来字段以保证编辑回写不丢数据。
fn routing_constraints_from_controls(
    existing: Option<&RoutingConstraints>,
    selected: &RoutingConstraints,
) -> RoutingConstraints {
    let mut constraints = existing.cloned().unwrap_or_default();
    constraints.preference = selected.preference;
    constraints.local_only = selected.local_only;
    constraints.allow_remote = selected.allow_remote;
    constraints.privacy_sensitive = selected.privacy_sensitive;
    constraints.require_streaming = selected.require_streaming;
    constraints.require_document = selected.require_document;
    constraints.explicit_fallback_allowed = selected.explicit_fallback_allowed;
    constraints
}

// 从纯文本值更新 Core 的列表和数值约束，并保留未暴露的字段。
fn routing_constraints_from_text_values(
    existing: Option<&RoutingConstraints>,
    selected: &RoutingConstraints,
    values: RoutingConstraintTextValues<'_>,
) -> Result<RoutingConstraints, ()> {
    let mut constraints = routing_constraints_from_controls(existing, selected);
    constraints.provider_allowlist = routing_identifier_list_from_text(values.provider_allowlist)?;
    constraints.provider_denylist = routing_identifier_list_from_text(values.provider_denylist)?;
    constraints.model_allowlist = routing_identifier_list_from_text(values.model_allowlist)?;
    constraints.model_denylist = routing_identifier_list_from_text(values.model_denylist)?;
    let minimum_quality_tier = routing_optional_limit_from_text(values.minimum_quality_tier)?;
    constraints.minimum_quality_tier = minimum_quality_tier
        .map(|value| u8::try_from(value).map_err(|_| ()))
        .transpose()?;
    constraints.max_request_bytes = routing_optional_limit_from_text(values.max_request_bytes)?;
    Ok(constraints)
}

// 从 GTK 文本输入框读取路由约束，并复用无界面解析路径。
fn routing_constraints_from_text_controls(
    existing: Option<&RoutingConstraints>,
    selected: &RoutingConstraints,
    widgets: RoutingConstraintTextWidgets<'_>,
) -> Result<RoutingConstraints, ()> {
    let provider_allowlist = widgets.provider_allowlist.text();
    let provider_denylist = widgets.provider_denylist.text();
    let model_allowlist = widgets.model_allowlist.text();
    let model_denylist = widgets.model_denylist.text();
    let minimum_quality_tier = widgets.minimum_quality_tier.text();
    let max_request_bytes = widgets.max_request_bytes.text();
    routing_constraints_from_text_values(
        existing,
        selected,
        RoutingConstraintTextValues {
            provider_allowlist: provider_allowlist.as_str(),
            provider_denylist: provider_denylist.as_str(),
            model_allowlist: model_allowlist.as_str(),
            model_denylist: model_denylist.as_str(),
            minimum_quality_tier: minimum_quality_tier.as_str(),
            max_request_bytes: max_request_bytes.as_str(),
        },
    )
}

// 按 Core 的标识符约束检查路由配置 ID，避免保存时才暴露无效输入。
fn valid_routing_profile_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ROUTING_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

// 防止新建配置静默覆盖已有 ID，同时允许编辑流程更新原记录。
fn routing_profile_id_conflicts(
    existing_ids: &[String],
    editing_profile_id: Option<&str>,
    profile_id: &str,
) -> bool {
    editing_profile_id.is_none() && existing_ids.iter().any(|id| id == profile_id)
}

// 将 Manual 模式的候选集合限制为当前显示顺序中的首个候选。
fn normalized_candidate_ids_for_mode(
    mode: RoutingMode,
    candidate_ids: Vec<ProviderProfileId>,
) -> Vec<ProviderProfileId> {
    if mode == RoutingMode::Manual {
        candidate_ids.into_iter().take(1).collect()
    } else {
        candidate_ids
    }
}

type RoutingCandidateControls = Rc<RefCell<Vec<(ProviderProfileId, gtk::Box, gtk::CheckButton)>>>;

// 在 Manual 模式下同步复选框，避免界面继续展示多个精确候选。
fn enforce_manual_candidate_selection(controls: &RoutingCandidateControls) {
    let mut selected = false;
    for (_, _, check) in controls.borrow().iter() {
        if check.is_active() {
            if selected {
                check.set_active(false);
            } else {
                selected = true;
            }
        }
    }
}

// 清空并按当前顺序重建路由候选行，避免 GTK 列表顺序与持久化顺序分离。
fn rebuild_routing_candidate_rows(container: &gtk::Box, controls: &RoutingCandidateControls) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    let controls = controls.borrow();
    for (_, row, _) in controls.iter() {
        container.append(row);
    }
}

// 按按钮方向移动候选行，并把新顺序留给配置创建闭包读取。
fn move_routing_candidate_row(
    container: &gtk::Box,
    controls: &RoutingCandidateControls,
    profile_id: &ProviderProfileId,
    offset: isize,
) {
    let mut controls_mut = controls.borrow_mut();
    let mut ids = controls_mut
        .iter()
        .map(|(id, _, _)| id.clone())
        .collect::<Vec<_>>();
    if !move_routing_profile_id(&mut ids, profile_id, offset) {
        return;
    }
    let mut remaining = std::mem::take(&mut *controls_mut);
    let mut reordered = Vec::with_capacity(remaining.len());
    for id in ids {
        if let Some(index) = remaining
            .iter()
            .position(|(candidate_id, _, _)| *candidate_id == id)
        {
            reordered.push(remaining.swap_remove(index));
        }
    }
    *controls_mut = reordered;
    drop(controls_mut);
    rebuild_routing_candidate_rows(container, controls);
}

// 按拖放目标移动候选行，并把新顺序留给配置创建闭包读取。
fn move_routing_candidate_row_before(
    container: &gtk::Box,
    controls: &RoutingCandidateControls,
    dragged_id: &ProviderProfileId,
    target_id: &ProviderProfileId,
) -> bool {
    let mut controls_mut = controls.borrow_mut();
    let mut ids = controls_mut
        .iter()
        .map(|(id, _, _)| id.clone())
        .collect::<Vec<_>>();
    if !move_routing_profile_id_before(&mut ids, dragged_id, target_id) {
        return false;
    }
    let mut remaining = std::mem::take(&mut *controls_mut);
    let mut reordered = Vec::with_capacity(remaining.len());
    for id in ids {
        if let Some(index) = remaining
            .iter()
            .position(|(candidate_id, _, _)| *candidate_id == id)
        {
            reordered.push(remaining.swap_remove(index));
        }
    }
    *controls_mut = reordered;
    drop(controls_mut);
    rebuild_routing_candidate_rows(container, controls);
    true
}

// 为候选行安装文本拖动源和“放置到该行之前”的 GTK 控制器。
fn attach_routing_candidate_drag(
    row: &gtk::Box,
    container: &gtk::Box,
    controls: &RoutingCandidateControls,
    profile_id: &ProviderProfileId,
) {
    let bytes = glib::Bytes::from(profile_id.as_str().as_bytes());
    let provider = gtk::gdk::ContentProvider::for_bytes("text/plain", &bytes);
    let drag_source = gtk::DragSource::new();
    drag_source.set_actions(gtk::gdk::DragAction::MOVE);
    drag_source.set_content(Some(&provider));
    row.add_controller(drag_source);

    let drop_target = gtk::DropTarget::new(String::static_type(), gtk::gdk::DragAction::MOVE);
    let drop_container = container.clone();
    let drop_controls = Rc::clone(controls);
    let target_id = profile_id.clone();
    drop_target.connect_drop(move |_, value, _, _| {
        let Ok(dragged_id) = value.get::<String>() else {
            return false;
        };
        let Ok(dragged_id) = ProviderProfileId::parse(dragged_id) else {
            return false;
        };
        move_routing_candidate_row_before(&drop_container, &drop_controls, &dragged_id, &target_id)
    });
    row.add_controller(drop_target);
}

// 将已保存配置恢复到编辑器，缺失的提供商候选会被安全地跳过。
fn load_routing_profile_editor(
    widgets: RoutingEditorWidgets<'_>,
    container: &gtk::Box,
    controls: &RoutingCandidateControls,
    profile: &RoutingProfile,
) {
    widgets
        .mode
        .set_selected(routing_mode_selection(profile.mode));
    widgets
        .preference
        .set_selected(routing_preference_selection(profile.constraints.preference));
    widgets
        .allow_fallback
        .set_active(profile.constraints.explicit_fallback_allowed);
    widgets
        .local_only
        .set_active(profile.constraints.local_only);
    widgets
        .allow_remote
        .set_active(profile.constraints.allow_remote);
    widgets
        .privacy_sensitive
        .set_active(profile.constraints.privacy_sensitive);
    widgets
        .require_streaming
        .set_active(profile.constraints.require_streaming);
    widgets
        .require_document
        .set_active(profile.constraints.require_document);
    widgets
        .provider_allowlist
        .set_text(&routing_identifier_list_text(
            &profile.constraints.provider_allowlist,
        ));
    widgets
        .provider_denylist
        .set_text(&routing_identifier_list_text(
            &profile.constraints.provider_denylist,
        ));
    widgets
        .model_allowlist
        .set_text(&routing_identifier_list_text(
            &profile.constraints.model_allowlist,
        ));
    widgets
        .model_denylist
        .set_text(&routing_identifier_list_text(
            &profile.constraints.model_denylist,
        ));
    widgets.minimum_quality_tier.set_text(
        &profile
            .constraints
            .minimum_quality_tier
            .map_or_else(String::new, |value| value.to_string()),
    );
    widgets.max_request_bytes.set_text(
        &profile
            .constraints
            .max_request_bytes
            .map_or_else(String::new, |value| value.to_string()),
    );
    {
        let controls_ref = controls.borrow();
        for (_, _, check) in controls_ref.iter() {
            check.set_active(false);
        }
    }
    let candidate_ids = profile
        .candidates
        .iter()
        .filter_map(|candidate| ProviderProfileId::parse(&candidate.provider_id).ok())
        .collect::<Vec<_>>();
    {
        let controls_ref = controls.borrow();
        for (profile_id, _, check) in controls_ref.iter() {
            check.set_active(candidate_ids.iter().any(|id| id == profile_id));
        }
    }
    for pair in candidate_ids.windows(2) {
        move_routing_candidate_row_before(container, controls, &pair[1], &pair[0]);
    }
    if profile.mode == RoutingMode::Manual {
        enforce_manual_candidate_selection(controls);
    }
}

// 从用户明确保存的提供商配置生成不含端点和秘密的路由候选。
fn routing_candidate_for_profile(
    profile: &ProviderProfile,
) -> Result<RoutingCandidate, TranslationError> {
    let model = profile.selected_model().ok_or_else(|| {
        TranslationError::new(
            ErrorKind::ModelUnavailable,
            "Select a model for every saved provider before creating a routing profile.",
        )
    })?;
    let endpoint = profile.base_endpoint();
    let local = endpoint.starts_with("http://127.0.0.1")
        || endpoint.starts_with("http://localhost")
        || endpoint.starts_with("http://[::1]");
    let mut candidate = RoutingCandidate::new(profile.id().as_str(), model, local, 64 * 1024)
        .map_err(|error| {
            TranslationError::new(ErrorKind::InvalidConfiguration, error.to_string())
        })?;
    candidate.supports_document = true;
    candidate.quality_tier = if local { 2 } else { 1 };
    Ok(candidate)
}

// 创建一个可复用的 Linux 自动路由配置，候选只来自已保存的提供商配置。
// 根据用户选择创建路由配置，并把回退权限保持为显式开关。
fn default_routing_profile(
    state: &AppState,
    profile_id: &str,
    mode: RoutingMode,
    constraints: RoutingConstraints,
    selected_candidate_ids: &[ProviderProfileId],
) -> Result<RoutingProfile, TranslationError> {
    let candidate_ids = ordered_routing_profile_ids(state.saved_profiles(), selected_candidate_ids);
    let candidates = candidate_ids
        .iter()
        .filter_map(|id| {
            state
                .saved_profiles()
                .iter()
                .find(|profile| profile.id() == id)
        })
        .map(routing_candidate_for_profile)
        .collect::<Result<Vec<_>, _>>()?;
    if candidates.is_empty() {
        return Err(TranslationError::new(
            ErrorKind::InvalidConfiguration,
            "Save at least one provider profile before creating a routing profile.",
        ));
    }
    RoutingProfile::new(profile_id, mode, candidates, constraints)
        .map_err(|error| TranslationError::new(ErrorKind::InvalidConfiguration, error.to_string()))
}

// 展示持久化的路由规划配置，并把保存、删除操作交给核心工作线程。
#[allow(clippy::too_many_lines)]
fn show_routing_profiles_dialog(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &WorkerCommandHandle,
    profiles: Vec<RoutingProfileRecord>,
) {
    let locale = state.borrow().locale();
    let dialog = gtk::Window::builder()
        .application(&bindings.application)
        .transient_for(&bindings.window)
        .modal(true)
        .title(localization::text(
            locale,
            "dialog.routing_profiles",
            "Routing profiles",
        ))
        .default_width(720)
        .default_height(480)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let profile_id_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let profile_id_label = gtk::Label::with_mnemonic(&localized_mnemonic(
        locale,
        "label.routing_profile_id",
        "Routing profile ID",
    ));
    let profile_id = gtk::Entry::new();
    profile_id.set_text("linux-default");
    profile_id.set_max_length(
        i32::try_from(MAX_ROUTING_IDENTIFIER_BYTES)
            .expect("routing profile identifier limit fits GTK length type"),
    );
    profile_id.set_hexpand(true);
    profile_id.set_focusable(true);
    profile_id.set_tooltip_text(Some(&localization::text(
        locale,
        "error.routing_profile_id_invalid",
        "Use 1-128 ASCII letters, numbers, '.', '_' or '-' for the routing profile ID.",
    )));
    profile_id_label.set_mnemonic_widget(Some(&profile_id));
    profile_id_row.append(&profile_id_label);
    profile_id_row.append(&profile_id);
    root.append(&profile_id_row);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let mode_labels = [
        localized_routing_mode(locale, RoutingMode::Manual),
        localized_routing_mode(locale, RoutingMode::Ordered),
        localized_routing_mode(locale, RoutingMode::Automatic),
    ];
    let mode_label_refs = mode_labels.iter().map(String::as_str).collect::<Vec<_>>();
    let mode = gtk::DropDown::from_strings(&mode_label_refs);
    mode.set_selected(2);
    mode.set_focusable(true);
    mode.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.routing_profiles",
        "Create, inspect, and delete non-secret routing planner profiles",
    )));
    let allow_fallback = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        locale,
        "action.enable_fallback",
        "Allow approved fallback",
    ));
    allow_fallback.set_focusable(true);
    allow_fallback.set_active(false);
    allow_fallback.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.fallback",
        "Retry only retryable network failures with this saved provider; document jobs, cancellation, and credential failures never fall back",
    )));
    actions.append(&mode);
    actions.append(&allow_fallback);
    let preference_labels = [
        localization::text(locale, "routing.preference.none", "No preference"),
        localization::text(locale, "routing.preference.local", "Local first"),
        localization::text(locale, "routing.preference.quality", "Quality first"),
        localization::text(locale, "routing.preference.latency", "Lowest latency"),
        localization::text(locale, "routing.preference.cost", "Lowest cost"),
    ];
    let preference_label_refs = preference_labels
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let preference = gtk::DropDown::from_strings(&preference_label_refs);
    preference.set_selected(routing_preference_selection(RoutingPreference::Local));
    preference.set_focusable(true);
    preference.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.routing_preference",
        "Choose the first ranking preference used by Automatic routing",
    )));
    let preference_control = labeled_control(
        &localized_mnemonic(locale, "label.routing_preference", "Automatic preference"),
        preference.upcast_ref::<gtk::Widget>(),
    );
    let local_only = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        locale,
        "option.routing_local_only",
        "Local candidates only",
    ));
    local_only.set_focusable(true);
    local_only.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.routing_local_only",
        "Reject saved providers that are not local loopback endpoints",
    )));
    let allow_remote = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        locale,
        "option.routing_allow_remote",
        "Allow remote candidates",
    ));
    allow_remote.set_active(true);
    allow_remote.set_focusable(true);
    allow_remote.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.routing_allow_remote",
        "Permit this profile to send content to non-local providers",
    )));
    let privacy_sensitive = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        locale,
        "option.routing_privacy_sensitive",
        "Protect privacy-sensitive requests",
    ));
    privacy_sensitive.set_focusable(true);
    privacy_sensitive.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.routing_privacy_sensitive",
        "When a request is marked privacy-sensitive, reject remote candidates",
    )));
    let require_streaming = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        locale,
        "option.routing_require_streaming",
        "Require streamed output",
    ));
    require_streaming.set_focusable(true);
    require_streaming.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.routing_require_streaming",
        "Reject candidates that cannot return real streamed deltas",
    )));
    let require_document = gtk::CheckButton::with_mnemonic(&localized_mnemonic(
        locale,
        "option.routing_require_document",
        "Require document support",
    ));
    require_document.set_focusable(true);
    require_document.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.routing_require_document",
        "Keep only candidates that advertise document translation",
    )));
    let (provider_allowlist, provider_allowlist_control) = routing_text_entry(
        locale,
        "label.routing_provider_allowlist",
        "Provider allowlist",
        "tooltip.routing_identifier_list",
        "Comma-separated provider or model identifiers; leave blank for no list",
    );
    let (provider_denylist, provider_denylist_control) = routing_text_entry(
        locale,
        "label.routing_provider_denylist",
        "Provider denylist",
        "tooltip.routing_identifier_list",
        "Comma-separated provider or model identifiers; leave blank for no list",
    );
    let (model_allowlist, model_allowlist_control) = routing_text_entry(
        locale,
        "label.routing_model_allowlist",
        "Model allowlist",
        "tooltip.routing_identifier_list",
        "Comma-separated provider or model identifiers; leave blank for no list",
    );
    let (model_denylist, model_denylist_control) = routing_text_entry(
        locale,
        "label.routing_model_denylist",
        "Model denylist",
        "tooltip.routing_identifier_list",
        "Comma-separated provider or model identifiers; leave blank for no list",
    );
    let (minimum_quality_tier, minimum_quality_tier_control) = routing_text_entry(
        locale,
        "label.routing_minimum_quality",
        "Minimum quality tier",
        "tooltip.routing_minimum_quality",
        "Leave blank to accept every quality tier",
    );
    let (max_request_bytes, max_request_bytes_control) = routing_text_entry(
        locale,
        "label.routing_max_request_bytes",
        "Maximum request bytes",
        "tooltip.routing_max_request_bytes",
        "Leave blank for no profile-level request size limit; otherwise use a positive byte count",
    );
    let constraints = gtk::Box::new(gtk::Orientation::Vertical, 4);
    constraints.append(&preference_control);
    constraints.append(&local_only);
    constraints.append(&allow_remote);
    constraints.append(&privacy_sensitive);
    constraints.append(&require_streaming);
    constraints.append(&require_document);
    constraints.append(&provider_allowlist_control);
    constraints.append(&provider_denylist_control);
    constraints.append(&model_allowlist_control);
    constraints.append(&model_denylist_control);
    constraints.append(&minimum_quality_tier_control);
    constraints.append(&max_request_bytes_control);
    root.append(&constraints);
    let remote_guard = allow_remote.clone();
    local_only.connect_toggled(move |button| {
        if button.is_active() {
            remote_guard.set_active(false);
        }
    });
    let local_guard = local_only.clone();
    allow_remote.connect_toggled(move |button| {
        if button.is_active() {
            local_guard.set_active(false);
        }
    });
    let candidate_controls: RoutingCandidateControls = Rc::new(RefCell::new(Vec::new()));
    let candidates_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    for profile in state
        .borrow()
        .saved_profiles()
        .iter()
        .filter(|profile| profile.enabled() && profile.selected_model().is_some())
    {
        let model = profile.selected_model().unwrap_or_default();
        let check =
            gtk::CheckButton::with_label(&format!("{} · {}", profile.display_name(), model));
        check.set_active(true);
        check.set_focusable(true);
        check.set_hexpand(true);
        check.set_halign(gtk::Align::Fill);
        check.set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.routing_profiles",
            "Create, inspect, and delete non-secret routing planner profiles",
        )));
        let up = gtk::Button::from_icon_name("go-up-symbolic");
        up.set_focusable(true);
        up.set_has_frame(false);
        let up_label = localization::text(locale, "action.move_candidate_up", "Move candidate up");
        up.set_tooltip_text(Some(&up_label));
        up.update_property(&[gtk::accessible::Property::Label(&up_label)]);
        let down = gtk::Button::from_icon_name("go-down-symbolic");
        down.set_focusable(true);
        down.set_has_frame(false);
        let down_label =
            localization::text(locale, "action.move_candidate_down", "Move candidate down");
        down.set_tooltip_text(Some(&down_label));
        down.update_property(&[gtk::accessible::Property::Label(&down_label)]);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        row.append(&check);
        row.append(&up);
        row.append(&down);
        let profile_id = profile.id().clone();
        attach_routing_candidate_drag(&row, &candidates_box, &candidate_controls, &profile_id);
        let up_controls = Rc::clone(&candidate_controls);
        let up_container = candidates_box.clone();
        let up_id = profile_id.clone();
        up.connect_clicked(move |_| {
            move_routing_candidate_row(&up_container, &up_controls, &up_id, -1);
        });
        let down_controls = Rc::clone(&candidate_controls);
        let down_container = candidates_box.clone();
        let down_id = profile_id.clone();
        down.connect_clicked(move |_| {
            move_routing_candidate_row(&down_container, &down_controls, &down_id, 1);
        });
        candidate_controls
            .borrow_mut()
            .push((profile_id, row, check.clone()));
    }
    rebuild_routing_candidate_rows(&candidates_box, &candidate_controls);
    root.append(&candidates_box);
    let manual_candidate_controls = Rc::clone(&candidate_controls);
    mode.connect_selected_notify(move |drop_down| {
        if routing_mode_for_selection(drop_down.selected()) == RoutingMode::Manual {
            enforce_manual_candidate_selection(&manual_candidate_controls);
        }
    });
    let editing_profile: Rc<RefCell<Option<RoutingProfile>>> = Rc::new(RefCell::new(None));
    let create = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.create_routing_profile",
        "Create local-first profile",
    ));
    create.set_focusable(true);
    let import = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.import_routing_profile",
        "Import profile",
    ));
    import.set_focusable(true);
    import.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.routing_profiles",
        "Create, inspect, and delete non-secret routing planner profiles",
    )));
    let close = gtk::Button::with_mnemonic(&localized_mnemonic(locale, "action.close", "Close"));
    close.set_focusable(true);
    actions.append(&import);
    actions.append(&create);
    actions.append(&close);
    root.append(&actions);
    let existing_profile_ids = profiles
        .iter()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_vexpand(true);
    if profiles.is_empty() {
        let empty = gtk::Label::new(Some(&localization::text(
            locale,
            "status.routing_profile_empty",
            "No routing profiles are saved.",
        )));
        empty.set_xalign(0.0);
        empty.add_css_class("dim-label");
        list.append(&empty);
    } else {
        for record in profiles {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.set_margin_top(8);
            row.set_margin_bottom(8);
            let metadata = localized_template(
                locale,
                "status.routing_profile_row",
                "{id} · {mode} · {candidates} candidates",
                &[
                    ("{id}", record.id.as_str()),
                    (
                        "{mode}",
                        localized_routing_mode(locale, record.profile.mode).as_str(),
                    ),
                    ("{candidates}", &record.profile.candidates.len().to_string()),
                ],
            );
            let label = gtk::Label::new(Some(&metadata));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.add_css_class("dim-label");
            let edit = gtk::Button::with_mnemonic(&localized_mnemonic(
                locale,
                "action.edit_routing_profile",
                "Edit",
            ));
            edit.set_focusable(true);
            let edit_mode = mode.clone();
            let edit_preference = preference.clone();
            let edit_allow_fallback = allow_fallback.clone();
            let edit_local_only = local_only.clone();
            let edit_allow_remote = allow_remote.clone();
            let edit_privacy_sensitive = privacy_sensitive.clone();
            let edit_require_streaming = require_streaming.clone();
            let edit_require_document = require_document.clone();
            let edit_provider_allowlist = provider_allowlist.clone();
            let edit_provider_denylist = provider_denylist.clone();
            let edit_model_allowlist = model_allowlist.clone();
            let edit_model_denylist = model_denylist.clone();
            let edit_minimum_quality_tier = minimum_quality_tier.clone();
            let edit_max_request_bytes = max_request_bytes.clone();
            let edit_container = candidates_box.clone();
            let edit_controls = Rc::clone(&candidate_controls);
            let edit_save = create.clone();
            let edit_profile_id = profile_id.clone();
            let edit_profile = record.profile.clone();
            let edit_selection = Rc::clone(&editing_profile);
            edit.connect_clicked(move |_| {
                load_routing_profile_editor(
                    RoutingEditorWidgets {
                        mode: &edit_mode,
                        preference: &edit_preference,
                        allow_fallback: &edit_allow_fallback,
                        local_only: &edit_local_only,
                        allow_remote: &edit_allow_remote,
                        privacy_sensitive: &edit_privacy_sensitive,
                        require_streaming: &edit_require_streaming,
                        require_document: &edit_require_document,
                        provider_allowlist: &edit_provider_allowlist,
                        provider_denylist: &edit_provider_denylist,
                        model_allowlist: &edit_model_allowlist,
                        model_denylist: &edit_model_denylist,
                        minimum_quality_tier: &edit_minimum_quality_tier,
                        max_request_bytes: &edit_max_request_bytes,
                    },
                    &edit_container,
                    &edit_controls,
                    &edit_profile,
                );
                edit_profile_id.set_text(&edit_profile.id);
                edit_profile_id.set_editable(false);
                edit_selection.replace(Some(edit_profile.clone()));
                edit_save.set_label(&localized_mnemonic(
                    locale,
                    "action.save_routing_profile",
                    "Save routing profile",
                ));
            });
            let use_button = gtk::Button::with_mnemonic(&localized_mnemonic(
                locale,
                "action.use_routing_profile",
                "Use",
            ));
            use_button.set_focusable(true);
            let use_bindings = bindings.clone();
            let use_dialog = dialog.clone();
            let use_profile_id = record.id.clone();
            use_button.connect_clicked(move |_| {
                use_bindings
                    .selected_routing_profile_id
                    .replace(Some(use_profile_id.clone()));
                use_dialog.close();
            });
            let export = gtk::Button::with_mnemonic(&localized_mnemonic(
                locale,
                "action.export_routing_profile",
                "Export",
            ));
            export.set_focusable(true);
            let export_worker = worker.clone();
            let export_bindings = bindings.clone();
            let export_state = Rc::clone(state);
            let export_profile_id = record.id.clone();
            export.connect_clicked(move |_| {
                if let Err(error) = export_worker.export_routing_profile(export_profile_id.clone())
                {
                    export_state
                        .borrow_mut()
                        .record_client_error(error.to_string());
                    refresh_ui(&export_bindings, &export_state.borrow());
                }
            });
            let delete = gtk::Button::with_mnemonic(&localized_mnemonic(
                locale,
                "action.delete_routing_profile",
                "Delete",
            ));
            delete.set_focusable(true);
            delete.add_css_class("destructive-action");
            let delete_worker = worker.clone();
            let delete_dialog = dialog.clone();
            let delete_bindings = bindings.clone();
            let delete_state = Rc::clone(state);
            let profile_id = record.id;
            delete.connect_clicked(move |_| {
                if let Err(error) = delete_worker.delete_routing_profile(profile_id.clone()) {
                    delete_state
                        .borrow_mut()
                        .record_client_error(error.to_string());
                    refresh_ui(&delete_bindings, &delete_state.borrow());
                } else {
                    delete_dialog.close();
                }
            });
            row.append(&label);
            row.append(&edit);
            row.append(&use_button);
            row.append(&export);
            row.append(&delete);
            list.append(&row);
        }
    }
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list)
        .build();
    root.append(&scroller);
    dialog.set_child(Some(&root));
    let create_bindings = bindings.clone();
    let create_state = Rc::clone(state);
    let create_worker = worker.clone();
    let create_dialog = dialog.clone();
    let create_candidate_controls = Rc::clone(&candidate_controls);
    let create_editing_profile = Rc::clone(&editing_profile);
    let create_profile_id = profile_id.clone();
    let create_existing_profile_ids = existing_profile_ids;
    create.connect_clicked(move |_| {
        let profile_id = create_profile_id.text().trim().to_owned();
        if !valid_routing_profile_id(&profile_id) {
            let error_message = localization::text(
                create_state.borrow().locale(),
                "error.routing_profile_id_invalid",
                "Use 1-128 ASCII letters, numbers, '.', '_' or '-' for the routing profile ID.",
            );
            create_state.borrow_mut().record_client_error(error_message);
            refresh_ui(&create_bindings, &create_state.borrow());
            return;
        }
        let editing_profile = create_editing_profile.borrow();
        if routing_profile_id_conflicts(
            &create_existing_profile_ids,
            editing_profile.as_ref().map(|profile| profile.id.as_str()),
            &profile_id,
        ) {
            let error_message = localization::text(
                create_state.borrow().locale(),
                "error.routing_profile_id_exists",
                "A routing profile with this ID already exists. Edit it or choose another ID.",
            );
            create_state.borrow_mut().record_client_error(error_message);
            refresh_ui(&create_bindings, &create_state.borrow());
            return;
        }
        drop(editing_profile);
        let selected_candidate_ids = create_candidate_controls
            .borrow()
            .iter()
            .filter(|(_, _, check)| check.is_active())
            .map(|(id, _, _)| id.clone())
            .collect::<Vec<_>>();
        let selected_candidate_ids = normalized_candidate_ids_for_mode(
            routing_mode_for_selection(mode.selected()),
            selected_candidate_ids,
        );
        let editing_profile = create_editing_profile.borrow();
        let Ok(constraints) = routing_constraints_from_text_controls(
            editing_profile.as_ref().map(|profile| &profile.constraints),
            &RoutingConstraints {
                preference: routing_preference_for_selection(preference.selected()),
                local_only: local_only.is_active(),
                allow_remote: allow_remote.is_active(),
                privacy_sensitive: privacy_sensitive.is_active(),
                require_streaming: require_streaming.is_active(),
                require_document: require_document.is_active(),
                explicit_fallback_allowed: allow_fallback.is_active(),
                ..RoutingConstraints::default()
            },
            RoutingConstraintTextWidgets {
                provider_allowlist: &provider_allowlist,
                provider_denylist: &provider_denylist,
                model_allowlist: &model_allowlist,
                model_denylist: &model_denylist,
                minimum_quality_tier: &minimum_quality_tier,
                max_request_bytes: &max_request_bytes,
            },
        ) else {
            create_state
                .borrow_mut()
                .record_client_error(localization::text(
                    create_state.borrow().locale(),
                    "error.routing_constraints_invalid",
                    "Use comma-separated ASCII identifiers and positive numeric limits.",
                ));
            refresh_ui(&create_bindings, &create_state.borrow());
            return;
        };
        match default_routing_profile(
            &create_state.borrow(),
            &profile_id,
            routing_mode_for_selection(mode.selected()),
            constraints,
            &selected_candidate_ids,
        ) {
            Ok(mut profile) => {
                if let Some(existing_profile) = editing_profile.clone() {
                    profile.id = existing_profile.id;
                }
                if let Err(error) = create_worker.save_routing_profile(profile) {
                    create_state
                        .borrow_mut()
                        .record_client_error(error.to_string());
                    refresh_ui(&create_bindings, &create_state.borrow());
                } else {
                    create_dialog.close();
                }
            }
            Err(error) => {
                create_state
                    .borrow_mut()
                    .record_client_error(localization::text(
                        create_state.borrow().locale(),
                        "error.routing_profile_no_candidates",
                        &error.to_string(),
                    ));
                refresh_ui(&create_bindings, &create_state.borrow());
            }
        }
    });
    let close_dialog = dialog.clone();
    close.connect_clicked(move |_| close_dialog.close());
    let import_bindings = bindings.clone();
    let import_state = Rc::clone(state);
    let import_worker = worker.clone();
    let import_dialog = dialog.clone();
    import.connect_clicked(move |_| {
        begin_routing_profile_import(
            &import_bindings,
            &import_state,
            &import_worker,
            &import_dialog,
        );
    });
    dialog.present();
}

// 通过受限的 GIO 异步读取导入文件，只把 UTF-8 JSON 交给核心校验。
fn begin_routing_profile_import(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &WorkerCommandHandle,
    dialog: &gtk::Window,
) {
    let locale = state.borrow().locale();
    let filter_name = localization::text(locale, "file.filter.json", "JSON files");
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&filter_name));
    filter.add_mime_type("application/json");
    filter.add_suffix("json");
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog_title = localization::text(
        locale,
        "dialog.open_routing_profile",
        "Import routing profile",
    );
    let dialog_accept = localization::text(locale, "dialog.open", "Open");
    let file_dialog = gtk::FileDialog::builder()
        .title(&dialog_title)
        .accept_label(&dialog_accept)
        .modal(true)
        .build();
    file_dialog.set_filters(Some(&filters));
    let import_bindings = bindings.clone();
    let import_state = Rc::clone(state);
    let import_worker = worker.clone();
    let import_dialog = dialog.clone();
    file_dialog.open(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                let bytes_read = Rc::new(Cell::new(0_usize));
                let too_large = Rc::new(Cell::new(false));
                let read_bytes = Rc::clone(&bytes_read);
                let read_too_large = Rc::clone(&too_large);
                let callback_bindings = import_bindings.clone();
                let callback_state = Rc::clone(&import_state);
                let callback_worker = import_worker.clone();
                let callback_dialog = import_dialog.clone();
                file.load_partial_contents_async(
                    None::<&gtk::gio::Cancellable>,
                    move |chunk| {
                        let next = read_bytes.get().saturating_add(chunk.len());
                        if next > MAX_ROUTING_PROFILE_JSON_BYTES {
                            read_too_large.set(true);
                            false
                        } else {
                            read_bytes.set(next);
                            true
                        }
                    },
                    move |read_result| {
                        if too_large.get() {
                            show_file_import_error(
                                &callback_bindings,
                                &localization::text(
                                    callback_state.borrow().locale(),
                                    "error.routing_profile_import",
                                    "The routing profile JSON could not be imported.",
                                ),
                            );
                            return;
                        }
                        match read_result {
                            Ok((contents, _)) => {
                                if let Err(error) = callback_worker
                                    .import_routing_profile(contents.as_ref().to_vec())
                                {
                                    callback_state
                                        .borrow_mut()
                                        .record_client_error(error.to_string());
                                    refresh_ui(&callback_bindings, &callback_state.borrow());
                                } else {
                                    callback_dialog.close();
                                }
                            }
                            Err(_) => show_file_import_error(
                                &callback_bindings,
                                &localization::text(
                                    callback_state.borrow().locale(),
                                    "error.routing_profile_import",
                                    "The routing profile JSON could not be imported.",
                                ),
                            ),
                        }
                    },
                );
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(_) => show_file_import_error(
                &import_bindings,
                &localization::text(
                    import_state.borrow().locale(),
                    "error.routing_profile_import",
                    "The routing profile JSON could not be imported.",
                ),
            ),
        },
    );
}

// 通过 GTK 原生保存对话框写出核心编码的非秘密路由配置 JSON。
fn begin_routing_profile_export(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    profile_id: &str,
    contents: Vec<u8>,
) {
    let locale = state.borrow().locale();
    let dialog_title = localization::text(
        locale,
        "dialog.export_routing_profile",
        "Export routing profile",
    );
    let dialog_accept = localization::text(locale, "dialog.save", "Save");
    let file_dialog = gtk::FileDialog::builder()
        .title(&dialog_title)
        .accept_label(&dialog_accept)
        .modal(true)
        .build();
    file_dialog.set_initial_name(Some(&format!("linguamesh-routing-{profile_id}.json")));
    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    file_dialog.save(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                let callback_bindings = export_bindings.clone();
                let callback_state = Rc::clone(&export_state);
                write_new_file_async(&file, contents, move |write_succeeded| {
                    if !write_succeeded {
                        show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.routing_profile_export",
                                "The routing profile JSON could not be saved.",
                            ),
                        );
                    }
                });
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(_) => show_file_export_error(
                &export_bindings,
                &localization::text(
                    export_state.borrow().locale(),
                    "error.routing_profile_export",
                    "The routing profile JSON could not be saved.",
                ),
            ),
        },
    );
}

// 展示持久化文档任务并允许用户选择或控制队列中的任务。
#[allow(clippy::too_many_lines)]
fn show_document_jobs_dialog(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &WorkerCommandHandle,
    jobs: Vec<DocumentJobSnapshot>,
) {
    let locale = state.borrow().locale();
    let dialog = gtk::Window::builder()
        .application(&bindings.application)
        .transient_for(&bindings.window)
        .modal(true)
        .title(localization::text(
            locale,
            "dialog.document_jobs",
            "Document jobs",
        ))
        .default_width(760)
        .default_height(520)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let close = gtk::Button::with_mnemonic(&localized_mnemonic(locale, "action.close", "Close"));
    close.set_focusable(true);
    root.append(&close);
    let job_count = u64::try_from(jobs.len()).unwrap_or(u64::MAX);
    let job_count_text = localization::text_plural(
        locale,
        "document.file_count",
        "{count} file",
        "{count} files",
        job_count,
    )
    .replace("{count}", &jobs.len().to_string());
    let job_count_label = gtk::Label::new(Some(&job_count_text));
    job_count_label.set_xalign(0.0);
    job_count_label.add_css_class("dim-label");
    root.append(&job_count_label);
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_vexpand(true);
    if jobs.is_empty() {
        let empty = gtk::Label::new(Some(&localization::text(
            locale,
            "status.document_jobs_empty",
            "No document jobs yet",
        )));
        empty.set_xalign(0.0);
        empty.add_css_class("dim-label");
        list.append(&empty);
    } else {
        for snapshot in jobs {
            let row = gtk::Box::new(gtk::Orientation::Vertical, 6);
            row.set_margin_top(8);
            row.set_margin_bottom(8);
            let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let (completed, total) = document_progress(&snapshot);
            let format_label = document_format_label(snapshot.job.format);
            let state_label = localized_document_job_state(locale, snapshot.state);
            let completed_label = completed.to_string();
            let total_label = total.to_string();
            let metadata_text = localized_template(
                locale,
                "status.document_job_row",
                "{source} · {format} · {state} · {completed}/{total}",
                &[
                    ("{source}", snapshot.job.source_name.as_str()),
                    ("{format}", format_label),
                    ("{state}", state_label.as_str()),
                    ("{completed}", completed_label.as_str()),
                    ("{total}", total_label.as_str()),
                ],
            );
            let metadata = gtk::Label::new(Some(&metadata_text));
            metadata.set_xalign(0.0);
            metadata.set_hexpand(true);
            metadata.add_css_class("dim-label");
            let select = gtk::Button::with_mnemonic(&localized_mnemonic(
                locale,
                "action.select_document_job",
                "Select document job",
            ));
            select.set_focusable(true);
            let selected = snapshot.clone();
            let select_dialog = dialog.clone();
            let select_bindings = bindings.clone();
            let select_state = Rc::clone(state);
            select.connect_clicked(move |_| {
                select_document_job(&select_bindings, &select_state, &selected);
                select_dialog.close();
            });
            header.append(&metadata);
            header.append(&select);
            row.append(&header);
            let id_text = localized_template(
                locale,
                "status.document_job_id",
                "Job: {id}",
                &[("{id}", snapshot.job_id.as_str())],
            );
            let id = gtk::Label::new(Some(&id_text));
            id.set_xalign(0.0);
            id.add_css_class("dim-label");
            row.append(&id);
            let queue_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let action_spec = match snapshot.state {
                DocumentJobState::Pending | DocumentJobState::Running => {
                    Some(("action.pause_document", "Pause document", 0_u8))
                }
                DocumentJobState::Paused => {
                    Some(("action.resume_document", "Resume document", 1_u8))
                }
                DocumentJobState::Cancelled | DocumentJobState::Failed => {
                    Some(("action.retry_document", "Retry document", 2_u8))
                }
                DocumentJobState::Completed => None,
            };
            if let Some((action_key, action_fallback, action_kind)) = action_spec {
                let action = gtk::Button::with_mnemonic(&localized_mnemonic(
                    locale,
                    action_key,
                    action_fallback,
                ));
                action.set_focusable(true);
                let action_bindings = bindings.clone();
                let action_state = Rc::clone(state);
                let action_worker = worker.clone();
                let action_selected = snapshot.clone();
                let action_dialog = dialog.clone();
                action.connect_clicked(move |_| {
                    select_document_job(&action_bindings, &action_state, &action_selected);
                    let job_id = action_selected.job_id.clone();
                    let result = match action_kind {
                        0 => action_worker.pause_document_job(job_id),
                        1 => action_worker.resume_document_job(job_id),
                        2 => action_worker.retry_document_job(job_id),
                        _ => Ok(()),
                    };
                    if let Err(error) = result {
                        action_state
                            .borrow_mut()
                            .record_client_error(error.to_string());
                        refresh_ui(&action_bindings, &action_state.borrow());
                    } else {
                        action_dialog.close();
                    }
                });
                queue_actions.append(&action);
            }
            let report = gtk::Button::with_mnemonic(&localized_mnemonic(
                locale,
                "action.export_report",
                "Export translation report",
            ));
            report.set_focusable(true);
            report.set_tooltip_text(Some(&localization::text(
                locale,
                "tooltip.export_report",
                "Save a redacted TSV report for this document job",
            )));
            let report_bindings = bindings.clone();
            let report_state = Rc::clone(state);
            let report_snapshot = snapshot.clone();
            report.connect_clicked(move |_| {
                begin_document_report_export(&report_bindings, &report_state, &report_snapshot);
            });
            queue_actions.append(&report);
            row.append(&queue_actions);
            list.append(&row);
        }
    }
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list)
        .build();
    root.append(&scroller);
    dialog.set_child(Some(&root));
    let close_dialog = dialog.clone();
    close.connect_clicked(move |_| close_dialog.close());
    dialog.present();
}

// 以可读的确定性 TSV 展示本地历史，并把删除操作重新交给核心工作线程。
#[allow(clippy::too_many_lines)]
fn show_history_dialog(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &WorkerCommandHandle,
    entries: Vec<TranslationHistoryEntry>,
) {
    bindings.history_export_notice.set(false);
    let locale = state.borrow().locale();
    let dialog = gtk::Window::builder()
        .application(&bindings.application)
        .transient_for(&bindings.window)
        .modal(true)
        .title(localization::text(
            locale,
            "dialog.history",
            "Translation history",
        ))
        .default_width(760)
        .default_height(520)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let export = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.export_history",
        "Export history",
    ));
    export.set_focusable(true);
    let close = gtk::Button::with_mnemonic(&localized_mnemonic(locale, "action.close", "Close"));
    close.set_focusable(true);
    actions.append(&export);
    actions.append(&close);
    root.append(&actions);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_vexpand(true);
    if entries.is_empty() {
        let empty = gtk::Label::new(Some(&localization::text(
            locale,
            "status.history_empty",
            "No local translation history is stored.",
        )));
        empty.set_xalign(0.0);
        empty.add_css_class("dim-label");
        list.append(&empty);
    } else {
        for entry in &entries {
            let row = gtk::Box::new(gtk::Orientation::Vertical, 6);
            row.set_margin_top(8);
            row.set_margin_bottom(8);
            let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let auto_locale = localization::text(locale, "option.source.auto", "Auto");
            let source_locale = entry
                .source_locale
                .as_deref()
                .unwrap_or(auto_locale.as_str());
            let created_at = entry.created_at.to_string();
            let metadata_text = localized_template(
                locale,
                "status.translation_entry_metadata",
                "{source} → {target} · {model} · {created_at}",
                &[
                    ("{source}", source_locale),
                    ("{target}", entry.target_locale.as_str()),
                    ("{model}", entry.model_id.as_str()),
                    ("{created_at}", created_at.as_str()),
                ],
            );
            let metadata = gtk::Label::new(Some(&metadata_text));
            metadata.set_xalign(0.0);
            metadata.set_hexpand(true);
            metadata.add_css_class("dim-label");
            let delete = gtk::Button::with_mnemonic(&localized_mnemonic(
                locale,
                "action.delete_history_entry",
                "Delete",
            ));
            delete.set_focusable(true);
            delete.add_css_class("destructive-action");
            let delete_worker = worker.clone();
            let delete_dialog = dialog.clone();
            let delete_bindings = bindings.clone();
            let delete_state = Rc::clone(state);
            let operation_id = entry.operation_id.clone();
            delete.connect_clicked(move |_| {
                delete_dialog.close();
                if let Err(error) = delete_worker.delete_translation_history(operation_id.clone()) {
                    delete_state
                        .borrow_mut()
                        .record_client_error(error.to_string());
                    refresh_ui(&delete_bindings, &delete_state.borrow());
                }
            });
            header.append(&metadata);
            header.append(&delete);
            row.append(&header);
            let source_prefix = localization::text(locale, "field.source_text", "Source text");
            let source = gtk::Label::new(Some(&format!("{source_prefix}: {}", entry.source_text)));
            source.set_xalign(0.0);
            source.set_wrap(true);
            source.set_selectable(true);
            row.append(&source);
            let translation_prefix = localization::text(locale, "field.translation", "Translation");
            let translated = gtk::Label::new(Some(&format!(
                "{translation_prefix}: {}",
                entry.translated_text
            )));
            translated.set_xalign(0.0);
            translated.set_wrap(true);
            translated.set_selectable(true);
            row.append(&translated);
            list.append(&row);
        }
    }
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list)
        .build();
    root.append(&scroller);
    dialog.set_child(Some(&root));

    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    let export_entries = entries;
    export.connect_clicked(move |_| {
        begin_history_export(&export_bindings, &export_state, &export_entries);
    });
    let close_dialog = dialog.clone();
    close.connect_clicked(move |_| close_dialog.close());
    dialog.present();
}

// 将历史字段中的控制字符转义，避免导出内容伪造 TSV 行或列。
fn history_tsv_field(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

// 通过 GTK 原生保存对话框异步导出本地翻译历史。
fn begin_history_export(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    entries: &[TranslationHistoryEntry],
) {
    let locale = state.borrow().locale();
    let mut contents = String::from(
        "operation_id\tcreated_at\tsource_locale\ttarget_locale\tmodel_id\tsource_text\ttranslated_text\n",
    );
    for entry in entries {
        contents.push_str(&history_tsv_field(&entry.operation_id));
        contents.push('\t');
        contents.push_str(&entry.created_at.to_string());
        contents.push('\t');
        contents.push_str(&history_tsv_field(
            entry.source_locale.as_deref().unwrap_or("auto"),
        ));
        contents.push('\t');
        contents.push_str(&history_tsv_field(&entry.target_locale));
        contents.push('\t');
        contents.push_str(&history_tsv_field(&entry.model_id));
        contents.push('\t');
        contents.push_str(&history_tsv_field(&entry.source_text));
        contents.push('\t');
        contents.push_str(&history_tsv_field(&entry.translated_text));
        contents.push('\n');
    }
    let contents = contents.into_bytes();
    let dialog = gtk::FileDialog::builder()
        .title(localization::text(
            locale,
            "dialog.export_history",
            "Export translation history",
        ))
        .accept_label(localization::text(locale, "dialog.save", "Save"))
        .modal(true)
        .build();
    dialog.set_initial_name(Some("linguamesh-history.tsv"));
    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    dialog.save(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                let callback_bindings = export_bindings.clone();
                let callback_state = Rc::clone(&export_state);
                write_new_file_async(&file, contents, move |write_succeeded| {
                    if write_succeeded {
                        callback_bindings.history_export_notice.set(true);
                        refresh_ui(&callback_bindings, &callback_state.borrow());
                    } else {
                        show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.history_export",
                                "The translation history could not be saved.",
                            ),
                        );
                    }
                });
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(_) => show_file_export_error(
                &export_bindings,
                &localization::text(
                    export_state.borrow().locale(),
                    "error.history_export",
                    "The translation history could not be saved.",
                ),
            ),
        },
    );
}

// 以可读的确定性 TSV 展示本地翻译记忆，并把删除操作重新交给核心工作线程。
#[allow(clippy::too_many_lines)]
fn show_memory_dialog(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &WorkerCommandHandle,
    entries: Vec<TranslationMemoryEntry>,
) {
    bindings.memory_export_notice.set(false);
    let locale = state.borrow().locale();
    let dialog = gtk::Window::builder()
        .application(&bindings.application)
        .transient_for(&bindings.window)
        .modal(true)
        .title(localization::text(
            locale,
            "dialog.memory",
            "Translation memory",
        ))
        .default_width(760)
        .default_height(520)
        .build();
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let export = gtk::Button::with_mnemonic(&localized_mnemonic(
        locale,
        "action.export_memory",
        "Export translation memory",
    ));
    export.set_focusable(true);
    let close = gtk::Button::with_mnemonic(&localized_mnemonic(locale, "action.close", "Close"));
    close.set_focusable(true);
    actions.append(&export);
    actions.append(&close);
    root.append(&actions);
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_vexpand(true);
    if entries.is_empty() {
        let empty = gtk::Label::new(Some(&localization::text(
            locale,
            "status.memory_empty",
            "No local translation memory is stored.",
        )));
        empty.set_xalign(0.0);
        empty.add_css_class("dim-label");
        list.append(&empty);
    } else {
        for entry in &entries {
            let row = gtk::Box::new(gtk::Orientation::Vertical, 6);
            row.set_margin_top(8);
            row.set_margin_bottom(8);
            let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let auto_locale = localization::text(locale, "option.source.auto", "Auto");
            let source_locale = entry
                .source_locale
                .as_deref()
                .unwrap_or(auto_locale.as_str());
            let created_at = entry.created_at.to_string();
            let metadata_text = localized_template(
                locale,
                "status.translation_entry_metadata",
                "{source} → {target} · {model} · {created_at}",
                &[
                    ("{source}", source_locale),
                    ("{target}", entry.target_locale.as_str()),
                    ("{model}", entry.model_id.as_str()),
                    ("{created_at}", created_at.as_str()),
                ],
            );
            let metadata = gtk::Label::new(Some(&metadata_text));
            metadata.set_xalign(0.0);
            metadata.set_hexpand(true);
            metadata.add_css_class("dim-label");
            let delete = gtk::Button::with_mnemonic(&localized_mnemonic(
                locale,
                "action.delete_memory_entry",
                "Delete",
            ));
            delete.set_focusable(true);
            delete.add_css_class("destructive-action");
            let delete_worker = worker.clone();
            let delete_dialog = dialog.clone();
            let delete_bindings = bindings.clone();
            let delete_state = Rc::clone(state);
            let cache_key = entry.cache_key.clone();
            delete.connect_clicked(move |_| {
                delete_dialog.close();
                if let Err(error) = delete_worker.delete_translation_memory(cache_key.clone()) {
                    delete_state
                        .borrow_mut()
                        .record_client_error(error.to_string());
                    refresh_ui(&delete_bindings, &delete_state.borrow());
                }
            });
            header.append(&metadata);
            header.append(&delete);
            row.append(&header);
            let source_prefix = localization::text(locale, "field.source_text", "Source text");
            let source = gtk::Label::new(Some(&format!("{source_prefix}: {}", entry.source_text)));
            source.set_xalign(0.0);
            source.set_wrap(true);
            source.set_selectable(true);
            row.append(&source);
            let translation_prefix = localization::text(locale, "field.translation", "Translation");
            let translated = gtk::Label::new(Some(&format!(
                "{translation_prefix}: {}",
                entry.translated_text
            )));
            translated.set_xalign(0.0);
            translated.set_wrap(true);
            translated.set_selectable(true);
            row.append(&translated);
            let identity_prefix = localization::text(locale, "dialog.memory", "Translation memory");
            let identity =
                gtk::Label::new(Some(&format!("{identity_prefix}: {}", entry.identity_json)));
            identity.set_xalign(0.0);
            identity.set_wrap(true);
            identity.set_selectable(true);
            identity.add_css_class("dim-label");
            row.append(&identity);
            list.append(&row);
        }
    }
    let scroller = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .child(&list)
        .build();
    root.append(&scroller);
    dialog.set_child(Some(&root));
    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    let export_entries = entries;
    export.connect_clicked(move |_| {
        begin_memory_export(&export_bindings, &export_state, &export_entries);
    });
    let close_dialog = dialog.clone();
    close.connect_clicked(move |_| close_dialog.close());
    dialog.present();
}

// 将翻译记忆字段中的控制字符转义，避免导出内容伪造 TSV 行或列。
fn memory_tsv_field(value: &str) -> String {
    history_tsv_field(value)
}

// 通过 GTK 原生保存对话框异步导出本地翻译记忆。
fn begin_memory_export(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    entries: &[TranslationMemoryEntry],
) {
    let locale = state.borrow().locale();
    let mut contents = String::from(
        "cache_key\tcreated_at\tsource_locale\ttarget_locale\tmodel_id\tsource_text\ttranslated_text\tidentity_json\n",
    );
    for entry in entries {
        contents.push_str(&memory_tsv_field(&entry.cache_key));
        contents.push('\t');
        contents.push_str(&entry.created_at.to_string());
        contents.push('\t');
        contents.push_str(&memory_tsv_field(
            entry.source_locale.as_deref().unwrap_or("auto"),
        ));
        contents.push('\t');
        contents.push_str(&memory_tsv_field(&entry.target_locale));
        contents.push('\t');
        contents.push_str(&memory_tsv_field(&entry.model_id));
        contents.push('\t');
        contents.push_str(&memory_tsv_field(&entry.source_text));
        contents.push('\t');
        contents.push_str(&memory_tsv_field(&entry.translated_text));
        contents.push('\t');
        contents.push_str(&memory_tsv_field(&entry.identity_json));
        contents.push('\n');
    }
    let contents = contents.into_bytes();
    let dialog = gtk::FileDialog::builder()
        .title(localization::text(
            locale,
            "dialog.export_memory",
            "Export translation memory",
        ))
        .accept_label(localization::text(locale, "dialog.save", "Save"))
        .modal(true)
        .build();
    dialog.set_initial_name(Some("linguamesh-translation-memory.tsv"));
    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    dialog.save(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                let callback_bindings = export_bindings.clone();
                let callback_state = Rc::clone(&export_state);
                write_new_file_async(&file, contents, move |write_succeeded| {
                    if write_succeeded {
                        callback_bindings.memory_export_notice.set(true);
                        refresh_ui(&callback_bindings, &callback_state.borrow());
                    } else {
                        show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.memory_export",
                                "The translation memory could not be saved.",
                            ),
                        );
                    }
                });
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(_) => show_file_export_error(
                &export_bindings,
                &localization::text(
                    export_state.borrow().locale(),
                    "error.memory_export",
                    "The translation memory could not be saved.",
                ),
            ),
        },
    );
}

// 拖放和按钮导入共享相同的工作状态边界，避免覆盖正在处理的用户内容。
fn source_import_allowed(state: &AppState) -> bool {
    !state.worker_unavailable()
        && state.pending_profile_deletion().is_none()
        && state.pending_model_selection().is_none()
        && !matches!(
            state.status(),
            AppStatus::Connecting | AppStatus::Translating | AppStatus::Cancelling
        )
}

fn start_event_pump(bindings: &UiBindings, state: &Rc<RefCell<AppState>>, worker: &Rc<CoreWorker>) {
    let event_bindings = bindings.clone();
    let event_state = Rc::clone(state);
    let event_worker = Rc::clone(worker);
    let mut worker_reported_stopped = false;
    let _event_source = glib::timeout_add_local(Duration::from_millis(16), move || {
        let mut state_changed = false;
        for _ in 0..MAX_EVENTS_PER_TICK {
            match event_worker.try_recv() {
                Ok(event) => {
                    if matches!(&event, WorkerEvent::Stopped) {
                        worker_reported_stopped = true;
                    }
                    apply_worker_event(&event_bindings, &event_state, &event_worker, event);
                    state_changed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    {
                        let mut state = event_state.borrow_mut();
                        state.mark_worker_unavailable();
                        if !worker_reported_stopped {
                            let message = localization::text(
                                state.locale(),
                                "error.worker_disconnected",
                                "The core worker disconnected.",
                            );
                            state.record_client_error(message);
                        }
                    }
                    refresh_ui(&event_bindings, &event_state.borrow());
                    return glib::ControlFlow::Break;
                }
            }
        }
        if state_changed {
            refresh_ui(&event_bindings, &event_state.borrow());
        }
        glib::ControlFlow::Continue
    });
}

// 所有工作线程事件在单一入口按精确关联键提交或忽略陈旧结果。
#[allow(clippy::too_many_lines)]
fn apply_worker_event(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &CoreWorker,
    event: WorkerEvent,
) {
    match event {
        WorkerEvent::ProfilesRestored {
            profiles,
            active_profile_id,
        } => {
            let selected_profile = {
                let mut state = state.borrow_mut();
                match state.restore_saved_profiles(profiles, active_profile_id) {
                    Ok(()) => state.selected_saved_profile().cloned(),
                    Err(error) => {
                        state.record_client_error(error.to_string());
                        None
                    }
                }
            };
            rebuild_saved_profile_dropdown(bindings, &state.borrow());
            if let Some(profile) = selected_profile {
                show_saved_profile_in_form(bindings, &profile);
            }
        }
        WorkerEvent::ProfileStorageUnavailable(error) => {
            state.borrow_mut().profile_storage_unavailable(error);
            bindings.remember_profile.set_active(false);
            bindings.history_policy_pending.set(false);
            bindings.history_policy_guard.set(true);
            bindings.history_enabled.set_active(false);
            bindings.history_policy_guard.set(false);
            bindings.memory_policy_guard.set(true);
            bindings.memory_enabled.set_active(false);
            bindings.memory_policy_guard.set(false);
            rebuild_saved_profile_dropdown(bindings, &state.borrow());
        }
        WorkerEvent::TranslationHistoryRestored { count } => {
            state.borrow_mut().restore_translation_history_count(count);
        }
        WorkerEvent::TranslationHistoryPolicyRestored { enabled } => {
            state
                .borrow_mut()
                .restore_translation_history_enabled(enabled);
            bindings.history_policy_guard.set(true);
            bindings.history_enabled.set_active(enabled);
            bindings.history_policy_guard.set(false);
        }
        WorkerEvent::TranslationHistoryPolicyUpdated { enabled } => {
            state
                .borrow_mut()
                .restore_translation_history_enabled(enabled);
            bindings.history_policy_pending.set(false);
            bindings.history_policy_notice.set(Some(enabled));
            bindings.history_policy_guard.set(true);
            bindings.history_enabled.set_active(enabled);
            bindings.history_policy_guard.set(false);
        }
        WorkerEvent::TranslationHistoryUpdated { count } => {
            state.borrow_mut().restore_translation_history_count(count);
            bindings.history_warning.set(false);
            bindings.history_notice.set(false);
        }
        WorkerEvent::TranslationHistoryCleared => {
            state.borrow_mut().clear_translation_history_count();
            bindings.history_clear_pending.set(false);
            bindings.history_warning.set(false);
            bindings.history_notice.set(true);
        }
        WorkerEvent::TranslationHistoryListed { entries, count } => {
            state.borrow_mut().restore_translation_history_count(count);
            let history_worker = worker.command_handle();
            show_history_dialog(bindings, state, &history_worker, entries);
        }
        WorkerEvent::TranslationHistoryActionRejected(error)
        | WorkerEvent::SecretCleanupFailed { error, .. }
        | WorkerEvent::TranslationMemoryActionRejected(error)
        | WorkerEvent::DocumentJobStorageUnavailable(error)
        | WorkerEvent::RoutingProfileActionRejected(error) => {
            state.borrow_mut().record_client_error(error.to_string());
        }
        WorkerEvent::DocumentJobActionRejected(error) => {
            bindings.ocr_pending.set(false);
            state.borrow_mut().record_client_error(error.to_string());
        }
        WorkerEvent::TranslationHistoryPolicyRejected(error) => {
            bindings.history_policy_pending.set(false);
            bindings.history_policy_guard.set(true);
            bindings
                .history_enabled
                .set_active(state.borrow().translation_history_enabled());
            bindings.history_policy_guard.set(false);
            state.borrow_mut().record_client_error(error.to_string());
        }
        WorkerEvent::TranslationHistoryClearRejected(error) => {
            bindings.history_clear_pending.set(false);
            state.borrow_mut().record_client_error(error.to_string());
        }
        WorkerEvent::TranslationHistoryPersistenceFailed(_) => {
            bindings.history_warning.set(true);
            bindings.history_clear_pending.set(false);
        }
        WorkerEvent::TranslationMemoryRestored { count, enabled } => {
            let mut state = state.borrow_mut();
            state.restore_translation_memory_count(count);
            state.restore_translation_memory_enabled(enabled);
            bindings.memory_policy_guard.set(true);
            bindings.memory_enabled.set_active(enabled);
            bindings.memory_policy_guard.set(false);
        }
        WorkerEvent::TranslationMemoryPolicyUpdated { enabled } => {
            state
                .borrow_mut()
                .restore_translation_memory_enabled(enabled);
            bindings.memory_policy_pending.set(false);
            bindings.memory_policy_notice.set(Some(enabled));
            bindings.memory_policy_guard.set(true);
            bindings.memory_enabled.set_active(enabled);
            bindings.memory_policy_guard.set(false);
        }
        WorkerEvent::TranslationMemoryListed { entries, count } => {
            state.borrow_mut().restore_translation_memory_count(count);
            let memory_worker = worker.command_handle();
            show_memory_dialog(bindings, state, &memory_worker, entries);
        }
        WorkerEvent::TranslationMemoryCleared => {
            state.borrow_mut().clear_translation_memory_count();
            bindings.memory_clear_pending.set(false);
            bindings.memory_warning.set(false);
            bindings.memory_notice.set(true);
        }
        WorkerEvent::TranslationMemoryPolicyRejected(error) => {
            bindings.memory_policy_pending.set(false);
            bindings.memory_policy_guard.set(true);
            bindings
                .memory_enabled
                .set_active(state.borrow().translation_memory_enabled());
            bindings.memory_policy_guard.set(false);
            state.borrow_mut().record_client_error(error.to_string());
        }
        WorkerEvent::TranslationMemoryClearRejected(error) => {
            bindings.memory_clear_pending.set(false);
            state.borrow_mut().record_client_error(error.to_string());
        }
        WorkerEvent::TranslationMemoryPersistenceFailed(_) => {
            bindings.memory_warning.set(true);
            bindings.memory_clear_pending.set(false);
        }
        WorkerEvent::RoutingProfilesListed { profiles } => {
            let routing_worker = worker.command_handle();
            show_routing_profiles_dialog(bindings, state, &routing_worker, profiles);
        }
        WorkerEvent::RoutingProfileExported {
            profile_id,
            contents,
        } => {
            begin_routing_profile_export(bindings, state, &profile_id, contents);
        }
        WorkerEvent::RoutingDecisionSelected {
            profile_id,
            provider_id,
            model_id,
            eligible_count,
            rejected_count,
            fallback_count,
            eligible_candidates,
            rejected_candidates,
            ranking_inputs,
            fallback_order,
        } => {
            state
                .borrow_mut()
                .record_routing_decision(RoutingDecisionSummary {
                    profile_id,
                    provider_id,
                    model_id,
                    eligible_count,
                    rejected_count,
                    fallback_count,
                    eligible_candidates,
                    rejected_candidates,
                    ranking_inputs,
                    fallback_order,
                });
        }
        WorkerEvent::RoutingProfileSaved(_) | WorkerEvent::RoutingProfileImported(_) => {
            if let Err(error) = worker.command_handle().list_routing_profiles() {
                state.borrow_mut().record_client_error(error.to_string());
            }
        }
        WorkerEvent::RoutingProfileDeleted { profile_id } => {
            if bindings.selected_routing_profile_id.borrow().as_deref() == Some(profile_id.as_str())
            {
                bindings.selected_routing_profile_id.replace(None);
            }
            if let Err(error) = worker.command_handle().list_routing_profiles() {
                state.borrow_mut().record_client_error(error.to_string());
            }
        }
        WorkerEvent::DocumentJobsRestored { jobs } => {
            if bindings.document_job_id.borrow().is_none()
                && let Some(snapshot) = jobs.first()
            {
                *bindings.document_job_id.borrow_mut() = Some(snapshot.job_id.clone());
                bindings.document_job_state.set(Some(snapshot.state));
                bindings
                    .document_progress
                    .set(Some(document_progress(snapshot)));
                bindings.document_job_guard.set(true);
                let source_text = snapshot.job.source_text();
                bindings.source.set_text(&source_text);
                bindings.document_job_guard.set(false);
                *bindings.document_warnings.borrow_mut() =
                    snapshot.job.warnings().unwrap_or_default();
                restore_document_translation_options(state, snapshot);
            }
        }
        WorkerEvent::DocumentJobsListed { jobs } => {
            let jobs_worker = worker.command_handle();
            show_document_jobs_dialog(bindings, state, &jobs_worker, jobs);
        }
        WorkerEvent::DocumentJobExported {
            source_name,
            target_locale,
            contents,
        } => {
            begin_document_binary_export(bindings, state, &source_name, &target_locale, contents);
        }
        WorkerEvent::DocumentJobUpdated(snapshot) => {
            bindings.ocr_pending.set(false);
            *bindings.document_job_id.borrow_mut() = Some(snapshot.job_id.clone());
            bindings.document_job_state.set(Some(snapshot.state));
            bindings
                .document_progress
                .set(Some(document_progress(&snapshot)));
            *bindings.document_warnings.borrow_mut() = snapshot.job.warnings().unwrap_or_default();
            restore_document_translation_options(state, &snapshot);
            if let Ok(output) = snapshot.job.reconstruct() {
                let mut state = state.borrow_mut();
                match snapshot.state {
                    DocumentJobState::Completed => state.complete_document_translation(output),
                    DocumentJobState::Cancelled => state.cancel_document_translation(output),
                    DocumentJobState::Paused => state.pause_document_translation(output),
                    DocumentJobState::Running => {
                        if state.status() != AppStatus::Translating {
                            let _ = state.begin_document_translation();
                        }
                        state.set_document_output(output);
                    }
                    DocumentJobState::Pending => state.set_document_output(output),
                    DocumentJobState::Failed => state.fail_document_translation(
                        output,
                        TranslationError::new(
                            ErrorKind::Internal,
                            "The document job failed. Retry to continue.",
                        ),
                    ),
                }
            }
        }
        WorkerEvent::DocumentJobSegment { .. } => {}
        WorkerEvent::DemoProviderReady { endpoint } => {
            let should_use_demo = {
                let state = state.borrow();
                state.active_provider().is_none()
                    && state.pending_provider().is_none()
                    && state.saved_profiles().is_empty()
                    && bindings.provider_preset.selected() == 0
                    && bindings.provider_endpoint.text() == DEFAULT_PROVIDER_ENDPOINT
            };
            if should_use_demo {
                bindings.provider_endpoint.set_text(&endpoint);
            }
            state.borrow_mut().mark_worker_ready();
        }
        WorkerEvent::Connected {
            profile,
            models,
            saved_profile,
        } => {
            let profile_was_saved = saved_profile.is_some();
            let model_placeholder = localization::text(
                state.borrow().locale(),
                "option.model.select",
                "Select a model...",
            );
            let mut labels = vec![model_placeholder];
            labels.extend(models.iter().map(|model| model.display_name.clone()));
            let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
            let selected = {
                let mut state = state.borrow_mut();
                match state.provider_connected_with_saved_profile(profile, models, saved_profile) {
                    Ok(()) => {}
                    Err(StateError::UnexpectedProviderConnection) => return,
                    Err(error) => {
                        state.provider_failed(TranslationError::new(
                            ErrorKind::Internal,
                            error.to_string(),
                        ));
                        return;
                    }
                }
                state
                    .selected_model()
                    .and_then(|selected| {
                        state.models().iter().position(|model| model.id == selected)
                    })
                    .map_or(0, |index| index + 1)
            };
            let model_list = gtk::StringList::new(&label_refs);
            bindings.model.set_model(Some(&model_list));
            bindings
                .model
                .set_selected(u32::try_from(selected).unwrap_or(0));
            if profile_was_saved {
                bindings.draft_profile_id.replace(None);
                rebuild_saved_profile_dropdown(bindings, &state.borrow());
            }
        }
        WorkerEvent::ConnectionTested {
            profile_id,
            model_count,
        } => {
            bindings.connection_test_notice.set(true);
            bindings.connection_test_model_count.set(Some(model_count));
            bindings
                .connection_test_profile_id
                .replace(Some(profile_id.as_str().to_owned()));
        }
        WorkerEvent::ConnectionTestRejected { error, .. } => {
            bindings.connection_test_notice.set(false);
            bindings.connection_test_model_count.set(None);
            bindings.connection_test_profile_id.replace(None);
            state.borrow_mut().record_operation_failure(error);
        }
        WorkerEvent::ModelSelected {
            profile_id,
            model_id,
            saved_profile,
        } => {
            let restore_selection = {
                let mut state = state.borrow_mut();
                if state.provider_id() == Some(&profile_id) {
                    let result = state.confirm_model_selection_with_saved_profile(
                        &profile_id,
                        &model_id,
                        saved_profile,
                    );
                    if let Err(error) = result {
                        let _ = state.model_selection_failed(
                            &model_id,
                            TranslationError::new(ErrorKind::Internal, error.to_string()),
                        );
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };
            if restore_selection {
                let confirmed = confirmed_model_index(&state.borrow());
                bindings.model.set_selected(confirmed);
            }
        }
        WorkerEvent::ModelSelectionRejected {
            profile_id,
            model_id,
            error,
        } => {
            let is_current = {
                let state = state.borrow();
                state.provider_id() == Some(&profile_id)
                    && state.pending_model_selection() == Some(model_id.as_str())
            };
            if is_current {
                let _ = state.borrow_mut().model_selection_failed(&model_id, error);
                let confirmed = confirmed_model_index(&state.borrow());
                bindings.model.set_selected(confirmed);
            }
        }
        WorkerEvent::ProfileDeleted { profile_id } => {
            let was_selected = state.borrow().selected_saved_profile_id() == Some(&profile_id);
            let result = state.borrow_mut().confirm_profile_deletion(&profile_id);
            match result {
                Ok(()) => {
                    rebuild_saved_profile_dropdown(bindings, &state.borrow());
                    let form_result = if was_selected {
                        show_new_profile_in_form(bindings, &state.borrow())
                    } else {
                        Ok(())
                    };
                    if let Err(error) = form_result {
                        state.borrow_mut().provider_failed(error);
                    }
                }
                Err(StateError::UnexpectedProfileDeletion) => {}
                Err(error) => state.borrow_mut().record_client_error(error.to_string()),
            }
        }
        WorkerEvent::ProfileDeletionRejected { profile_id, error } => {
            let is_current = state.borrow().pending_profile_deletion() == Some(&profile_id);
            if is_current {
                let _ = state
                    .borrow_mut()
                    .profile_deletion_failed(&profile_id, error);
            }
        }
        WorkerEvent::Translation(event) => {
            let completed = matches!(&event, TranslationEvent::Completed { .. });
            let result = state.borrow_mut().apply_translation_event(event);
            if let Err(error) = result {
                state.borrow_mut().record_stream_error(error.to_string());
                if let Err(error) = worker.try_send(WorkerCommand::Cancel) {
                    state.borrow_mut().record_client_error(error.to_string());
                }
            } else if completed && state.borrow().status() == AppStatus::Completed {
                send_translation_notification(bindings, state.borrow().locale());
            }
        }
        WorkerEvent::RoutingFallbackSelected { .. } | WorkerEvent::FallbackSelected { .. } => {
            bindings.fallback_notice.set(true);
        }
        WorkerEvent::OperationFailed(error) | WorkerEvent::TranslationRejected(error) => {
            state.borrow_mut().record_operation_failure(error);
        }
        WorkerEvent::ProviderRejected { profile, error } => {
            let mut state = state.borrow_mut();
            let _ = state.provider_connection_failed(&profile, error);
        }
        WorkerEvent::Rejected(error) => {
            if !state.borrow().worker_ready() {
                state.borrow_mut().mark_worker_unavailable();
            }
            if !matches!(
                state.borrow().status(),
                AppStatus::Translating | AppStatus::Cancelling
            ) {
                state.borrow_mut().provider_failed(error);
            }
        }
        WorkerEvent::Stopped => {
            let is_terminal = operation_is_terminal(state.borrow().status());
            let mut state = state.borrow_mut();
            state.mark_worker_unavailable();
            if !is_terminal {
                let message = localization::text(
                    state.locale(),
                    "error.worker_stopped",
                    "The core worker stopped.",
                );
                state.record_client_error(message);
            }
        }
    }
}

// 翻译完成时只发送不含源文和译文的桌面通知，避免通知服务保存敏感内容。
fn send_translation_notification(bindings: &UiBindings, locale: UiLocale) {
    let notification = gtk::gio::Notification::new(&localization::text(
        locale,
        "notification.title",
        "Translation complete",
    ));
    notification.set_body(Some(&localization::text(
        locale,
        "notification.body",
        "The translated output is ready in LinguaMesh.",
    )));
    bindings
        .application
        .send_notification(Some("translation-complete"), &notification);
}

const fn operation_is_terminal(status: AppStatus) -> bool {
    matches!(
        status,
        AppStatus::Completed | AppStatus::Cancelled | AppStatus::Failed
    )
}

fn refresh_active_provider_label(bindings: &UiBindings, state: &AppState) {
    let active_mode = if state.active_provider_is_saved() {
        localization::text(state.locale(), "provider.mode.saved", "saved")
    } else {
        localization::text(state.locale(), "provider.mode.session", "session only")
    };
    let pending_mode = if state.pending_provider_will_be_saved() {
        localization::text(
            state.locale(),
            "provider.mode.pending_saved",
            "will be saved with Secret Service credential protection when supplied",
        )
    } else {
        localization::text(state.locale(), "provider.mode.session", "session only")
    };
    match (state.active_provider(), state.pending_provider()) {
        (Some(active), Some(pending)) => bindings.active_provider.set_label(&localized_template(
            state.locale(),
            "provider.active_pending",
            "Active provider remains {active} ({active_mode}); connecting {pending} ({pending_mode}).",
            &[
                ("{active}", active.display_name()),
                ("{active_mode}", &active_mode),
                ("{pending}", pending.display_name()),
                ("{pending_mode}", &pending_mode),
            ],
        )),
        (None, Some(pending)) => bindings.active_provider.set_label(&localized_template(
            state.locale(),
            "provider.connecting",
            "Connecting {provider} ({mode}).",
            &[("{provider}", pending.display_name()), ("{mode}", &pending_mode)],
        )),
        (Some(active), None) => {
            bindings.active_provider.set_label(&localized_template(
                state.locale(),
                "provider.active_with_mode",
                "Active provider: {provider} ({mode})",
                &[
                    ("{provider}", active.display_name()),
                    ("{mode}", &active_mode),
                ],
            ));
        }
        (None, None) if !state.saved_profiles().is_empty() => {
            bindings.active_provider.set_label(&localization::text(
                state.locale(),
                "provider.saved_restored",
                "Saved non-secret profiles were restored. Choose one, enter its credential if required, then connect.",
            ));
        }
        (None, None) => bindings.active_provider.set_label(&localization::text(
            state.locale(),
            "provider.disconnected",
            "No provider connected. Credentials stay session-only unless remembered through Secret Service.",
        )),
    }
}

#[allow(clippy::too_many_lines)]
fn refresh_onboarding(bindings: &UiBindings, state: &AppState) {
    let onboarding_phase = state.onboarding_stage();
    let (title, mut detail) = match onboarding_phase {
        OnboardingStage::Starting => (
            localization::text(
                state.locale(),
                "onboarding.stage.starting",
                "Provider setup · Starting",
            ),
            localization::text(
                state.locale(),
                "onboarding.detail.starting",
                "Checking profile storage and starting the local validation provider. No provider connection is made automatically.",
            ),
        ),
        OnboardingStage::Unavailable => (
            localization::text(
                state.locale(),
                "onboarding.stage.unavailable",
                "Provider setup · Unavailable",
            ),
            localization::text(
                state.locale(),
                "onboarding.detail.unavailable",
                "The core worker is unavailable. Restart the application and review any error below; no provider request can be sent.",
            ),
        ),
        OnboardingStage::ConfigureProvider => {
            let detail = if state.profile_storage_status() == ProfileStorageStatus::Unavailable {
                localization::text(
                    state.locale(),
                    "onboarding.detail.storage_unavailable",
                    "Saved profile storage is unavailable. Configure a provider below and leave Remember off; credentials stay in memory for this session only.",
                )
            } else if state.saved_profiles().is_empty() {
                localization::text(
                    state.locale(),
                    "onboarding.detail.configure_empty",
                    "Create a provider profile below, enter a credential only if required, then choose Connect. Remembering uses Secret Service; otherwise the credential remains in memory for this session only.",
                )
            } else {
                localization::text(
                    state.locale(),
                    "onboarding.detail.configure_saved",
                    "Choose a saved profile below, re-enter its credential if required, then choose Connect. Restored profiles never connect automatically.",
                )
            };
            (
                localization::text(
                    state.locale(),
                    "onboarding.stage.configure",
                    "Provider setup · Step 1 of 2",
                ),
                detail,
            )
        }
        OnboardingStage::Connecting => {
            let detail = state.pending_provider().map_or_else(
                || localization::text(
                    state.locale(),
                    "onboarding.detail.connecting_generic",
                    "Validating the provider and discovering models. The previous active provider remains unchanged until this succeeds.",
                ),
                |profile| {
                    localized_template(
                        state.locale(),
                        "onboarding.detail.connecting",
                        "Validating {provider} [{profile_id}] and discovering models. The previous active provider remains unchanged until this succeeds.",
                        &[
                            ("{provider}", profile.display_name()),
                            ("{profile_id}", profile.id().as_str()),
                        ],
                    )
                },
            );
            (
                localization::text(
                    state.locale(),
                    "onboarding.stage.connecting",
                    "Provider setup · Connecting",
                ),
                detail,
            )
        }
        OnboardingStage::SelectModel => {
            let detail = state.pending_model_selection().map_or_else(
                || localization::text(
                    state.locale(),
                    "onboarding.detail.select_model",
                    "Choose a discovered model. Translation remains disabled until the selection is confirmed.",
                ),
                |model| localized_template(
                    state.locale(),
                    "onboarding.detail.confirm_model",
                    "Confirming model {model}. Translation remains disabled until this selection is committed.",
                    &[("{model}", model)],
                ),
            );
            (
                localization::text(
                    state.locale(),
                    "onboarding.stage.select_model",
                    "Provider setup · Step 2 of 2",
                ),
                detail,
            )
        }
        OnboardingStage::Ready => {
            let provider = state.active_provider().map_or_else(
                || localization::text(state.locale(), "status.unavailable", "Unavailable"),
                |profile| format!("{} [{}]", profile.display_name(), profile.id().as_str()),
            );
            let unavailable =
                localization::text(state.locale(), "status.unavailable", "Unavailable");
            let model = state.selected_model().unwrap_or(unavailable.as_str());
            (
                localization::text(
                    state.locale(),
                    "onboarding.stage.ready",
                    "Provider setup · Ready",
                ),
                localized_template(
                    state.locale(),
                    "onboarding.detail.ready",
                    "Next request: {provider} · {model}. Use the saved-profile list and Connect to switch deliberately.",
                    &[("{provider}", &provider), ("{model}", model)],
                ),
            )
        }
    };
    if state.profile_storage_status() == ProfileStorageStatus::Unavailable
        && onboarding_phase != OnboardingStage::ConfigureProvider
    {
        detail.push(' ');
        detail.push_str(&localization::text(
            state.locale(),
            "onboarding.detail.persistence_warning",
            "Saved profile storage is unavailable; profile persistence is disabled, Remember stays off, and credentials remain session only.",
        ));
    }
    bindings.onboarding.set_visible(true);
    bindings.onboarding_title.set_label(&title);
    bindings.onboarding_detail.set_label(&detail);
}

#[allow(clippy::too_many_lines)]
fn refresh_localized_actions(bindings: &UiBindings, locale: UiLocale) {
    let open_source = localization::text(locale, "action.open_source", "Open text file");
    let open_source_label = format!("_{open_source}");
    let open_source_tooltip = localization::text(
        locale,
        "tooltip.open_source",
        "Load a UTF-8 text file into the source editor",
    );
    let document_jobs = localization::text(locale, "action.document_jobs", "Document jobs");
    let document_jobs_label = format!("_{document_jobs}");
    let enable_ocr = localization::text(
        locale,
        "settings.enable_ocr",
        "Enable OCR for image-only PDF",
    );
    let enable_ocr_tooltip = localization::text(
        locale,
        "tooltip.enable_ocr",
        "When enabled, use the optional Tesseract plugin for image-only PDF pages and import page-marked text",
    );
    let translate = localization::text(locale, "action.translate", "Translate");
    let retry_translation =
        localization::text(locale, "action.retry_translation", "Retry translation");
    let retry_translation_tooltip = localization::text(
        locale,
        "tooltip.retry_translation",
        "Retry the last failed or cancelled text translation",
    );
    let stop = localization::text(locale, "accessibility.stop_translation", "Stop translation");
    let pause = localization::text(locale, "action.pause_document", "Pause document");
    let resume = localization::text(locale, "action.resume_document", "Resume document");
    let retry = localization::text(locale, "action.retry_document", "Retry document");
    let export = localization::text(locale, "action.export_output", "Export translation");
    let open_output = localization::text(locale, "action.open_output", "Open exported output");
    let connect = localization::text(locale, "action.connect", "Connect");
    let test_connection = localization::text(locale, "action.test_connection", "Test connection");
    let test_connection_tooltip = localization::text(
        locale,
        "tooltip.test_connection",
        "Check the provider model endpoint without switching or saving the active profile",
    );
    let remove_profile =
        localization::text(locale, "action.remove_profile", "Remove saved profile");
    let remember_profile = localization::text(
        locale,
        "option.remember_profile",
        "Remember profile, model, and credential in Secret Service",
    );
    let import_glossary = localization::text(locale, "action.import_glossary", "Import glossary");
    let export_glossary = localization::text(locale, "action.export_glossary", "Export glossary");
    let incognito = localization::text(locale, "settings.incognito", "Incognito mode");
    let history_enabled =
        localization::text(locale, "settings.save_history", "Save translation history");
    let history = localization::text(locale, "action.view_history", "View history");
    let clear_history = localization::text(locale, "action.clear_history", "Clear history");
    let memory_enabled =
        localization::text(locale, "settings.save_memory", "Save translation memory");
    let memory = localization::text(locale, "action.view_memory", "View translation memory");
    let clear_memory =
        localization::text(locale, "action.clear_memory", "Clear translation memory");
    let routing_profiles =
        localization::text(locale, "action.routing_profiles", "Routing profiles");
    let fallback_action =
        localization::text(locale, "action.enable_fallback", "Allow approved fallback");
    let fallback_tooltip = localization::text(
        locale,
        "tooltip.fallback",
        "Retry only retryable network failures with this saved provider; document jobs, cancellation, and credential failures never fall back",
    );
    let translate_label = format!("_{translate}");
    let stop_label = format!("_{stop}");
    bindings.open_source.set_label(&open_source_label);
    bindings
        .open_source
        .set_tooltip_text(Some(&open_source_tooltip));
    bindings
        .ocr_enabled
        .set_label(Some(&format!("_{enable_ocr}")));
    bindings
        .ocr_enabled
        .set_tooltip_text(Some(&enable_ocr_tooltip));
    bindings.document_jobs.set_label(&document_jobs_label);
    bindings
        .document_jobs
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.document_jobs",
            "Inspect persisted document jobs and their progress",
        )));
    bindings.translate.set_label(&translate_label);
    bindings
        .retry_translation
        .set_label(&format!("_{retry_translation}"));
    bindings
        .retry_translation
        .set_tooltip_text(Some(&retry_translation_tooltip));
    bindings
        .retry_translation
        .update_property(&[gtk::accessible::Property::Label(&retry_translation)]);
    bindings.export_output.set_label(&format!("_{export}"));
    bindings
        .export_output
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.export_output",
            "Save the translated output to a new file",
        )));
    bindings.open_output.set_label(&format!("_{open_output}"));
    bindings
        .open_output
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.open_output",
            "Open the most recently exported translation output",
        )));
    bindings.stop.set_label(&stop_label);
    bindings.pause_document.set_label(&format!("_{pause}"));
    bindings.resume_document.set_label(&format!("_{resume}"));
    bindings.retry_document.set_label(&format!("_{retry}"));
    bindings.connect.set_label(&format!("_{connect}"));
    bindings
        .test_connection
        .set_label(&format!("_{test_connection}"));
    bindings
        .test_connection
        .set_tooltip_text(Some(&test_connection_tooltip));
    bindings.remove_saved_profile.set_label(&remove_profile);
    bindings.remember_profile.set_label(Some(&remember_profile));
    bindings
        .import_glossary
        .set_label(&format!("_{import_glossary}"));
    bindings
        .export_glossary
        .set_label(&format!("_{export_glossary}"));
    bindings.incognito.set_label(Some(&format!("_{incognito}")));
    bindings
        .history_enabled
        .set_label(Some(&format!("_{history_enabled}")));
    bindings.history.set_label(&format!("_{history}"));
    bindings
        .clear_history
        .set_label(&format!("_{clear_history}"));
    bindings
        .memory_enabled
        .set_label(Some(&format!("_{memory_enabled}")));
    bindings.memory.set_label(&format!("_{memory}"));
    bindings.clear_memory.set_label(&format!("_{clear_memory}"));
    bindings
        .routing_profiles
        .set_label(&format!("_{routing_profiles}"));
    bindings
        .fallback_enabled
        .set_label(Some(&format!("_{fallback_action}")));
    // 在运行时切换界面语言时同步更新复选框的可访问名称。
    bindings
        .fallback_enabled
        .update_property(&[gtk::accessible::Property::Label(&fallback_action)]);
    bindings
        .fallback_enabled
        .set_tooltip_text(Some(&fallback_tooltip));
    bindings
        .import_glossary
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.import_glossary",
            "Load glossary rules from a UTF-8 CSV file",
        )));
    bindings
        .export_glossary
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.export_glossary",
            "Save the current glossary rules as a UTF-8 CSV file",
        )));
    bindings
        .incognito
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.incognito",
            "Do not persist source, output, history, or translation-memory data for this request",
        )));
    bindings.history_enabled.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.save_history",
        "Persist completed standard translations in local history; existing entries are kept when disabled",
    )));
    bindings
        .clear_history
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.clear_history",
            "Delete all locally stored translation history",
        )));
    bindings.history.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.view_history",
        "Inspect, export, or delete individual local translation history entries",
    )));
    bindings
        .memory_enabled
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.save_memory",
            "Reuse and persist completed standard translations in local translation memory",
        )));
    bindings.memory.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.view_memory",
        "Inspect, export, or delete individual local translation memory entries",
    )));
    bindings
        .routing_profiles
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.routing_profiles",
            "Create, inspect, and delete non-secret routing planner profiles",
        )));
    bindings
        .clear_memory
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.clear_memory",
            "Delete all locally stored translation memory entries",
        )));
    bindings
        .remove_saved_profile
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.remove_profile",
            "Remove the selected saved profile without disconnecting its current session",
        )));
    bindings
        .remember_profile
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.remember_profile",
            "Save non-secret profile data and the credential through Secret Service",
        )));
    bindings
        .saved_profile
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.saved_profile",
            "Choose a saved non-secret profile or create a new profile",
        )));
    bindings
        .provider_preset
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.provider_preset",
            "Choose the provider protocol used for model discovery and streaming",
        )));
    bindings
        .provider_name
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.provider_name",
            "Session-only provider display name",
        )));
    bindings
        .provider_endpoint
        .set_tooltip_text(Some(&provider_endpoint_tooltip(
            locale,
            bindings.provider_preset.selected(),
        )));
    set_labeled_control_label(
        bindings.manual_model.upcast_ref(),
        &localized_mnemonic(locale, "field.model", "Model"),
    );
    bindings
        .manual_model
        .set_placeholder_text(Some(&localization::text(
            locale,
            "option.model.manual",
            "Enter a model ID manually...",
        )));
    bindings
        .manual_model
        .set_tooltip_text(Some(&provider_model_tooltip(
            locale,
            bindings.provider_preset.selected(),
        )));
    bindings
        .provider_credential
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.credential",
            "Optional credential; it is kept in memory unless remembered through Secret Service",
        )));
    bindings
        .stop
        .update_property(&[gtk::accessible::Property::Label(&stop)]);
}

#[allow(clippy::too_many_lines)]
fn refresh_localized_widgets(bindings: &UiBindings, locale: UiLocale) {
    bindings
        .window
        .set_title(Some(&localization::text(locale, "app.title", "LinguaMesh")));
    bindings.provider_title.set_label(&localization::text(
        locale,
        "section.provider_profiles",
        "Provider profiles",
    ));
    bindings.provider_note.set_label(&localization::text(
        locale,
        "profile.storage_note",
        "Names, endpoints, model preferences, and credentials can be remembered through Secret Service. Credentials are cleared from this form immediately and remain session-only when secure storage is unavailable. Removing a saved profile does not disconnect its current session.",
    ));
    bindings.source_label.set_label(&localization::text(
        locale,
        "field.source_text",
        "Source text",
    ));
    bindings.output_label.set_label(&localization::text(
        locale,
        "field.translation",
        "Translation",
    ));
    let source_accessible =
        localization::text(locale, "accessibility.source_content", "Text to translate");
    let output_accessible = localization::text(
        locale,
        "accessibility.translation_output",
        "Streamed translation output",
    );
    bindings
        .source_view
        .update_property(&[gtk::accessible::Property::Label(&source_accessible)]);
    bindings
        .output_view
        .update_property(&[gtk::accessible::Property::Label(&output_accessible)]);
    set_labeled_control_label(
        bindings.provider_preset.upcast_ref(),
        &localized_mnemonic(locale, "label.provider_preset", "Provider preset"),
    );
    refresh_dropdown_labels(&bindings.provider_preset, &provider_preset_labels(locale));
    set_labeled_control_label(
        bindings.provider_name.upcast_ref(),
        &localized_mnemonic(locale, "provider.name", "Provider name"),
    );
    set_labeled_control_label(
        bindings.provider_endpoint.upcast_ref(),
        &localized_mnemonic(
            locale,
            "label.endpoint_loopback",
            "Endpoint (loopback example)",
        ),
    );
    set_labeled_control_label(
        bindings.provider_credential.upcast_ref(),
        &localized_mnemonic(
            locale,
            "label.credential",
            "Credential (optional; secure when remembered)",
        ),
    );
    set_labeled_control_label(
        bindings.model.upcast_ref(),
        &localized_mnemonic(locale, "field.model", "Model"),
    );
    set_labeled_control_label(
        bindings.saved_profile.upcast_ref(),
        &localized_mnemonic(locale, "label.saved_profile", "Saved profile"),
    );
    bindings
        .fallback_profile_label
        .set_label(&localized_mnemonic(
            locale,
            "label.fallback_profile",
            "Fallback provider",
        ));
    set_labeled_control_label(
        bindings.source_locale.upcast_ref(),
        &localized_mnemonic(locale, "label.source_language", "Source language"),
    );
    set_labeled_control_label(
        bindings.target_locale.upcast_ref(),
        &localized_mnemonic(locale, "settings.target_language", "Target language"),
    );
    set_labeled_control_label(
        bindings.quality_mode.upcast_ref(),
        &localized_mnemonic(locale, "label.quality_mode", "Quality mode"),
    );
    set_labeled_control_label(
        bindings.translation_preset.upcast_ref(),
        &localized_mnemonic(locale, "label.translation_preset", "Translation preset"),
    );
    set_labeled_control_label(
        bindings.glossary.upcast_ref(),
        &localized_mnemonic(locale, "field.glossary", "Glossary"),
    );
    bindings
        .glossary
        .set_placeholder_text(Some(&localization::text(
            locale,
            "field.glossary.placeholder",
            "source => target; Product Name => Product Name",
        )));
    bindings.glossary.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.glossary",
        "Optional semicolon-separated source => target glossary rules; entries stay in memory for this translation.",
    )));
    set_labeled_control_label(
        bindings.theme.upcast_ref(),
        &localization::text(locale, "settings.theme", "Theme"),
    );
    set_labeled_control_label(
        bindings.locale.upcast_ref(),
        &localization::text(locale, "settings.ui_language", "Interface language"),
    );
    refresh_dropdown_labels(
        &bindings.theme,
        &[
            localization::text(locale, "theme.system", "System"),
            localization::text(locale, "theme.light", "Light"),
            localization::text(locale, "theme.dark", "Dark"),
        ],
    );
    refresh_dropdown_labels(
        &bindings.source_locale,
        &[
            localization::text(locale, "option.source.auto", "Auto"),
            localization::text(locale, "option.source.english", "English"),
            localization::text(locale, "option.source.chinese", "Chinese"),
        ],
    );
    refresh_dropdown_labels(
        &bindings.target_locale,
        &[
            localization::text(
                locale,
                "option.target.chinese_simplified",
                "Chinese (Simplified)",
            ),
            localization::text(locale, "option.target.english", "English"),
            localization::text(locale, "option.target.japanese", "Japanese"),
        ],
    );
    refresh_dropdown_labels(&bindings.quality_mode, &quality_mode_labels(locale));
    bindings.quality_mode.set_tooltip_text(Some(&localization::text(
        locale,
        "tooltip.quality_mode",
        "Fast uses one direct pass; Balanced adds deterministic structure checks; Best asks for an internal critique and revision.",
    )));
    refresh_dropdown_labels(
        &bindings.translation_preset,
        &translation_preset_labels(locale),
    );
    bindings
        .translation_preset
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.translation_preset",
            "Apply a bounded domain, tone, formality, and audience preference to the next request",
        )));
    let locale_labels = UiLocale::ALL
        .map(|displayed_locale| localized_locale_name(locale, displayed_locale))
        .to_vec();
    refresh_dropdown_labels(&bindings.locale, &locale_labels);
    refresh_model_placeholder(&bindings.model, locale);
    refresh_profile_placeholder(&bindings.saved_profile, locale);
    bindings
        .diagnostics_panel
        .set_label(Some(&localization::text(
            locale,
            "diagnostics.title",
            "Diagnostics",
        )));
}

// 将文档格式转换为稳定、可读且不依赖 Rust Debug 输出的标签。
fn document_format_label(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::Txt => "TXT",
        DocumentFormat::Markdown => "Markdown",
        DocumentFormat::Srt => "SRT",
        DocumentFormat::WebVtt => "WebVTT",
        DocumentFormat::Csv => "CSV",
        DocumentFormat::Html => "HTML",
        DocumentFormat::Json => "JSON",
        DocumentFormat::Docx => "DOCX",
        DocumentFormat::Pptx => "PPTX",
        DocumentFormat::Xlsx => "XLSX",
        DocumentFormat::Epub => "EPUB",
        DocumentFormat::Pdf => "PDF",
    }
}

// 通过 canonical catalog 渲染文档任务的生命周期状态。
fn localized_document_job_state(locale: UiLocale, state: DocumentJobState) -> String {
    let (key, fallback) = match state {
        DocumentJobState::Pending => ("status.document_job_pending", "Pending"),
        DocumentJobState::Running => ("status.document_job_running", "Running"),
        DocumentJobState::Paused => ("status.document_job_paused", "Paused"),
        DocumentJobState::Completed => ("status.document_job_completed", "Completed"),
        DocumentJobState::Cancelled => ("status.document_job_cancelled", "Cancelled"),
        DocumentJobState::Failed => ("status.document_job_failed", "Failed"),
    };
    localization::text(locale, key, fallback)
}

fn localized_status_label(locale: UiLocale, status: AppStatus) -> String {
    match status {
        AppStatus::Disconnected => {
            localization::text(locale, "status.disconnected", "Disconnected")
        }
        AppStatus::Connecting => localization::text(locale, "status.connecting", "Connecting"),
        AppStatus::Ready => localization::text(locale, "status.ready", "Ready"),
        AppStatus::Translating => localization::text(locale, "status.translating", "Translating…"),
        AppStatus::Cancelling => localization::text(locale, "status.cancelling", "Cancelling"),
        AppStatus::Completed => localization::text(locale, "status.completed", "Completed"),
        AppStatus::Cancelled => localization::text(
            locale,
            "status.cancelled",
            "Translation cancelled. Partial output was kept.",
        ),
        AppStatus::Failed => localization::text(locale, "status.failed", "Failed"),
    }
}

// 将 Core 提供的文档限制转换为不包含源内容的本地化提示。
fn localized_document_warnings(locale: UiLocale, warnings: &[DocumentWarning]) -> String {
    let mut messages = Vec::new();
    if warnings
        .iter()
        .any(|warning| matches!(&warning.kind, DocumentWarningKind::PdfReconstructionLimited))
    {
        messages.push(localization::text(
            locale,
            "warning.pdf_reconstruction_limited",
            "PDF layout fidelity is limited; page association is preserved, but pixel-identical output is not guaranteed.",
        ));
    }
    let image_pages = warning_pages(warnings, &DocumentWarningKind::PdfImageOnlyPage);
    if !image_pages.is_empty() {
        messages.push(localized_template(
            locale,
            "warning.pdf_image_only_pages",
            "PDF page(s) {pages} contain no extractable text; OCR is not enabled.",
            &[("{pages}", image_pages.as_str())],
        ));
    }
    let uncertain_pages = warning_pages(warnings, &DocumentWarningKind::PdfUncertainReadingOrder);
    if !uncertain_pages.is_empty() {
        messages.push(localized_template(
            locale,
            "warning.pdf_uncertain_order",
            "PDF page(s) {pages} have uncertain reading order; review the structured output.",
            &[("{pages}", uncertain_pages.as_str())],
        ));
    }
    let long_line_cues = warning_cues(warnings, &DocumentWarningKind::SubtitleLineLengthExceeded);
    if !long_line_cues.is_empty() {
        messages.push(localized_template(
            locale,
            "warning.subtitle_line_length",
            "Subtitle cue(s) {cues} exceed the {limit}-character line guidance.",
            &[
                ("{cues}", long_line_cues.as_str()),
                ("{limit}", &DEFAULT_SUBTITLE_MAX_LINE_CHARS.to_string()),
            ],
        ));
    }
    let fast_cues = warning_cues(warnings, &DocumentWarningKind::SubtitleReadingSpeedHigh);
    if !fast_cues.is_empty() {
        messages.push(localized_template(
            locale,
            "warning.subtitle_reading_speed",
            "Subtitle cue(s) {cues} exceed the {limit}-character-per-second reading-speed guidance.",
            &[
                ("{cues}", fast_cues.as_str()),
                ("{limit}", &DEFAULT_SUBTITLE_MAX_READING_SPEED.to_string()),
            ],
        ));
    }
    messages.join(" ")
}

// 提取并排序 warning 涉及的页码，避免向诊断或日志写入文档内容。
fn warning_pages(warnings: &[DocumentWarning], kind: &DocumentWarningKind) -> String {
    let mut pages = warnings
        .iter()
        .filter(|warning| &warning.kind == kind)
        .filter_map(|warning| warning.page)
        .collect::<Vec<_>>();
    pages.sort_unstable();
    pages.dedup();
    pages
        .into_iter()
        .map(|page| page.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

// 提取并排序 warning 涉及的字幕 cue 序号，避免向诊断或日志写入文档内容。
fn warning_cues(warnings: &[DocumentWarning], kind: &DocumentWarningKind) -> String {
    let mut cues = warnings
        .iter()
        .filter(|warning| &warning.kind == kind)
        .filter_map(|warning| warning.cue)
        .collect::<Vec<_>>();
    cues.sort_unstable();
    cues.dedup();
    cues.into_iter()
        .map(|cue| cue.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[allow(clippy::too_many_lines)]
fn refresh_ui(bindings: &UiBindings, state: &AppState) {
    refresh_localized_actions(bindings, state.locale());
    refresh_localized_widgets(bindings, state.locale());
    rebuild_fallback_profile_dropdown(bindings, state);
    bindings
        .workspace
        .set_direction(if state.locale().is_rtl() {
            gtk::TextDirection::Rtl
        } else {
            gtk::TextDirection::Ltr
        });
    bindings.output.set_text(state.output());
    refresh_text_metrics(bindings, state.locale(), state.usage());
    let document_state = bindings.document_job_state.get();
    let status_label = if bindings.ocr_pending.get() {
        localization::text(state.locale(), "status.ocr_running", "Running OCR")
    } else if state.worker_unavailable() {
        localization::text(state.locale(), "status.unavailable", "Unavailable")
    } else if !state.worker_ready() {
        localization::text(state.locale(), "status.starting", "Starting")
    } else if state.pending_profile_deletion().is_some() {
        localization::text(
            state.locale(),
            "status.removing_profile",
            "Removing saved profile",
        )
    } else if state.pending_model_selection().is_some() {
        localization::text(state.locale(), "status.selecting_model", "Selecting model")
    } else if document_state == Some(DocumentJobState::Paused) {
        localization::text(state.locale(), "status.document_paused", "Document paused")
    } else {
        localized_status_label(state.locale(), state.status())
    };
    bindings.status.set_label(&format!(
        "{}: {status_label}",
        localization::text(state.locale(), "status.label", "Status")
    ));
    if let Some((completed, total)) = bindings.document_progress.get() {
        let progress_label = localized_template(
            state.locale(),
            "status.document_progress",
            "{completed} of {total} segments translated",
            &[
                ("{completed}", &completed.to_string()),
                ("{total}", &total.to_string()),
            ],
        );
        let fraction = if total == 0 {
            0.0
        } else {
            let completed = u32::try_from(completed.min(total)).unwrap_or(u32::MAX);
            let total = u32::try_from(total).unwrap_or(u32::MAX);
            (f64::from(completed) / f64::from(total)).clamp(0.0, 1.0)
        };
        bindings.progress.set_fraction(fraction);
        bindings.progress.set_text(Some(&progress_label));
        bindings
            .progress
            .update_property(&[gtk::accessible::Property::Label(&progress_label)]);
        bindings.progress.set_visible(true);
        bindings.partial.set_label("");
    } else {
        bindings.progress.set_fraction(0.0);
        bindings.progress.set_text(None);
        bindings.progress.set_visible(false);
        let partial_label = if state.has_partial_output() {
            localization::text(state.locale(), "status.partial_output", "Partial output")
        } else {
            String::new()
        };
        bindings.partial.set_label(&partial_label);
    }
    let error_text = state.localized_error_text(state.locale());
    let has_error = error_text.is_some();
    bindings
        .error
        .set_label(error_text.as_deref().unwrap_or_default());
    bindings.error.set_visible(has_error);
    if has_error {
        bindings.error.reset_state(gtk::AccessibleState::Hidden);
    } else {
        bindings
            .error
            .update_state(&[gtk::accessible::State::Hidden(true)]);
    }
    let document_warning_text =
        localized_document_warnings(state.locale(), &bindings.document_warnings.borrow());
    let locale_note = if bindings.fallback_notice.get() {
        localization::text(
            state.locale(),
            "status.fallback_selected",
            "The approved fallback provider was selected; content may be sent there.",
        )
    } else if bindings.connection_test_notice.get() {
        let model_count = bindings
            .connection_test_model_count
            .get()
            .unwrap_or_default()
            .to_string();
        let provider = bindings
            .connection_test_profile_id
            .borrow()
            .clone()
            .unwrap_or_else(|| "provider".to_owned());
        localized_template(
            state.locale(),
            "status.connection_tested",
            "Connection test passed for {provider}; {model_count} models are available.",
            &[
                ("{provider}", provider.as_str()),
                ("{model_count}", &model_count),
            ],
        )
    } else if bindings.export_notice.get() {
        localization::text(
            state.locale(),
            "status.exported",
            "Translation saved to the selected file.",
        )
    } else if bindings.report_export_notice.get() {
        localization::text(
            state.locale(),
            "status.report_exported",
            "Translation report saved to the selected file.",
        )
    } else if bindings.history_warning.get() {
        localization::text(
            state.locale(),
            "status.history_not_saved",
            "The translation completed, but local history could not be saved.",
        )
    } else if bindings.history_export_notice.get() {
        localization::text(
            state.locale(),
            "status.history_exported",
            "Translation history was exported to the selected file.",
        )
    } else if bindings.history_notice.get() {
        localization::text(
            state.locale(),
            "status.history_cleared",
            "Local translation history was cleared.",
        )
    } else if let Some(enabled) = bindings.history_policy_notice.get() {
        if enabled {
            localization::text(
                state.locale(),
                "status.history_enabled",
                "Local translation history is enabled.",
            )
        } else {
            localization::text(
                state.locale(),
                "status.history_disabled",
                "Local translation history is disabled; existing entries are kept.",
            )
        }
    } else if bindings.memory_warning.get() {
        localization::text(
            state.locale(),
            "status.memory_not_saved",
            "The translation completed, but local translation memory could not be saved.",
        )
    } else if bindings.memory_export_notice.get() {
        localization::text(
            state.locale(),
            "status.memory_exported",
            "Translation memory was exported to the selected file.",
        )
    } else if bindings.memory_notice.get() {
        localization::text(
            state.locale(),
            "status.memory_cleared",
            "Local translation memory was cleared.",
        )
    } else if let Some(enabled) = bindings.memory_policy_notice.get() {
        if enabled {
            localization::text(
                state.locale(),
                "status.memory_enabled",
                "Local translation memory is enabled.",
            )
        } else {
            localization::text(
                state.locale(),
                "status.memory_disabled",
                "Local translation memory is disabled; existing entries are kept.",
            )
        }
    } else if state.translation_memory_count() > 0 {
        localized_template(
            state.locale(),
            "status.memory_count",
            "Stored translation memory entries: {count}",
            &[("{count}", &state.translation_memory_count().to_string())],
        )
    } else if state.translation_history_count() > 0 {
        localized_template(
            state.locale(),
            "status.history_count",
            "Stored translation history entries: {count}",
            &[("{count}", &state.translation_history_count().to_string())],
        )
    } else if bindings.glossary_notice.get() {
        localization::text(
            state.locale(),
            "status.glossary_ready",
            "Glossary CSV is ready.",
        )
    } else if !document_warning_text.is_empty() {
        document_warning_text
    } else if state.is_incognito() {
        localization::text(
            state.locale(),
            "status.incognito",
            "Incognito mode is active for this request.",
        )
    } else {
        localization::text(
            state.locale(),
            "locale.note.drafts",
            "Simplified Chinese resources are loaded from the pinned runtime catalog; translations remain unreviewed drafts.",
        )
    };
    bindings.locale_note.set_label(&locale_note);
    bindings
        .diagnostics
        .set_label(&state.localized_diagnostics_text(state.locale()));
    let translation_busy = matches!(
        state.status(),
        AppStatus::Translating | AppStatus::Cancelling
    );
    if translation_busy {
        bindings
            .workspace
            .update_state(&[gtk::accessible::State::Busy(true)]);
        bindings
            .output_view
            .update_state(&[gtk::accessible::State::Busy(true)]);
    } else {
        bindings.workspace.reset_state(gtk::AccessibleState::Busy);
        bindings.output_view.reset_state(gtk::AccessibleState::Busy);
    }
    let ocr_pending = bindings.ocr_pending.get();
    let blocked = state.pending_profile_deletion().is_some()
        || state.pending_model_selection().is_some()
        || ocr_pending
        || matches!(
            state.status(),
            AppStatus::Connecting | AppStatus::Translating | AppStatus::Cancelling
        );
    let provider_controls_enabled = state.worker_ready() && !blocked;
    let document_job_available = bindings.document_job_id.borrow().is_some();
    refresh_onboarding(bindings, state);
    refresh_active_provider_label(bindings, state);
    bindings
        .provider_name
        .set_sensitive(provider_controls_enabled);
    bindings
        .provider_preset
        .set_sensitive(provider_controls_enabled);
    bindings
        .provider_endpoint
        .set_sensitive(provider_controls_enabled);
    bindings
        .manual_model_row
        .set_visible(preset_requires_manual_model(
            bindings.provider_preset.selected(),
        ));
    bindings.manual_model.set_sensitive(
        provider_controls_enabled
            && preset_requires_manual_model(bindings.provider_preset.selected()),
    );
    bindings
        .provider_credential
        .set_sensitive(provider_controls_enabled);
    bindings
        .saved_profile
        .set_sensitive(provider_controls_enabled && state.profile_storage_available());
    bindings
        .remember_profile
        .set_sensitive(provider_controls_enabled && state.profile_storage_available());
    bindings.remove_saved_profile.set_sensitive(
        provider_controls_enabled
            && state.profile_storage_available()
            && state.selected_saved_profile_id().is_some(),
    );
    bindings
        .test_connection
        .set_sensitive(provider_controls_enabled);
    bindings.connect.set_sensitive(provider_controls_enabled);
    bindings
        .translate
        .set_sensitive(state.worker_ready() && !blocked && state.selected_model().is_some());
    bindings.retry_translation.set_sensitive(
        state.worker_ready()
            && !blocked
            && !document_job_available
            && state.selected_model().is_some()
            && state.can_retry_translation(),
    );
    let fallback_available = state.worker_ready()
        && !blocked
        && !document_job_available
        && !state.is_incognito()
        && state.profile_storage_available()
        && state
            .saved_profiles()
            .iter()
            .any(|profile| state.active_saved_profile_id() != Some(profile.id()));
    if !fallback_available && bindings.fallback_enabled.is_active() {
        bindings.fallback_enabled.set_active(false);
    }
    bindings.fallback_enabled.set_sensitive(fallback_available);
    bindings
        .fallback_profile
        .set_sensitive(fallback_available && bindings.fallback_enabled.is_active());
    bindings.pause_document.set_sensitive(
        document_job_available
            && state.worker_ready()
            && state.status() == AppStatus::Translating
            && document_state == Some(DocumentJobState::Running),
    );
    bindings.resume_document.set_sensitive(
        document_job_available
            && state.worker_ready()
            && !blocked
            && document_state == Some(DocumentJobState::Paused),
    );
    bindings.retry_document.set_sensitive(
        document_job_available
            && state.worker_ready()
            && !blocked
            && matches!(
                document_state,
                Some(DocumentJobState::Cancelled | DocumentJobState::Failed)
            ),
    );
    bindings
        .export_output
        .set_sensitive(!state.output().is_empty() && !blocked);
    bindings
        .open_output
        .set_sensitive(bindings.output_uri.borrow().is_some() && !blocked);
    bindings
        .open_source
        .set_sensitive(source_import_allowed(state) && !ocr_pending);
    bindings
        .ocr_enabled
        .set_sensitive(state.worker_ready() && !blocked && !document_job_available && !ocr_pending);
    bindings.document_jobs.set_sensitive(
        state.worker_ready() && !blocked && !ocr_pending && state.profile_storage_available(),
    );
    bindings.import_glossary.set_sensitive(!blocked);
    if bindings.incognito.is_active() != state.is_incognito() {
        bindings.incognito.set_active(state.is_incognito());
    }
    bindings.incognito.set_sensitive(!blocked);
    bindings.history_enabled.set_sensitive(
        state.worker_ready()
            && state.profile_storage_available()
            && !blocked
            && !bindings.history_policy_pending.get(),
    );
    bindings.clear_history.set_sensitive(
        state.profile_storage_available()
            && !blocked
            && !bindings.history_clear_pending.get()
            && state.translation_history_count() > 0,
    );
    bindings.history.set_sensitive(
        state.profile_storage_available() && !blocked && state.translation_history_count() > 0,
    );
    bindings.memory_enabled.set_sensitive(
        state.worker_ready()
            && state.profile_storage_available()
            && !blocked
            && !bindings.memory_policy_pending.get(),
    );
    bindings.clear_memory.set_sensitive(
        state.profile_storage_available()
            && !blocked
            && !bindings.memory_clear_pending.get()
            && state.translation_memory_count() > 0,
    );
    bindings.memory.set_sensitive(
        state.profile_storage_available() && !blocked && state.translation_memory_count() > 0,
    );
    bindings
        .routing_profiles
        .set_sensitive(state.profile_storage_available() && !blocked);
    bindings.export_glossary.set_sensitive(
        !blocked && (!bindings.glossary.text().trim().is_empty() || state.glossary().is_some()),
    );
    // Orca 烟测需要在空闲窗口中聚焦 Stop 控件，生产状态仍由工作阶段控制可用性。
    let orca_fixture = std::env::var_os("LINGUAMESH_TEST_ORCA_ATSPI").is_some();
    bindings.stop.set_sensitive(
        orca_fixture
            || (state.worker_ready()
                && matches!(
                    state.status(),
                    AppStatus::Connecting | AppStatus::Translating
                )),
    );
    bindings
        .model
        .set_sensitive(state.worker_ready() && !blocked && !state.models().is_empty());
    bindings.source_locale.set_sensitive(!blocked);
    bindings.target_locale.set_sensitive(!blocked);
    if bindings.quality_mode.selected() != quality_mode_selection(state.quality_mode()) {
        bindings
            .quality_mode
            .set_selected(quality_mode_selection(state.quality_mode()));
    }
    bindings.quality_mode.set_sensitive(!blocked);
    let preset_selection = translation_preset_selection(state.translation_preset());
    if bindings.translation_preset.selected() != preset_selection {
        bindings.translation_preset.set_selected(preset_selection);
    }
    bindings.translation_preset.set_sensitive(!blocked);
    bindings.glossary.set_sensitive(!blocked);
}

#[cfg(test)]
mod tests {
    use super::{
        ANTHROPIC_ADAPTER_TYPE, ANTHROPIC_PROVIDER_PRESET_ID, AZURE_ADAPTER_TYPE,
        AZURE_PROVIDER_PRESET_ID, AppState, AppStatus, CUSTOM_PROVIDER_PRESET_ID, CoreWorker,
        DEFAULT_ANTHROPIC_ENDPOINT, DEFAULT_ANTHROPIC_PROVIDER_NAME, DEFAULT_AZURE_ENDPOINT,
        DEFAULT_AZURE_PROVIDER_NAME, DEFAULT_GEMINI_ENDPOINT, DEFAULT_GEMINI_PROVIDER_NAME,
        DEFAULT_OLLAMA_ENDPOINT, DEFAULT_OLLAMA_PROVIDER_NAME, DEFAULT_PROVIDER_ENDPOINT,
        DEFAULT_PROVIDER_NAME, DEFAULT_RESPONSES_ENDPOINT, DEFAULT_RESPONSES_PROVIDER_NAME,
        ErrorKind, ExportWriteStrategy, GEMINI_ADAPTER_TYPE, GEMINI_PROVIDER_PRESET_ID,
        OLLAMA_ADAPTER_TYPE, OLLAMA_PROVIDER_PRESET_ID, OPENAI_ADAPTER_TYPE, OnboardingStage,
        ProviderProfileId, RESPONSES_ADAPTER_TYPE, RESPONSES_PROVIDER_PRESET_ID, RoutingCandidate,
        RoutingConstraintTextValues, RoutingDecisionSummary, RoutingProfile, RoutingProfileRecord,
        SecretRef, SecretRefNamespace, SecretValue, TranslationError, UiLocale, WorkerCommand,
        WorkerEvent, apply_worker_event, collision_safe_destination, collision_safe_output_path,
        connect_action_handlers, connect_selection_handlers, create_window,
        custom_provider_profile, destination_matches_source, document_format_label,
        document_translation_report, endpoint_matches_preset_default, export_write_strategy,
        fallback_confirmation_needed, generate_custom_provider_id, load_source_file,
        localized_document_job_state, localized_document_warnings, localized_provider_default_name,
        localized_template, normalized_candidate_ids_for_mode, output_metrics_label,
        preset_requires_manual_model, provider_preset_config, provider_preset_index,
        quality_mode_for_selection, quality_mode_selection, refresh_ui,
        routing_constraints_from_controls, routing_constraints_from_text_values,
        routing_identifier_list_from_text, routing_optional_limit_from_text,
        routing_preference_for_selection, routing_preference_selection,
        routing_profile_id_conflicts, show_document_jobs_dialog, show_fallback_approval_dialog,
        show_new_profile_in_form, show_routing_profiles_dialog,
        show_secret_storage_session_fallback, start_event_pump, text_metrics_label,
        translation_output_name, translation_preset_for_selection, translation_preset_selection,
        usage_label, valid_routing_profile_id, validate_provider_preset_catalog,
        write_new_file_async,
    };
    use adw::prelude::*;
    use gtk::glib;
    use linguamesh_document::{
        DocumentFormat, DocumentJob, DocumentJobState, DocumentWarning, DocumentWarningKind,
    };
    use linguamesh_domain::{
        RoutingConstraints, RoutingMode, RoutingPreference, TranslationPreset,
        TranslationQualityMode, UsageRecord,
    };
    use linguamesh_storage::{DocumentJobSnapshot, Storage};
    use linguamesh_testkit::FakeProviderServer;
    use std::cell::{Cell, RefCell};
    use std::fmt::Write as FmtWrite;
    use std::fs;
    use std::io::{Cursor, Read, Write};
    use std::net::TcpListener;
    use std::os::unix::fs::PermissionsExt;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};
    use tokio::runtime::Builder;
    use tokio::sync::oneshot;
    use zip::write::{SimpleFileOptions, ZipWriter};

    fn descendant_widgets(root: &gtk::Widget) -> Vec<gtk::Widget> {
        let mut descendants = Vec::new();
        let mut child = root.first_child();
        while let Some(widget) = child {
            descendants.push(widget.clone());
            descendants.extend(descendant_widgets(&widget));
            child = widget.next_sibling();
        }
        descendants
    }

    #[test]
    fn routing_profile_id_matches_core_identifier_bounds() {
        assert!(valid_routing_profile_id("linux-default"));
        assert!(valid_routing_profile_id("team.eu_01"));
        assert!(valid_routing_profile_id(&"a".repeat(128)));
        assert!(!valid_routing_profile_id(""));
        assert!(!valid_routing_profile_id("routing profile"));
        assert!(!valid_routing_profile_id("配置"));
        assert!(!valid_routing_profile_id(&"a".repeat(129)));
    }

    #[test]
    fn quality_mode_selection_round_trips_core_values() {
        for mode in [
            TranslationQualityMode::Fast,
            TranslationQualityMode::Balanced,
            TranslationQualityMode::Best,
        ] {
            assert_eq!(
                quality_mode_for_selection(quality_mode_selection(mode)),
                mode
            );
        }
        assert_eq!(
            quality_mode_for_selection(99),
            TranslationQualityMode::Balanced
        );
    }

    #[test]
    fn translation_preset_selection_round_trips_core_values() {
        for preset in [
            TranslationPreset::general(),
            TranslationPreset::technical(),
            TranslationPreset::marketing(),
        ] {
            assert_eq!(
                translation_preset_for_selection(translation_preset_selection(&preset)),
                preset
            );
        }
        assert_eq!(
            translation_preset_for_selection(99),
            TranslationPreset::general()
        );
    }

    #[test]
    fn new_routing_profile_id_cannot_replace_existing_record() {
        let existing_ids = vec!["linux-default".to_owned(), "team-eu".to_owned()];
        assert!(routing_profile_id_conflicts(
            &existing_ids,
            None,
            "linux-default"
        ));
        assert!(!routing_profile_id_conflicts(
            &existing_ids,
            Some("linux-default"),
            "linux-default"
        ));
        assert!(!routing_profile_id_conflicts(
            &existing_ids,
            None,
            "team-apac"
        ));
    }

    #[test]
    fn manual_routing_profile_keeps_only_the_first_candidate() {
        let candidates = vec![
            ProviderProfileId::parse("first").expect("first candidate"),
            ProviderProfileId::parse("second").expect("second candidate"),
        ];
        assert_eq!(
            normalized_candidate_ids_for_mode(RoutingMode::Manual, candidates.clone()),
            vec![candidates[0].clone()]
        );
        assert_eq!(
            normalized_candidate_ids_for_mode(RoutingMode::Ordered, candidates.clone()),
            candidates
        );
        assert_eq!(
            normalized_candidate_ids_for_mode(RoutingMode::Automatic, candidates.clone()),
            candidates
        );
    }

    #[test]
    fn fallback_requires_one_shot_user_confirmation() {
        assert!(fallback_confirmation_needed(true, false));
        assert!(!fallback_confirmation_needed(true, true));
        assert!(!fallback_confirmation_needed(false, false));
    }

    #[allow(clippy::too_many_lines)]
    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_fallback_approval_dialog_requires_an_explicit_one_shot_action() {
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.FallbackApprovalTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK test application");

        let state = Rc::new(RefCell::new(AppState::default()));
        let (window, bindings, theme, locale) = create_window(&application);
        let translate_count = Rc::new(std::cell::Cell::new(0_u32));
        let translate_count_handler = Rc::clone(&translate_count);
        bindings.translate.connect_clicked(move |_| {
            translate_count_handler.set(translate_count_handler.get().saturating_add(1));
        });
        let context = glib::MainContext::default();
        window.present();

        show_fallback_approval_dialog(&bindings, &state);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Allow approved fallback"))
        });
        let dialog = application
            .windows()
            .into_iter()
            .find(|candidate| candidate.title().as_deref() == Some("Allow approved fallback"))
            .expect("fallback approval dialog");
        assert!(dialog.is_modal());
        let widgets = descendant_widgets(dialog.upcast_ref::<gtk::Widget>());
        let message = widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Label>())
            .find(|label| {
                label
                    .label()
                    .contains("The approved fallback provider was selected")
            })
            .expect("fallback approval message");
        assert!(message.is_focusable());
        let buttons = widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .cloned()
            .collect::<Vec<_>>();
        let close = buttons
            .iter()
            .find(|button| {
                button
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Close")
            })
            .cloned()
            .expect("fallback close button");
        let approve = buttons
            .iter()
            .find(|button| {
                button
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Translate")
            })
            .cloned()
            .expect("fallback approve button");
        assert!(close.is_focusable());
        assert!(approve.is_focusable());
        assert!(!bindings.fallback_approval.get());
        close.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(1), || {
            !application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Allow approved fallback"))
        });
        assert_eq!(translate_count.get(), 0);
        assert!(!bindings.fallback_approval.get());

        show_fallback_approval_dialog(&bindings, &state);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Allow approved fallback"))
        });
        let dialog = application
            .windows()
            .into_iter()
            .find(|candidate| candidate.title().as_deref() == Some("Allow approved fallback"))
            .expect("second fallback approval dialog");
        let approve = descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .find(|button| {
                button
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Translate")
            })
            .cloned()
            .expect("second fallback approve button");
        approve.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(1), || {
            !application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Allow approved fallback"))
        });
        assert_eq!(translate_count.get(), 1);
        assert!(bindings.fallback_approval.get());
        assert!(!fallback_confirmation_needed(
            true,
            bindings.fallback_approval.get()
        ));

        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
    }

    // 验证 Secret Service 持久化失败时，用户必须明确选择仅会话恢复且关闭不会改写意图。
    #[allow(clippy::too_many_lines)]
    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_secret_storage_fallback_dialog_requires_explicit_session_only_action() {
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.SecretStorageFallbackTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK test application");

        let state = Rc::new(RefCell::new(AppState::default()));
        let (window, bindings, theme, locale) = create_window(&application);
        bindings.remember_profile.set_active(true);
        let context = glib::MainContext::default();
        window.present();

        show_secret_storage_session_fallback(&bindings, &state);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application.windows().iter().any(|candidate| {
                candidate.title().as_deref() == Some("Secure credential storage is unavailable.")
            })
        });
        let dialog = application
            .windows()
            .into_iter()
            .find(|candidate| {
                candidate.title().as_deref() == Some("Secure credential storage is unavailable.")
            })
            .expect("secure-storage fallback dialog");
        assert!(dialog.is_modal());
        let widgets = descendant_widgets(dialog.upcast_ref::<gtk::Widget>());
        let message = widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Label>())
            .find(|label| {
                label
                    .label()
                    .contains("Profile storage is unavailable; use session-only mode.")
            })
            .expect("session-only recovery message");
        assert!(message.is_focusable());
        let buttons = widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .cloned()
            .collect::<Vec<_>>();
        let session_only = buttons
            .iter()
            .find(|button| {
                button.label().as_deref().is_some_and(|label| {
                    label.trim_start_matches('_')
                        == "Profile storage is unavailable; use session-only mode."
                })
            })
            .cloned()
            .expect("session-only recovery button");
        let close = buttons
            .iter()
            .find(|button| {
                button
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Close")
            })
            .cloned()
            .expect("secure-storage close button");
        assert!(session_only.is_focusable());
        assert!(close.is_focusable());
        session_only.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(1), || {
            !application.windows().iter().any(|candidate| {
                candidate.title().as_deref() == Some("Secure credential storage is unavailable.")
            })
        });
        assert!(!bindings.remember_profile.is_active());
        assert!(bindings.provider_credential.is_focusable());

        bindings.remember_profile.set_active(true);
        show_secret_storage_session_fallback(&bindings, &state);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application.windows().iter().any(|candidate| {
                candidate.title().as_deref() == Some("Secure credential storage is unavailable.")
            })
        });
        let dialog = application
            .windows()
            .into_iter()
            .find(|candidate| {
                candidate.title().as_deref() == Some("Secure credential storage is unavailable.")
            })
            .expect("second secure-storage fallback dialog");
        let close = descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .find(|button| {
                button
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Close")
            })
            .cloned()
            .expect("second secure-storage close button");
        close.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(1), || {
            !application.windows().iter().any(|candidate| {
                candidate.title().as_deref() == Some("Secure credential storage is unavailable.")
            })
        });
        assert!(bindings.remember_profile.is_active());

        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
    }

    // 验证路由候选管理器的可访问控件、排序操作和编辑生命周期。
    #[allow(clippy::too_many_lines)]
    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_routing_profile_candidate_controls_have_accessible_lifecycle() {
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.RoutingProfilesTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK test application");

        let state = Rc::new(RefCell::new(AppState::default()));
        let database_directory = std::env::temp_dir().join(format!(
            "linguamesh-linux-routing-ui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&database_directory).expect("create routing UI database directory");
        fs::set_permissions(&database_directory, fs::Permissions::from_mode(0o700))
            .expect("protect routing UI database directory");
        let database_path = database_directory.join("state.sqlite3");
        let worker = Rc::new(CoreWorker::spawn_with_database(&database_path));
        let startup_deadline = Instant::now() + Duration::from_secs(5);
        let mut worker_ready = false;
        while Instant::now() < startup_deadline {
            match worker.try_recv() {
                Ok(WorkerEvent::DemoProviderReady { .. }) => {
                    worker_ready = true;
                    break;
                }
                Ok(_) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("routing UI worker event channel disconnected")
                }
            }
        }
        assert!(worker_ready, "routing UI worker did not become ready");
        let (window, bindings, theme, locale) = create_window(&application);
        let profile_a = custom_provider_profile(
            ProviderProfileId::parse("profile-a").expect("candidate A ID"),
            "Candidate A".to_owned(),
            CUSTOM_PROVIDER_PRESET_ID.to_owned(),
            OPENAI_ADAPTER_TYPE.to_owned(),
            "http://127.0.0.1:4242/v1/".to_owned(),
            None,
            Some("model-a".to_owned()),
        )
        .expect("candidate A profile");
        let profile_b = custom_provider_profile(
            ProviderProfileId::parse("profile-b").expect("candidate B ID"),
            "Candidate B".to_owned(),
            CUSTOM_PROVIDER_PRESET_ID.to_owned(),
            OPENAI_ADAPTER_TYPE.to_owned(),
            "http://127.0.0.1:4243/v1/".to_owned(),
            None,
            Some("model-b".to_owned()),
        )
        .expect("candidate B profile");
        state
            .borrow_mut()
            .restore_saved_profiles(vec![profile_b.clone(), profile_a.clone()], None)
            .expect("restore routing candidates");
        // 验证诊断面板展示经过脱敏的路由候选、拒绝原因、排名输入和回退顺序。
        state
            .borrow_mut()
            .record_routing_decision(RoutingDecisionSummary {
                profile_id: "linux-candidates".to_owned(),
                provider_id: "profile-a".to_owned(),
                model_id: "model-a".to_owned(),
                eligible_count: 2,
                rejected_count: 1,
                fallback_count: 1,
                eligible_candidates: vec!["profile-a@model-a".to_owned()],
                rejected_candidates: vec!["profile-c@model-c (RemoteDisallowed)".to_owned()],
                ranking_inputs: vec!["profile-a@model-a [1,0]".to_owned()],
                fallback_order: vec!["profile-b@model-b".to_owned()],
            });
        refresh_ui(&bindings, &state.borrow());
        assert!(bindings.diagnostics.label().contains("profile-a@model-a"));
        assert!(
            bindings
                .diagnostics
                .label()
                .contains("profile-c@model-c (RemoteDisallowed)")
        );
        assert!(bindings.diagnostics.label().contains("profile-b@model-b"));
        window.present();
        let context = glib::MainContext::default();
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("LinguaMesh"))
        });

        let routing_profile = RoutingProfile::new(
            "linux-candidates",
            RoutingMode::Ordered,
            vec![
                RoutingCandidate::new("profile-a", "model-a", true, 64 * 1024)
                    .expect("candidate A"),
                RoutingCandidate::new("profile-b", "model-b", true, 64 * 1024)
                    .expect("candidate B"),
            ],
            RoutingConstraints::default(),
        )
        .expect("routing profile");
        show_routing_profiles_dialog(
            &bindings,
            &state,
            &worker.command_handle(),
            vec![RoutingProfileRecord {
                id: "linux-candidates".to_owned(),
                profile: routing_profile,
                created_at: 0,
                updated_at: 0,
            }],
        );
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
        });
        let dialog = application
            .windows()
            .into_iter()
            .find(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
            .expect("routing profile dialog");
        assert!(dialog.is_modal());
        let widgets = descendant_widgets(dialog.upcast_ref::<gtk::Widget>());

        let profile_id = widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Entry>())
            .find(|entry| entry.text() == "linux-default")
            .cloned()
            .expect("routing profile ID entry");
        assert_labeled_control(profile_id.upcast_ref::<gtk::Widget>());

        let mode = widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::DropDown>())
            .find(|drop_down| {
                drop_down
                    .model()
                    .and_then(|model| model.downcast::<gtk::StringList>().ok())
                    .is_some_and(|model| {
                        model.n_items() == 3
                            && model.string(0).as_deref() == Some("Manual")
                            && model.string(1).as_deref() == Some("Ordered")
                            && model.string(2).as_deref() == Some("Automatic")
                    })
            })
            .cloned()
            .expect("routing mode control");
        assert!(mode.is_focusable());
        assert_eq!(mode.selected(), 2);

        let fallback = widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::CheckButton>())
            .find(|check| {
                check
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Allow approved fallback")
            })
            .cloned()
            .expect("fallback routing control");
        assert!(fallback.is_focusable());
        assert!(!fallback.is_active());

        let candidate_labels = || {
            descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
                .iter()
                .filter_map(|widget| widget.downcast_ref::<gtk::CheckButton>())
                .filter_map(gtk::prelude::CheckButtonExt::label)
                .map(|label| label.to_string())
                .filter(|label| label.starts_with("Candidate "))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            candidate_labels(),
            vec!["Candidate A · model-a", "Candidate B · model-b"]
        );
        for candidate in descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::CheckButton>())
            .filter(|check| {
                check
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.starts_with("Candidate "))
            })
        {
            assert!(candidate.is_focusable());
            assert!(candidate.is_active());
        }
        let movement_buttons = descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .filter_map(gtk::prelude::WidgetExt::tooltip_text)
            .collect::<Vec<_>>();
        assert!(
            movement_buttons
                .iter()
                .any(|label| label == "Move candidate up")
        );
        assert!(
            movement_buttons
                .iter()
                .any(|label| label == "Move candidate down")
        );
        assert!(
            descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
                .iter()
                .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
                .filter(|button| {
                    button.tooltip_text().is_some_and(|label| {
                        label == "Move candidate up" || label == "Move candidate down"
                    })
                })
                .all(|button| {
                    button.is_focusable()
                        && gtk::test_accessible_has_property(button, gtk::AccessibleProperty::Label)
                })
        );

        let candidate_a_row = descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Box>())
            .find(|row| {
                row.first_child()
                    .and_then(|child| child.downcast::<gtk::CheckButton>().ok())
                    .and_then(|check| check.label())
                    .is_some_and(|label| label == "Candidate A · model-a")
            })
            .cloned()
            .expect("candidate A row");
        let candidate_a_down = candidate_a_row
            .first_child()
            .and_then(|child| child.next_sibling())
            .and_then(|child| child.next_sibling())
            .and_then(|child| child.downcast::<gtk::Button>().ok())
            .expect("candidate A down button");
        candidate_a_down.emit_clicked();
        assert_eq!(
            candidate_labels(),
            vec!["Candidate B · model-b", "Candidate A · model-a"]
        );
        mode.set_selected(0);
        assert!(
            descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
                .iter()
                .filter_map(|widget| widget.downcast_ref::<gtk::CheckButton>())
                .find(|check| {
                    check
                        .label()
                        .as_deref()
                        .is_some_and(|label| label == "Candidate B · model-b")
                })
                .is_some_and(gtk::prelude::CheckButtonExt::is_active)
        );
        assert!(
            !descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
                .iter()
                .filter_map(|widget| widget.downcast_ref::<gtk::CheckButton>())
                .find(|check| {
                    check
                        .label()
                        .as_deref()
                        .is_some_and(|label| label == "Candidate A · model-a")
                })
                .is_some_and(gtk::prelude::CheckButtonExt::is_active)
        );

        let edit_button = descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .find(|button| {
                button
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Edit")
            })
            .cloned()
            .expect("edit routing profile button");
        edit_button.emit_clicked();
        assert!(!profile_id.is_editable());
        let save_button = descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .find(|button| {
                button
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Save routing profile")
            })
            .cloned()
            .expect("save routing profile button");
        let candidate_b = descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::CheckButton>())
            .find(|check| {
                check
                    .label()
                    .as_deref()
                    .is_some_and(|label| label == "Candidate B · model-b")
            })
            .cloned()
            .expect("candidate B checkbox after edit");
        candidate_b.set_active(false);
        save_button.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(1), || {
            !application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
        });
        let save_deadline = Instant::now() + Duration::from_secs(5);
        let saved_profile = loop {
            assert!(
                Instant::now() < save_deadline,
                "routing profile save timed out"
            );
            match worker.try_recv() {
                Ok(WorkerEvent::RoutingProfileSaved(record)) => break record,
                Ok(WorkerEvent::RoutingProfileActionRejected(error)) => {
                    panic!("routing profile save rejected: {error}")
                }
                Ok(_) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("routing UI worker event channel disconnected after save")
                }
            }
        };
        assert_eq!(saved_profile.id, "linux-candidates");
        assert_eq!(saved_profile.profile.mode, RoutingMode::Ordered);
        assert_eq!(saved_profile.profile.candidates.len(), 1);
        assert_eq!(saved_profile.profile.candidates[0].provider_id, "profile-a");
        worker
            .try_send(WorkerCommand::ListRoutingProfiles)
            .expect("list saved routing profile");
        let listed_deadline = Instant::now() + Duration::from_secs(5);
        let listed_profile = loop {
            assert!(
                Instant::now() < listed_deadline,
                "routing profile reload timed out"
            );
            match worker.try_recv() {
                Ok(WorkerEvent::RoutingProfilesListed { profiles }) => {
                    break profiles
                        .into_iter()
                        .find(|record| record.id == "linux-candidates")
                        .expect("saved routing profile after reload");
                }
                Ok(WorkerEvent::RoutingProfileActionRejected(error)) => {
                    panic!("routing profile reload rejected: {error}")
                }
                Ok(_) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("routing UI worker event channel disconnected during reload")
                }
            }
        };
        assert_eq!(listed_profile.profile.candidates.len(), 1);
        assert_eq!(
            listed_profile.profile.candidates[0].provider_id,
            "profile-a"
        );
        show_routing_profiles_dialog(
            &bindings,
            &state,
            &worker.command_handle(),
            vec![listed_profile.clone()],
        );
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
        });
        let reloaded_dialog = application
            .windows()
            .into_iter()
            .find(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
            .expect("reloaded routing profile dialog");
        let reload_edit = descendant_widgets(reloaded_dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .find(|button| {
                button
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Edit")
            })
            .cloned()
            .expect("reloaded edit routing profile button");
        reload_edit.emit_clicked();
        let reloaded_candidates = descendant_widgets(reloaded_dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::CheckButton>())
            .filter(|check| {
                check
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.starts_with("Candidate "))
            })
            .cloned()
            .collect::<Vec<_>>();
        assert!(reloaded_candidates.iter().any(|check| {
            check.label().as_deref() == Some("Candidate A · model-a") && check.is_active()
        }));
        assert!(reloaded_candidates.iter().any(|check| {
            check.label().as_deref() == Some("Candidate B · model-b") && !check.is_active()
        }));
        reloaded_dialog.close();

        let use_button = descendant_widgets(dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .find(|button| {
                button
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Use")
            })
            .cloned()
            .expect("use routing profile button");
        assert!(use_button.is_focusable());
        use_button.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(1), || {
            !application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
        });
        assert_eq!(
            bindings.selected_routing_profile_id.borrow().as_deref(),
            Some("linux-candidates")
        );

        show_routing_profiles_dialog(
            &bindings,
            &state,
            &worker.command_handle(),
            vec![listed_profile],
        );
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
        });
        let delete_dialog = application
            .windows()
            .into_iter()
            .find(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
            .expect("routing profile delete dialog");
        let delete_button = descendant_widgets(delete_dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .find(|button| {
                button
                    .label()
                    .as_deref()
                    .is_some_and(|label| label.trim_start_matches('_') == "Delete")
            })
            .cloned()
            .expect("delete routing profile button");
        assert!(delete_button.is_focusable());
        delete_button.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(1), || {
            !application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
        });
        let delete_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                Instant::now() < delete_deadline,
                "routing profile delete timed out"
            );
            match worker.try_recv() {
                Ok(WorkerEvent::RoutingProfileDeleted { profile_id }) => {
                    assert_eq!(profile_id, "linux-candidates");
                    apply_worker_event(
                        &bindings,
                        &state,
                        worker.as_ref(),
                        WorkerEvent::RoutingProfileDeleted { profile_id },
                    );
                    break;
                }
                Ok(WorkerEvent::RoutingProfileActionRejected(error)) => {
                    panic!("routing profile delete rejected: {error}")
                }
                Ok(_) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("routing UI worker event channel disconnected after delete")
                }
            }
        }
        assert!(bindings.selected_routing_profile_id.borrow().is_none());
        let empty_deadline = Instant::now() + Duration::from_secs(5);
        loop {
            assert!(
                Instant::now() < empty_deadline,
                "routing profile empty-list reload timed out"
            );
            match worker.try_recv() {
                Ok(WorkerEvent::RoutingProfilesListed { profiles }) => {
                    assert!(profiles.is_empty());
                    break;
                }
                Ok(WorkerEvent::RoutingProfileActionRejected(error)) => {
                    panic!("routing profile empty-list reload rejected: {error}")
                }
                Ok(_) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    panic!("routing UI worker event channel disconnected after delete reload")
                }
            }
        }

        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
        std::thread::sleep(Duration::from_millis(100));
        fs::remove_dir_all(&database_directory).expect("remove routing UI database directory");
    }

    #[test]
    fn text_metrics_report_characters_and_approximate_tokens() {
        assert_eq!(
            text_metrics_label(UiLocale::English, "abcd"),
            "Characters: 4 · Estimated tokens: 1"
        );
        assert_eq!(
            text_metrics_label(UiLocale::English, "中a"),
            "Characters: 2 · Estimated tokens: 1"
        );
    }

    #[test]
    fn usage_label_preserves_source_and_unknown_boundary() {
        let estimated = UsageRecord::locally_estimated("abcd", "你好");
        assert_eq!(
            usage_label(UiLocale::English, Some(&estimated)).as_deref(),
            Some("Usage: 3 tokens (locally estimated)")
        );
        let unknown = UsageRecord::unknown();
        assert_eq!(
            output_metrics_label(UiLocale::English, "abcd", Some(&unknown)),
            "Characters: 4 · Estimated tokens: 1\nUsage: unavailable (unknown)"
        );
    }

    #[test]
    fn routing_preference_selection_round_trips_core_values() {
        for preference in [
            RoutingPreference::None,
            RoutingPreference::Local,
            RoutingPreference::Quality,
            RoutingPreference::Latency,
            RoutingPreference::Cost,
        ] {
            assert_eq!(
                routing_preference_for_selection(routing_preference_selection(preference)),
                preference
            );
        }
        assert_eq!(
            routing_preference_for_selection(99),
            RoutingPreference::None
        );
    }

    #[test]
    fn routing_editor_constraints_preserve_hidden_core_fields() {
        let existing = RoutingConstraints {
            provider_allowlist: vec!["local-loopback".to_owned()],
            minimum_quality_tier: Some(2),
            ..RoutingConstraints::default()
        };
        let updated = routing_constraints_from_controls(
            Some(&existing),
            &RoutingConstraints {
                preference: RoutingPreference::Quality,
                local_only: true,
                allow_remote: false,
                privacy_sensitive: true,
                require_streaming: true,
                require_document: false,
                explicit_fallback_allowed: true,
                ..RoutingConstraints::default()
            },
        );
        assert_eq!(updated.provider_allowlist, existing.provider_allowlist);
        assert_eq!(updated.minimum_quality_tier, Some(2));
        assert_eq!(updated.preference, RoutingPreference::Quality);
        assert!(updated.local_only);
        assert!(!updated.allow_remote);
        assert!(updated.privacy_sensitive);
        assert!(updated.require_streaming);
        assert!(!updated.require_document);
        assert!(updated.explicit_fallback_allowed);
    }

    #[test]
    fn routing_constraint_text_parsers_reject_unsafe_values() {
        assert_eq!(
            routing_identifier_list_from_text("provider-a, model-b"),
            Ok(vec!["provider-a".to_owned(), "model-b".to_owned()])
        );
        assert_eq!(routing_identifier_list_from_text(""), Ok(Vec::new()));
        assert!(routing_identifier_list_from_text("provider-a,,model-b").is_err());
        assert!(routing_identifier_list_from_text("provider/a").is_err());
        assert_eq!(routing_optional_limit_from_text(""), Ok(None));
        assert_eq!(routing_optional_limit_from_text("4096"), Ok(Some(4096)));
        assert!(routing_optional_limit_from_text("0").is_err());
        assert!(routing_optional_limit_from_text("not-a-number").is_err());
    }

    #[test]
    fn routing_constraint_text_values_update_visible_core_fields() {
        let existing = RoutingConstraints {
            provider_denylist: vec!["legacy-provider".to_owned()],
            ..RoutingConstraints::default()
        };
        let updated = routing_constraints_from_text_values(
            Some(&existing),
            &RoutingConstraints {
                preference: RoutingPreference::Quality,
                ..RoutingConstraints::default()
            },
            RoutingConstraintTextValues {
                provider_allowlist: "local-loopback, team-eu",
                provider_denylist: "blocked-provider",
                model_allowlist: "qwen-2, llama-3",
                model_denylist: "unsafe-model",
                minimum_quality_tier: "2",
                max_request_bytes: "65536",
            },
        )
        .expect("routing text values");
        assert_eq!(
            updated.provider_allowlist,
            vec!["local-loopback", "team-eu"]
        );
        assert_eq!(updated.provider_denylist, vec!["blocked-provider"]);
        assert_eq!(updated.model_allowlist, vec!["qwen-2", "llama-3"]);
        assert_eq!(updated.model_denylist, vec!["unsafe-model"]);
        assert_eq!(updated.minimum_quality_tier, Some(2));
        assert_eq!(updated.max_request_bytes, Some(65536));
        assert_eq!(updated.preference, RoutingPreference::Quality);
    }

    #[test]
    fn output_export_rejects_same_file_aliases() {
        let directory =
            std::env::temp_dir().join(format!("linguamesh-linux-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("directory");
        let source_path = directory.join("source.txt");
        fs::write(&source_path, "source").expect("source");
        let source_file = gtk::gio::File::for_path(&source_path);
        let source_uri = source_file.uri().to_string();

        assert!(destination_matches_source(
            Some(source_uri.as_str()),
            &gtk::gio::File::for_path(&source_path),
        ));

        let different_path = directory.join("different.txt");
        assert!(!destination_matches_source(
            Some(source_uri.as_str()),
            &gtk::gio::File::for_path(&different_path),
        ));

        #[cfg(unix)]
        {
            let symlink_path = directory.join("source-link.txt");
            std::os::unix::fs::symlink(&source_path, &symlink_path).expect("symlink");
            assert!(destination_matches_source(
                Some(source_uri.as_str()),
                &gtk::gio::File::for_path(&symlink_path),
            ));

            let hard_link_path = directory.join("source-hard-link.txt");
            fs::hard_link(&source_path, &hard_link_path).expect("hard link");
            assert!(destination_matches_source(
                Some(source_uri.as_str()),
                &gtk::gio::File::for_path(&hard_link_path),
            ));
        }
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn translation_output_name_uses_source_stem_and_target_locale() {
        assert_eq!(
            translation_output_name("guide.v1.md", "zh-CN"),
            "guide.v1.zh-CN.md"
        );
        assert_eq!(
            translation_output_name("nested\\unsafe\tname", "ar_XB"),
            "nested_unsafe_name.ar_XB.txt"
        );
        assert_eq!(translation_output_name(".txt", ""), "translation.und.txt");
    }

    #[test]
    fn collision_safe_output_path_adds_stable_suffix_without_overwriting() {
        let directory =
            std::env::temp_dir().join(format!("linguamesh-linux-collision-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("directory");
        let destination = directory.join("guide.zh-CN.md");
        fs::write(&destination, "existing").expect("destination");
        fs::write(directory.join("guide.zh-CN-1.md"), "existing").expect("first collision");
        assert_eq!(
            collision_safe_output_path(&destination),
            Some(directory.join("guide.zh-CN-2.md"))
        );
        assert_eq!(
            collision_safe_output_path(&directory.join("new.txt")),
            Some(directory.join("new.txt"))
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn non_local_export_uses_exclusive_create_fallback() {
        let destination = gtk::gio::File::for_uri("smb://server/share/output.txt");
        assert_eq!(
            export_write_strategy(&destination),
            ExportWriteStrategy::ExclusiveCreate
        );
        assert!(destination.path().is_none());
        let selected = collision_safe_destination(&destination).expect("non-local destination");
        assert_eq!(selected.uri().as_str(), "smb://server/share/output.txt");
    }

    #[test]
    fn non_local_source_alias_is_rejected_by_uri_identity() {
        let source_uri = "smb://server/share/source.txt";
        let source = gtk::gio::File::for_uri(source_uri);
        assert!(destination_matches_source(Some(source_uri), &source));
        assert!(!destination_matches_source(
            Some(source_uri),
            &gtk::gio::File::for_uri("smb://server/share/translated.txt")
        ));
    }

    // 验证临时文件原子完成在目标被占用时失败，并保留原有文件内容。
    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_atomic_output_writer_never_replaces_existing_file() {
        adw::init().expect("initialize GTK output writer fixture");
        let directory = std::env::temp_dir().join(format!(
            "linguamesh-linux-exclusive-output-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("directory");
        let destination_path = directory.join("existing.txt");
        fs::write(&destination_path, b"keep").expect("existing output");
        let destination = gtk::gio::File::for_path(&destination_path);
        let context = glib::MainContext::default();
        let result = Rc::new(Cell::new(None));
        let callback_result = Rc::clone(&result);
        write_new_file_async(&destination, b"replace".to_vec(), move |succeeded| {
            callback_result.set(Some(succeeded));
        });
        spin_main_context_until(&context, Duration::from_secs(2), || result.get().is_some());
        assert_eq!(result.get(), Some(false));
        assert_eq!(
            fs::read(&destination_path).expect("read existing output"),
            b"keep"
        );
        assert_eq!(
            fs::read_dir(&directory)
                .expect("read output directory")
                .count(),
            1
        );
        let new_destination_path = directory.join("new.txt");
        let new_destination = gtk::gio::File::for_path(&new_destination_path);
        let new_result = Rc::new(Cell::new(None));
        let new_callback_result = Rc::clone(&new_result);
        write_new_file_async(&new_destination, b"created".to_vec(), move |succeeded| {
            new_callback_result.set(Some(succeeded));
        });
        spin_main_context_until(&context, Duration::from_secs(2), || {
            new_result.get().is_some()
        });
        assert_eq!(new_result.get(), Some(true));
        assert_eq!(
            fs::read(&new_destination_path).expect("read new output"),
            b"created"
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn document_translation_report_is_redacted_and_counts_segments() {
        let mut job = DocumentJob::from_text(
            "guide\tprivate.txt",
            DocumentFormat::Txt,
            "translated\npending",
        );
        job.segments[0].translated_text = Some("traduction".to_owned());
        let snapshot = DocumentJobSnapshot {
            job,
            job_id: "report-job".to_owned(),
            state: DocumentJobState::Completed,
            options: None,
            created_at: 100,
            updated_at: 200,
        };
        let report = document_translation_report(&snapshot, "0.1.0-alpha.2");
        assert!(report.contains("source_identifier\tguide private.txt"));
        assert!(report.contains("segment_total\t2"));
        assert!(report.contains("completed_count\t1"));
        assert!(report.contains("pending_count\t1"));
        assert!(report.contains("output_identifier\tguide_private.und.txt"));
        assert!(report.contains("core_version\t0.1.0-alpha.2"));
        assert!(report.contains("retried_count\tunknown"));
        assert!(report.contains(
            "usage\t{\"source\":\"locally_estimated\",\"input_tokens\":5,\"output_tokens\":3,\"total_tokens\":8}"
        ));
        assert!(!report.contains("traduction"));
        assert!(!report.contains("\ttranslated\n"));
        assert!(!report.contains("\tpending\n"));
        assert!(!report.contains("private\n"));
    }

    #[test]
    fn document_job_metadata_uses_stable_format_and_localized_state_labels() {
        let formats = [
            (DocumentFormat::Txt, "TXT"),
            (DocumentFormat::Markdown, "Markdown"),
            (DocumentFormat::Srt, "SRT"),
            (DocumentFormat::WebVtt, "WebVTT"),
            (DocumentFormat::Csv, "CSV"),
            (DocumentFormat::Html, "HTML"),
            (DocumentFormat::Json, "JSON"),
            (DocumentFormat::Docx, "DOCX"),
            (DocumentFormat::Pptx, "PPTX"),
            (DocumentFormat::Xlsx, "XLSX"),
            (DocumentFormat::Epub, "EPUB"),
            (DocumentFormat::Pdf, "PDF"),
        ];
        for (format, expected) in formats {
            assert_eq!(document_format_label(format), expected);
        }

        let states = [
            (DocumentJobState::Pending, "Pending"),
            (DocumentJobState::Running, "Running"),
            (DocumentJobState::Paused, "Paused"),
            (DocumentJobState::Completed, "Completed"),
            (DocumentJobState::Cancelled, "Cancelled"),
            (DocumentJobState::Failed, "Failed"),
        ];
        for (state, expected) in states {
            assert_eq!(
                localized_document_job_state(UiLocale::English, state),
                expected
            );
        }
    }

    #[test]
    fn pdf_warning_text_names_limited_pages_without_source_content() {
        let warnings = vec![
            DocumentWarning {
                kind: DocumentWarningKind::PdfReconstructionLimited,
                page: None,
                cue: None,
            },
            DocumentWarning {
                kind: DocumentWarningKind::PdfImageOnlyPage,
                page: Some(2),
                cue: None,
            },
            DocumentWarning {
                kind: DocumentWarningKind::PdfImageOnlyPage,
                page: Some(1),
                cue: None,
            },
        ];
        let text = localized_document_warnings(UiLocale::English, &warnings);
        assert!(text.contains("page association is preserved"));
        assert!(text.contains("page(s) 1, 2"));
        assert!(!text.contains("source"));
    }

    #[test]
    fn subtitle_warning_text_names_cues_without_source_content() {
        let warnings = vec![
            DocumentWarning {
                kind: DocumentWarningKind::SubtitleLineLengthExceeded,
                page: None,
                cue: Some(2),
            },
            DocumentWarning {
                kind: DocumentWarningKind::SubtitleLineLengthExceeded,
                page: None,
                cue: Some(1),
            },
            DocumentWarning {
                kind: DocumentWarningKind::SubtitleReadingSpeedHigh,
                page: None,
                cue: Some(1),
            },
        ];
        let text = localized_document_warnings(UiLocale::English, &warnings);
        assert!(text.contains("cue(s) 1, 2"));
        assert!(text.contains("42-character line guidance"));
        assert!(text.contains("17-character-per-second"));
        assert!(!text.contains("source"));
    }

    #[test]
    fn provider_presets_map_to_stable_native_and_compatible_defaults() {
        assert!(validate_provider_preset_catalog().is_ok());
        assert_eq!(provider_preset_index(OLLAMA_PROVIDER_PRESET_ID), 1);
        assert_eq!(provider_preset_index(ANTHROPIC_PROVIDER_PRESET_ID), 2);
        assert_eq!(provider_preset_index(GEMINI_PROVIDER_PRESET_ID), 3);
        assert_eq!(provider_preset_index(AZURE_PROVIDER_PRESET_ID), 4);
        assert_eq!(provider_preset_index(RESPONSES_PROVIDER_PRESET_ID), 5);
        assert_eq!(provider_preset_index(CUSTOM_PROVIDER_PRESET_ID), 0);
        assert_eq!(
            provider_preset_config(0),
            (
                CUSTOM_PROVIDER_PRESET_ID,
                OPENAI_ADAPTER_TYPE,
                DEFAULT_PROVIDER_NAME,
                DEFAULT_PROVIDER_ENDPOINT,
            )
        );
        assert_eq!(
            provider_preset_config(1),
            (
                OLLAMA_PROVIDER_PRESET_ID,
                OLLAMA_ADAPTER_TYPE,
                DEFAULT_OLLAMA_PROVIDER_NAME,
                DEFAULT_OLLAMA_ENDPOINT,
            )
        );
        assert_eq!(
            provider_preset_config(2),
            (
                ANTHROPIC_PROVIDER_PRESET_ID,
                ANTHROPIC_ADAPTER_TYPE,
                DEFAULT_ANTHROPIC_PROVIDER_NAME,
                DEFAULT_ANTHROPIC_ENDPOINT,
            )
        );
        assert_eq!(
            provider_preset_config(3),
            (
                GEMINI_PROVIDER_PRESET_ID,
                GEMINI_ADAPTER_TYPE,
                DEFAULT_GEMINI_PROVIDER_NAME,
                DEFAULT_GEMINI_ENDPOINT,
            )
        );
        assert_eq!(
            provider_preset_config(4),
            (
                AZURE_PROVIDER_PRESET_ID,
                AZURE_ADAPTER_TYPE,
                DEFAULT_AZURE_PROVIDER_NAME,
                DEFAULT_AZURE_ENDPOINT,
            )
        );
        assert_eq!(
            provider_preset_config(5),
            (
                RESPONSES_PROVIDER_PRESET_ID,
                RESPONSES_ADAPTER_TYPE,
                DEFAULT_RESPONSES_PROVIDER_NAME,
                DEFAULT_RESPONSES_ENDPOINT,
            )
        );
        assert!(endpoint_matches_preset_default(
            DEFAULT_PROVIDER_ENDPOINT,
            0
        ));
        assert!(endpoint_matches_preset_default(DEFAULT_OLLAMA_ENDPOINT, 1));
        assert!(endpoint_matches_preset_default(
            DEFAULT_ANTHROPIC_ENDPOINT,
            2
        ));
        assert!(endpoint_matches_preset_default(DEFAULT_GEMINI_ENDPOINT, 3));
        assert!(endpoint_matches_preset_default(DEFAULT_AZURE_ENDPOINT, 4));
        assert!(endpoint_matches_preset_default(
            DEFAULT_RESPONSES_ENDPOINT,
            5
        ));
        assert!(preset_requires_manual_model(2));
        assert!(preset_requires_manual_model(4));
        assert!(!preset_requires_manual_model(0));
        assert!(!preset_requires_manual_model(5));
        assert!(!endpoint_matches_preset_default(
            "https://api.example.test/v1/",
            0
        ));
        assert!(!endpoint_matches_preset_default(
            "https://api.example.test/api/",
            1
        ));
        assert!(!endpoint_matches_preset_default(
            "https://api.example.test/v1/",
            2
        ));
        assert!(!endpoint_matches_preset_default(
            "https://api.example.test/v1beta/",
            3
        ));
    }

    #[test]
    fn built_in_provider_default_names_follow_the_active_locale() {
        assert_eq!(
            localized_provider_default_name(UiLocale::English, 0),
            DEFAULT_PROVIDER_NAME
        );
        assert_eq!(
            localized_provider_default_name(UiLocale::English, 1),
            DEFAULT_OLLAMA_PROVIDER_NAME
        );
        assert_eq!(
            localized_provider_default_name(UiLocale::English, 2),
            DEFAULT_ANTHROPIC_PROVIDER_NAME
        );
        assert_eq!(
            localized_provider_default_name(UiLocale::English, 3),
            DEFAULT_GEMINI_PROVIDER_NAME
        );
        assert_eq!(
            localized_provider_default_name(UiLocale::SimplifiedChinese, 0),
            "本地 OpenAI 兼容提供商"
        );
        assert_eq!(
            localized_provider_default_name(UiLocale::SimplifiedChinese, 1),
            "本地 Ollama 提供商"
        );
    }

    fn spin_main_context_until(
        context: &glib::MainContext,
        timeout: Duration,
        mut condition: impl FnMut() -> bool,
    ) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            while context.pending() {
                context.iteration(false);
            }
            if condition() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("Timed out while waiting for the GTK state transition.");
    }

    // 构造带有恶意归档条目的最小 DOCX，复用生产导入路径验证失败闭环。
    fn malicious_docx_fixture(entry_name: &str, payload: &[u8], compressed: bool) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = if compressed {
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
        } else {
            SimpleFileOptions::default()
        };
        writer
            .start_file("[Content_Types].xml", options)
            .expect("content types");
        writer.write_all(b"<Types/>").expect("content types bytes");
        writer
            .start_file(entry_name, options)
            .expect("malicious entry");
        writer.write_all(payload).expect("malicious entry bytes");
        writer
            .start_file("word/document.xml", options)
            .expect("document");
        writer
            .write_all(
                br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>Safe</w:t></w:r></w:p></w:body></w:document>"#,
            )
            .expect("document bytes");
        writer
            .finish()
            .expect("malicious DOCX archive")
            .into_inner()
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        let body_start = loop {
            let read = stream.read(&mut chunk).expect("HTTP request headers");
            assert!(read > 0);
            request.extend_from_slice(&chunk[..read]);
            if let Some(position) = request.windows(4).position(|window| window == b"\r\n\r\n") {
                break position + 4;
            }
        };
        let content_length = String::from_utf8_lossy(&request[..body_start])
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .unwrap_or(0);
        while request.len() - body_start < content_length {
            let read = stream.read(&mut chunk).expect("HTTP request body");
            assert!(read > 0);
            request.extend_from_slice(&chunk[..read]);
        }
        request
    }

    fn write_http_response(stream: &mut std::net::TcpStream, content_type: &str, body: &[u8]) {
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream
            .write_all(header.as_bytes())
            .expect("HTTP response headers");
        stream.write_all(body).expect("HTTP response body");
    }

    struct ExternalFakeProvider {
        endpoint: String,
        chat_requests: Arc<AtomicUsize>,
        shutdown: Option<oneshot::Sender<()>>,
        thread: Option<JoinHandle<()>>,
    }

    impl ExternalFakeProvider {
        fn start(expected_secret: &'static str) -> Self {
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            let (shutdown, shutdown_receiver) = oneshot::channel();
            let thread = std::thread::spawn(move || {
                let runtime = Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("external provider runtime");
                runtime.block_on(async move {
                    let server = FakeProviderServer::start_requiring_bearer_token(
                        SecretValue::new(expected_secret),
                    )
                    .await
                    .expect("external provider");
                    let chat_requests = server.chat_request_counter();
                    ready_sender
                        .send((server.base_url(), chat_requests))
                        .expect("provider endpoint");
                    let _ = shutdown_receiver.await;
                    server.shutdown().await;
                });
            });
            let (endpoint, chat_requests) = ready_receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("provider startup");
            Self {
                endpoint,
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

    struct NativeOllamaFakeProvider {
        endpoint: String,
        shutdown: Option<oneshot::Sender<()>>,
        thread: Option<JoinHandle<()>>,
    }

    impl NativeOllamaFakeProvider {
        fn start() -> Self {
            let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
            let (shutdown, shutdown_receiver) = oneshot::channel();
            let thread = std::thread::spawn(move || {
                let runtime = Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("native Ollama provider runtime");
                runtime.block_on(async move {
                    let server = FakeProviderServer::start_ollama_native()
                        .await
                        .expect("native Ollama provider");
                    ready_sender
                        .send(server.ollama_base_url())
                        .expect("native Ollama endpoint");
                    let _ = shutdown_receiver.await;
                    server.shutdown().await;
                });
            });
            let endpoint = ready_receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("native Ollama startup");
            Self {
                endpoint,
                shutdown: Some(shutdown),
                thread: Some(thread),
            }
        }
    }

    impl Drop for NativeOllamaFakeProvider {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
            if let Some(thread) = self.thread.take() {
                thread.join().expect("native Ollama shutdown");
            }
        }
    }

    fn assert_labeled_control(control: &gtk::Widget) {
        assert!(gtk::test_accessible_has_relation(
            control,
            gtk::AccessibleRelation::LabelledBy
        ));
        assert!(control.is_focusable());
        let label = control
            .parent()
            .and_then(|parent| parent.first_child())
            .and_then(|child| child.downcast::<gtk::Label>().ok())
            .expect("control label");
        assert_eq!(label.mnemonic_widget().as_ref(), Some(control));
    }

    fn run_gtk_native_ollama_preset_flow(application: &adw::Application) {
        let external = NativeOllamaFakeProvider::start();
        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn());
        let (window, bindings, theme, locale) = create_window(application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        let context = glib::MainContext::default();
        window.present();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
        });

        bindings.provider_preset.set_selected(1);
        bindings.provider_name.set_text("Native Ollama provider");
        bindings.provider_endpoint.set_text(&external.endpoint);
        bindings.connect.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Ready
                && state.borrow().active_provider().is_some()
                && !state.borrow().models().is_empty()
        });
        bindings.model.set_selected(1);
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().selected_model().is_some()
        });
        let active = state
            .borrow()
            .active_provider()
            .cloned()
            .expect("active provider");
        assert_eq!(active.preset_id(), OLLAMA_PROVIDER_PRESET_ID);
        assert_eq!(active.adapter_type(), OLLAMA_ADAPTER_TYPE);
        assert_eq!(state.borrow().selected_model(), Some("llama3.2:latest"));

        bindings.source.set_text("Hello");
        bindings.translate.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Completed
        });
        assert_eq!(state.borrow().output(), "你好，Ollama！");
        let _ = worker.try_send(WorkerCommand::Shutdown);
        drop(window);
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
    }

    // 单个原生流程覆盖真实控件生命周期和恢复表单，避免拆分后并行初始化 GTK。
    #[ignore = "requires the persistent Secret Service onboarding fixture"]
    #[test]
    fn gtk_remembered_credential_uses_secret_service_and_clears_the_form() {
        const SECRET_CANARY: &str = "GTK_PERSISTENT_ONBOARDING_SECRET_CANARY";
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.SecureOnboardingTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK test application");

        let external = ExternalFakeProvider::start(SECRET_CANARY);
        let database_directory = std::env::temp_dir().join(format!(
            "linguamesh-linux-secure-onboarding-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&database_directory);
        let database_path = database_directory.join("state.sqlite3");
        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn_with_database(&database_path));
        let (window, bindings, theme, locale) = create_window(&application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        let context = glib::MainContext::default();
        window.present();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
                && bindings.provider_endpoint.text() != DEFAULT_PROVIDER_ENDPOINT
        });

        bindings
            .provider_name
            .set_text("Secure onboarding provider");
        bindings.provider_endpoint.set_text(&external.endpoint);
        bindings.provider_credential.set_text(SECRET_CANARY);
        bindings.remember_profile.set_active(true);
        bindings.connect.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        assert_eq!(state.borrow().status(), AppStatus::Connecting);
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Ready
                && state.borrow().active_provider().is_some()
        });
        let saved_secret_ref = state
            .borrow()
            .saved_profiles()
            .first()
            .and_then(|profile| profile.secret_ref())
            .cloned()
            .expect("saved Secret Service reference");
        assert!(saved_secret_ref.is_persistent());
        assert!(
            state
                .borrow()
                .active_provider()
                .is_some_and(|profile| { profile.secret_ref() == Some(&saved_secret_ref) })
        );
        assert!(bindings.provider_credential.text().is_empty());
        for entry in fs::read_dir(&database_directory).expect("database directory") {
            let path = entry.expect("database entry").path();
            if path.is_file() {
                let bytes = fs::read(path).expect("database artifact");
                assert!(
                    !bytes
                        .windows(SECRET_CANARY.len())
                        .any(|candidate| candidate == SECRET_CANARY.as_bytes())
                );
            }
        }

        bindings.model.set_selected(1);
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().selected_model().is_some()
        });
        bindings.source.set_text("Hello");
        bindings.translate.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Completed
        });
        assert_eq!(state.borrow().output(), "你好，LinguaMesh！");
        super::secret_service::delete_secret(&saved_secret_ref)
            .expect("delete onboarding credential");
        let _ = worker.try_send(WorkerCommand::Shutdown);
        drop(window);
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
        let _ = fs::remove_dir_all(&database_directory);
    }

    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_authentication_failure_shows_localized_redacted_error() {
        const EXPECTED_SECRET: &str = "GTK_EXPECTED_AUTH_SECRET";
        const WRONG_SECRET: &str = "GTK_WRONG_AUTH_SECRET_CANARY";
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.GtkAuthenticationFailureTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK test application");

        let external = ExternalFakeProvider::start(EXPECTED_SECRET);
        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn());
        let (window, bindings, theme, locale) = create_window(&application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        let context = glib::MainContext::default();
        window.present();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
                && bindings.provider_endpoint.text() != DEFAULT_PROVIDER_ENDPOINT
        });

        bindings.locale.set_selected(1);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            state.borrow().locale() == UiLocale::SimplifiedChinese
        });
        bindings.provider_name.set_text("认证失败提供商");
        bindings.provider_endpoint.set_text(&external.endpoint);
        bindings.provider_credential.set_text(WRONG_SECRET);
        bindings.connect.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        assert_eq!(state.borrow().status(), AppStatus::Connecting);

        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Failed && bindings.error.is_visible()
        });
        let error_text = bindings.error.label().to_string();
        assert!(error_text.contains("身份验证"));
        assert!(error_text.contains("请检查"));
        assert!(error_text.contains("凭据"));
        assert!(!error_text.contains(WRONG_SECRET));
        assert!(!error_text.contains("401"));
        assert!(!error_text.contains("403"));
        assert!(gtk::test_accessible_has_role(
            &bindings.error,
            gtk::AccessibleRole::Alert
        ));
        assert!(state.borrow().active_provider().is_none());

        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
    }

    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_offline_connection_failure_preserves_confirmed_session() {
        const EXPECTED_SECRET: &str = "GTK_EXPECTED_OFFLINE_SECRET";
        const OFFLINE_SECRET: &str = "GTK_OFFLINE_SECRET_CANARY";
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.GtkOfflineConnectionTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK test application");

        let external = ExternalFakeProvider::start(EXPECTED_SECRET);
        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn());
        let (window, bindings, theme, locale) = create_window(&application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        let context = glib::MainContext::default();
        window.present();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
                && bindings.provider_endpoint.text() != DEFAULT_PROVIDER_ENDPOINT
        });

        bindings.locale.set_selected(1);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            state.borrow().locale() == UiLocale::SimplifiedChinese
        });
        bindings
            .provider_name
            .set_text("Confirmed offline provider");
        bindings.provider_endpoint.set_text(&external.endpoint);
        bindings.provider_credential.set_text(EXPECTED_SECRET);
        bindings.connect.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Ready
                && state.borrow().active_provider().is_some()
                && !state.borrow().models().is_empty()
        });
        bindings.model.set_selected(1);
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().selected_model() == Some("fake-translator")
        });
        let confirmed_provider = state
            .borrow()
            .active_provider()
            .cloned()
            .expect("confirmed provider");
        let confirmed_provider_id = confirmed_provider.id().clone();
        let confirmed_models = state.borrow().models().to_vec();
        bindings.source.set_text("保留离线源文本");

        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("offline fixture listener");
        let unavailable_endpoint = format!(
            "http://{}/v1/",
            listener.local_addr().expect("offline fixture address")
        );
        drop(listener);
        bindings
            .provider_name
            .set_text("Unavailable offline provider");
        bindings.provider_endpoint.set_text(&unavailable_endpoint);
        bindings.provider_credential.set_text(OFFLINE_SECRET);
        bindings.connect.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Ready && bindings.error.is_visible()
        });

        let source_text = bindings.source.text(
            &bindings.source.start_iter(),
            &bindings.source.end_iter(),
            true,
        );
        assert_eq!(source_text.as_str(), "保留离线源文本");
        assert_eq!(state.borrow().active_provider(), Some(&confirmed_provider));
        assert_eq!(state.borrow().provider_id(), Some(&confirmed_provider_id));
        assert_eq!(state.borrow().models(), confirmed_models.as_slice());
        assert_eq!(state.borrow().selected_model(), Some("fake-translator"));
        assert_eq!(
            state.borrow().onboarding_stage(),
            super::OnboardingStage::Ready
        );
        let error_text = bindings.error.label().to_string();
        assert!(error_text.contains("网络"));
        assert!(!error_text.contains(OFFLINE_SECRET));
        assert!(gtk::test_accessible_has_role(
            &bindings.error,
            gtk::AccessibleRole::Alert
        ));

        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
    }

    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_cancel_translation_preserves_partial_output() {
        const EXPECTED_SECRET: &str = "GTK_EXPECTED_CANCEL_SECRET";
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.GtkCancellationTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK test application");

        let external = ExternalFakeProvider::start(EXPECTED_SECRET);
        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn());
        let (window, bindings, theme, locale) = create_window(&application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        let context = glib::MainContext::default();
        window.present();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
                && bindings.provider_endpoint.text() != DEFAULT_PROVIDER_ENDPOINT
        });

        bindings.provider_name.set_text("GTK cancellation provider");
        bindings.provider_endpoint.set_text(&external.endpoint);
        bindings.provider_credential.set_text(EXPECTED_SECRET);
        bindings.connect.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Ready
                && state.borrow().active_provider().is_some()
                && !state.borrow().models().is_empty()
        });

        bindings.model.set_selected(2);
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().selected_model() == Some("fake-slow-translator")
        });
        bindings
            .source
            .set_text("Cancel after the first streamed delta.");
        bindings.translate.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Translating
                && !state.borrow().output().is_empty()
                && bindings.stop.is_sensitive()
        });
        let partial_output = state.borrow().output().to_owned();
        assert_eq!(partial_output, "你好");
        bindings.stop.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Cancelled && state.borrow().has_partial_output()
        });
        assert_eq!(state.borrow().output(), partial_output);
        assert!(bindings.status.label().contains("Translation cancelled"));
        assert!(!bindings.stop.is_sensitive());
        assert!(bindings.retry_translation.is_sensitive());
        assert!(state.borrow().error_text().is_none());

        std::thread::sleep(Duration::from_millis(350));
        while context.pending() {
            context.iteration(false);
        }
        assert_eq!(state.borrow().status(), AppStatus::Cancelled);
        assert_eq!(state.borrow().output(), partial_output);

        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
    }

    #[allow(clippy::too_many_lines)]
    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_malicious_archive_import_fails_closed_before_document_job() {
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.GtkMaliciousArchiveTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK malicious archive application");

        let fixture_directory = std::env::temp_dir().join(format!(
            "linguamesh-linux-gtk-malicious-archive-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&fixture_directory);
        fs::create_dir_all(&fixture_directory).expect("create malicious archive directory");
        fs::set_permissions(&fixture_directory, fs::Permissions::from_mode(0o700))
            .expect("restrict malicious archive directory");
        let repetitive_payload = vec![b'x'; 512 * 1024];
        let cases = vec![
            (
                "unsafe.docx",
                malicious_docx_fixture("../outside.txt", b"unsafe", false),
                "outside.txt",
            ),
            (
                "suspicious-ratio.docx",
                malicious_docx_fixture("word/repetitive.bin", &repetitive_payload, true),
                "repetitive.bin",
            ),
            (
                "macro.docx",
                malicious_docx_fixture("word/vbaProject.bin", b"unsupported macro", false),
                "vbaProject.bin",
            ),
            (
                "signed.docx",
                malicious_docx_fixture("_xmlsignatures/sig1.xml", b"unsupported signature", false),
                "sig1.xml",
            ),
        ];

        let context = glib::MainContext::default();
        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn());
        let (window, bindings, theme, locale) = create_window(&application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        window.present();
        eprintln!("GTK malicious archive: waiting for worker readiness");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
        });

        for (file_name, contents, forbidden_name) in cases {
            let fixture_path = fixture_directory.join(file_name);
            fs::write(&fixture_path, contents).expect("write malicious archive fixture");
            let file = gtk::gio::File::for_path(&fixture_path);
            load_source_file(&file, &bindings, &state, &worker);
            eprintln!("GTK malicious archive: waiting for rejected {file_name}");
            spin_main_context_until(&context, Duration::from_secs(5), || {
                bindings.error.is_visible()
            });
            let error_text = bindings.error.text().to_string();
            assert!(!error_text.is_empty());
            assert!(!error_text.contains(forbidden_name));
            assert!(bindings.document_job_id.borrow().is_none());
            assert!(
                bindings
                    .source
                    .text(
                        &bindings.source.start_iter(),
                        &bindings.source.end_iter(),
                        true,
                    )
                    .is_empty()
            );
            assert!(!fixture_directory.join(forbidden_name).exists());
            bindings.error.set_visible(false);
        }

        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
        let _ = fs::remove_dir_all(fixture_directory);
    }

    #[allow(clippy::too_many_lines)]
    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_glossary_and_protected_terms_preserve_translation() {
        const EXPECTED_SECRET: &str = "GTK_EXPECTED_GLOSSARY_SECRET";
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.GtkGlossaryTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK test application");

        let listener = TcpListener::bind("127.0.0.1:0").expect("glossary provider listener");
        let endpoint = format!(
            "http://{}/v1/",
            listener.local_addr().expect("listener address")
        );
        let (request_sender, request_receiver) = mpsc::sync_channel(1);
        let provider_thread = std::thread::spawn(move || {
            let (mut model_stream, _) = listener.accept().expect("model request");
            let _ = read_http_request(&mut model_stream);
            let models = r#"{"data":[{"id":"glossary-translator","object":"model"}]}"#;
            write_http_response(&mut model_stream, "application/json", models.as_bytes());

            let (mut chat_stream, _) = listener.accept().expect("chat request");
            let request = read_http_request(&mut chat_stream);
            let request_text = String::from_utf8_lossy(&request);
            request_sender
                .send(request_text.contains("LinguaMesh"))
                .expect("request observation");
            let prefix = b"__LINGUAMESH_PROTECTED_";
            let marker_start = request
                .windows(prefix.len())
                .rposition(|window| window == prefix)
                .expect("protected glossary marker");
            let suffix_start = marker_start + prefix.len();
            let suffix_offset = request[suffix_start..]
                .windows(2)
                .position(|window| window == b"__")
                .expect("protected marker suffix");
            let marker =
                String::from_utf8(request[marker_start..suffix_start + suffix_offset + 2].to_vec())
                    .expect("protected marker text");
            let split = marker.len() / 2;
            let mut events = String::new();
            for fragment in [
                "你好，".to_owned(),
                marker[..split].to_owned(),
                marker[split..].to_owned(),
                "！".to_owned(),
            ] {
                writeln!(
                    &mut events,
                    "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{fragment}\"}}}}]}}"
                )
                .expect("SSE event");
                events.push('\n');
            }
            events.push_str("data: [DONE]\n\n");
            write_http_response(&mut chat_stream, "text/event-stream", events.as_bytes());
        });

        let context = glib::MainContext::default();
        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn());
        let (window, bindings, theme, locale) = create_window(&application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        window.present();
        eprintln!("GTK document restart: waiting for first worker readiness");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
        });

        bindings.provider_name.set_text("GTK glossary provider");
        bindings.provider_endpoint.set_text(&endpoint);
        bindings.provider_credential.set_text(EXPECTED_SECRET);
        bindings.connect.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        eprintln!("GTK document restart: waiting for first provider connection");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Ready
                && state.borrow().active_provider().is_some()
                && !state.borrow().models().is_empty()
        });

        bindings.model.set_selected(1);
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().selected_model() == Some("glossary-translator")
        });
        bindings.source.set_text("LinguaMesh");
        bindings.glossary.set_text("LinguaMesh => 凌瓦网");
        bindings.translate.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Completed
                && state.borrow().output() == "你好，凌瓦网！"
        });
        assert_eq!(state.borrow().output(), "你好，凌瓦网！");
        assert!(state.borrow().glossary().is_some());
        assert!(
            !request_receiver
                .recv_timeout(Duration::from_secs(5))
                .expect("request observation")
        );

        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
        provider_thread.join().expect("glossary provider shutdown");
    }

    #[allow(clippy::too_many_lines)]
    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_incognito_translation_bypasses_memory_and_persistence() {
        const EXPECTED_SECRET: &str = "GTK_EXPECTED_INCOGNITO_SECRET";
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.GtkIncognitoTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK incognito application");

        let database_directory = std::env::temp_dir().join(format!(
            "linguamesh-linux-incognito-ui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after Unix epoch")
                .as_nanos()
        ));
        fs::create_dir(&database_directory).expect("create incognito UI database directory");
        fs::set_permissions(&database_directory, fs::Permissions::from_mode(0o700))
            .expect("protect incognito UI database directory");
        let database_path = database_directory.join("state.sqlite3");
        let external = ExternalFakeProvider::start(EXPECTED_SECRET);
        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn_with_database(&database_path));
        let (window, bindings, theme, locale) = create_window(&application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        let context = glib::MainContext::default();
        window.present();
        eprintln!("GTK incognito: waiting for worker readiness");
        // 等待数据库 worker 就绪后再驱动真实 GTK 连接流程。
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
        });

        bindings.provider_name.set_text("GTK incognito provider");
        bindings.provider_endpoint.set_text(&external.endpoint);
        bindings.provider_credential.set_text(EXPECTED_SECRET);
        bindings.connect.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        eprintln!("GTK incognito: waiting for provider connection");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Ready
                && state.borrow().active_provider().is_some()
                && !state.borrow().models().is_empty()
        });
        bindings.model.set_selected(1);
        eprintln!("GTK incognito: waiting for model selection");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().selected_model() == Some("fake-translator")
        });

        bindings.source.set_text("GTK incognito memory probe");
        bindings.translate.emit_clicked();
        eprintln!("GTK incognito: waiting for standard completion");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Completed
                && state.borrow().translation_history_count() == 1
        });
        let storage = Storage::open(&database_path).expect("open incognito UI storage");
        assert_eq!(
            storage
                .translation_history_count()
                .expect("history count after standard translation"),
            1
        );
        assert_eq!(
            storage
                .translation_memory_count()
                .expect("memory count after standard translation"),
            1
        );
        drop(storage);
        let first_chat_requests = external.chat_requests.load(Ordering::SeqCst);
        assert!(first_chat_requests >= 1);

        bindings.incognito.set_active(true);
        eprintln!("GTK incognito: waiting for privacy toggle");
        spin_main_context_until(&context, Duration::from_secs(1), || {
            state.borrow().is_incognito()
        });
        bindings.source.set_text("GTK incognito memory probe");
        bindings.translate.emit_clicked();
        eprintln!("GTK incognito: waiting for private completion");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Completed
                && external.chat_requests.load(Ordering::SeqCst) > first_chat_requests
        });
        assert!(state.borrow().is_incognito());
        assert!(bindings.incognito.is_active());
        let storage = Storage::open(&database_path).expect("reopen incognito UI storage");
        assert_eq!(
            storage
                .translation_history_count()
                .expect("history count after incognito translation"),
            1
        );
        assert_eq!(
            storage
                .translation_memory_count()
                .expect("memory count after incognito translation"),
            1
        );
        drop(storage);

        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
        let _ = fs::remove_dir_all(&database_directory);
    }

    #[allow(clippy::too_many_lines)]
    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_interrupted_document_job_restores_and_resumes() {
        const EXPECTED_SECRET: &str = "GTK_EXPECTED_DOCUMENT_RESTART_SECRET";
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.GtkDocumentRestartTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK document restart application");

        let database_directory = std::env::temp_dir().join(format!(
            "linguamesh-linux-gtk-document-restart-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&database_directory);
        fs::create_dir_all(&database_directory).expect("create document restart directory");
        fs::set_permissions(&database_directory, fs::Permissions::from_mode(0o700))
            .expect("restrict document restart directory");
        let database_path = database_directory.join("state.sqlite3");
        let external = ExternalFakeProvider::start(EXPECTED_SECRET);
        let profile_id = ProviderProfileId::parse("gtk-document-restart-provider")
            .expect("document restart provider profile ID");
        let context = glib::MainContext::default();
        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn_with_database(&database_path));
        let (window, bindings, theme, locale) = create_window(&application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        window.present();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
        });

        bindings
            .provider_name
            .set_text("GTK document restart provider");
        bindings.provider_endpoint.set_text(&external.endpoint);
        bindings.provider_credential.set_text(EXPECTED_SECRET);
        bindings.draft_profile_id.replace(Some(profile_id.clone()));
        bindings.connect.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Ready
                && state.borrow().active_provider().is_some()
                && !state.borrow().models().is_empty()
        });
        bindings.model.set_selected(2);
        eprintln!("GTK document restart: waiting for first model selection");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().selected_model() == Some("fake-slow-translator")
        });

        let job_id = "gtk-document-restart-1".to_owned();
        worker
            .try_send(WorkerCommand::CreateDocumentJob {
                job_id: job_id.clone(),
                job: DocumentJob::from_text("notes.txt", DocumentFormat::Txt, "one\ntwo"),
            })
            .expect("create GTK document job");
        eprintln!("GTK document restart: waiting for pending document job");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            bindings.document_job_id.borrow().as_deref() == Some(job_id.as_str())
                && bindings.document_job_state.get() == Some(DocumentJobState::Pending)
        });
        bindings.document_job_guard.set(true);
        bindings.source.set_text("one\ntwo");
        bindings.document_job_guard.set(false);
        bindings.translate.emit_clicked();
        eprintln!("GTK document restart: waiting for first segment");
        spin_main_context_until(&context, Duration::from_secs(10), || {
            state.borrow().status() == AppStatus::Translating
                && bindings.document_job_state.get() == Some(DocumentJobState::Running)
                && bindings
                    .document_progress
                    .get()
                    .is_some_and(|(completed, total)| completed == 1 && total == 2)
        });
        assert!(bindings.pause_document.is_sensitive());
        bindings.pause_document.emit_clicked();
        eprintln!("GTK document restart: waiting for paused state");
        spin_main_context_until(&context, Duration::from_secs(10), || {
            bindings.document_job_state.get() == Some(DocumentJobState::Paused)
        });
        assert_eq!(bindings.document_progress.get(), Some((1, 2)));
        let source_text = bindings.source.text(
            &bindings.source.start_iter(),
            &bindings.source.end_iter(),
            true,
        );
        assert_eq!(source_text.as_str(), "one\ntwo");

        worker
            .try_send(WorkerCommand::Shutdown)
            .expect("shutdown first GTK document worker");
        eprintln!("GTK document restart: waiting for first worker shutdown");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_unavailable()
        });
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);

        let restored_state = Rc::new(RefCell::new(AppState::default()));
        let restored_worker = Rc::new(CoreWorker::spawn_with_database(&database_path));
        let (restored_window, restored_bindings, restored_theme, restored_locale) =
            create_window(&application);
        connect_selection_handlers(
            &restored_bindings,
            &restored_theme,
            &restored_locale,
            &restored_state,
            &restored_worker,
        );
        connect_action_handlers(&restored_bindings, &restored_state, &restored_worker);
        start_event_pump(&restored_bindings, &restored_state, &restored_worker);
        restored_window.present();
        eprintln!("GTK document restart: waiting for restored paused job");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            restored_state.borrow().worker_ready()
                && restored_bindings.document_job_id.borrow().as_deref() == Some(job_id.as_str())
                && restored_bindings.document_job_state.get() == Some(DocumentJobState::Paused)
        });
        assert_eq!(restored_bindings.document_progress.get(), Some((1, 2)));
        let restored_source = restored_bindings.source.text(
            &restored_bindings.source.start_iter(),
            &restored_bindings.source.end_iter(),
            true,
        );
        assert_eq!(restored_source.as_str(), "one\ntwo");

        restored_bindings
            .provider_name
            .set_text("GTK document restart provider");
        restored_bindings
            .provider_endpoint
            .set_text(&external.endpoint);
        restored_bindings
            .provider_credential
            .set_text(EXPECTED_SECRET);
        restored_bindings.draft_profile_id.replace(Some(profile_id));
        restored_bindings.connect.emit_clicked();
        assert!(restored_bindings.provider_credential.text().is_empty());
        eprintln!("GTK document restart: waiting for restored provider connection");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            restored_state.borrow().status() == AppStatus::Ready
                && restored_state.borrow().active_provider().is_some()
        });
        restored_bindings.model.set_selected(2);
        eprintln!("GTK document restart: waiting for restored model selection");
        spin_main_context_until(&context, Duration::from_secs(5), || {
            restored_state.borrow().selected_model() == Some("fake-slow-translator")
        });
        assert!(restored_bindings.resume_document.is_sensitive());
        restored_bindings.resume_document.emit_clicked();
        eprintln!("GTK document restart: waiting for resumed completion");
        let resume_wait_started = Instant::now();
        let mut resume_diagnostic_logged = false;
        spin_main_context_until(&context, Duration::from_secs(15), || {
            if !resume_diagnostic_logged && resume_wait_started.elapsed() >= Duration::from_secs(5)
            {
                let status_completed = restored_state.borrow().status() == AppStatus::Completed;
                let safe_error = restored_state
                    .borrow()
                    .error_text()
                    .map(|error| error.replace(EXPECTED_SECRET, "<redacted>"));
                eprintln!(
                    "GTK document restart: resume snapshot status_completed={status_completed}, job_state={:?}, progress={:?}, error={safe_error:?}",
                    restored_bindings.document_job_state.get(),
                    restored_bindings.document_progress.get(),
                );
                resume_diagnostic_logged = true;
            }
            restored_bindings.document_job_state.get() == Some(DocumentJobState::Completed)
                && restored_bindings.document_progress.get() == Some((2, 2))
                && restored_state.borrow().status() == AppStatus::Completed
        });
        assert_eq!(
            restored_state.borrow().output(),
            "你好，LinguaMesh！\n你好，LinguaMesh！"
        );
        assert_eq!(
            restored_bindings.source.text(
                &restored_bindings.source.start_iter(),
                &restored_bindings.source.end_iter(),
                true,
            ),
            "one\ntwo"
        );

        restored_worker
            .try_send(WorkerCommand::Shutdown)
            .expect("shutdown restored GTK document worker");
        restored_window.close();
        drop(restored_bindings);
        drop(restored_theme);
        drop(restored_locale);
        drop(restored_state);
        drop(restored_worker);
        drop(external);
        let _ = fs::remove_dir_all(database_directory);
    }

    // 验证生产文档任务对话框可以展示多个任务，并且显式选择不会混淆任务快照。
    #[allow(clippy::too_many_lines)]
    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_document_jobs_dialog_selects_between_multiple_jobs() {
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.GtkDocumentJobsTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK document jobs application");

        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn());
        let (window, bindings, theme, locale) = create_window(&application);
        let first_job = DocumentJobSnapshot {
            job_id: "gtk-queue-first".to_owned(),
            state: DocumentJobState::Pending,
            job: DocumentJob::from_text("first.txt", DocumentFormat::Txt, "first source"),
            options: None,
            created_at: 1,
            updated_at: 1,
        };
        let second_job = DocumentJobSnapshot {
            job_id: "gtk-queue-second".to_owned(),
            state: DocumentJobState::Paused,
            job: DocumentJob::from_text("second.md", DocumentFormat::Markdown, "second source"),
            options: None,
            created_at: 2,
            updated_at: 2,
        };
        let cancelled_job = DocumentJobSnapshot {
            job_id: "gtk-queue-cancelled".to_owned(),
            state: DocumentJobState::Cancelled,
            job: DocumentJob::from_text("cancelled.txt", DocumentFormat::Txt, "cancelled source"),
            options: None,
            created_at: 3,
            updated_at: 3,
        };
        let context = glib::MainContext::default();
        window.present();
        show_document_jobs_dialog(
            &bindings,
            &state,
            &worker.command_handle(),
            vec![first_job.clone(), second_job.clone(), cancelled_job.clone()],
        );
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Document jobs"))
        });
        let dialog = application
            .windows()
            .into_iter()
            .find(|candidate| candidate.title().as_deref() == Some("Document jobs"))
            .expect("document jobs dialog");
        let widgets = descendant_widgets(dialog.upcast_ref::<gtk::Widget>());
        let metadata = widgets
            .iter()
            .filter_map(|widget| {
                widget
                    .downcast_ref::<gtk::Label>()
                    .map(|label| label.text().to_string())
            })
            .collect::<Vec<_>>();
        assert!(metadata.iter().any(|label| label.contains("3 files")));
        assert!(metadata.iter().any(|label| label.contains("first.txt")));
        assert!(metadata.iter().any(|label| label.contains("second.md")));
        assert!(metadata.iter().any(|label| label.contains("cancelled.txt")));
        // 验证每个持久化任务都暴露可聚焦且带有安全提示的报告导出动作。
        let report_buttons = widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .filter(|button| {
                button
                    .label()
                    .is_some_and(|label| label.contains("Export translation report"))
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(report_buttons.len(), 3);
        assert!(
            report_buttons
                .iter()
                .all(adw::prelude::WidgetExt::is_focusable)
        );
        assert!(report_buttons.iter().all(|button| {
            button
                .tooltip_text()
                .is_some_and(|tooltip| tooltip.contains("redacted TSV report"))
        }));
        let select_buttons = widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .filter(|button| {
                button
                    .label()
                    .is_some_and(|label| label.contains("Select document job"))
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(select_buttons.len(), 3);
        select_buttons[1].emit_clicked();
        assert_eq!(
            bindings.document_job_id.borrow().as_deref(),
            Some("gtk-queue-second")
        );
        assert_eq!(
            bindings.document_job_state.get(),
            Some(DocumentJobState::Paused)
        );
        assert_eq!(
            bindings.source.text(
                &bindings.source.start_iter(),
                &bindings.source.end_iter(),
                true
            ),
            "second source"
        );
        assert!(
            !application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Document jobs"))
        );

        // 验证暂停任务的 Resume 动作仍然绑定到同一个任务，并在发送命令后关闭队列窗口。
        show_document_jobs_dialog(
            &bindings,
            &state,
            &worker.command_handle(),
            vec![first_job.clone(), second_job.clone(), cancelled_job.clone()],
        );
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Document jobs"))
        });
        let resume_dialog = application
            .windows()
            .into_iter()
            .find(|candidate| candidate.title().as_deref() == Some("Document jobs"))
            .expect("document jobs resume dialog");
        let resume_buttons = descendant_widgets(resume_dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .filter(|button| {
                button
                    .label()
                    .is_some_and(|label| label.contains("Resume document"))
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(resume_buttons.len(), 1);
        resume_buttons[0].emit_clicked();
        assert_eq!(
            bindings.document_job_id.borrow().as_deref(),
            Some("gtk-queue-second")
        );
        assert_eq!(
            bindings.document_job_state.get(),
            Some(DocumentJobState::Paused)
        );
        assert!(
            !application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Document jobs"))
        );

        // 验证取消任务的 Retry 动作绑定到同一个任务，并在发送命令后关闭队列窗口。
        show_document_jobs_dialog(
            &bindings,
            &state,
            &worker.command_handle(),
            vec![first_job.clone(), second_job.clone(), cancelled_job.clone()],
        );
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Document jobs"))
        });
        let retry_dialog = application
            .windows()
            .into_iter()
            .find(|candidate| candidate.title().as_deref() == Some("Document jobs"))
            .expect("document jobs retry dialog");
        let retry_buttons = descendant_widgets(retry_dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .filter(|button| {
                button
                    .label()
                    .is_some_and(|label| label.contains("Retry document"))
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(retry_buttons.len(), 1);
        retry_buttons[0].emit_clicked();
        assert_eq!(
            bindings.document_job_id.borrow().as_deref(),
            Some("gtk-queue-cancelled")
        );
        assert_eq!(
            bindings.document_job_state.get(),
            Some(DocumentJobState::Cancelled)
        );
        assert!(
            !application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Document jobs"))
        );

        // 验证待处理任务的 Pause 动作绑定到同一个任务，并在发送命令后关闭队列窗口。
        show_document_jobs_dialog(
            &bindings,
            &state,
            &worker.command_handle(),
            vec![first_job, second_job, cancelled_job],
        );
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Document jobs"))
        });
        let pause_dialog = application
            .windows()
            .into_iter()
            .find(|candidate| candidate.title().as_deref() == Some("Document jobs"))
            .expect("document jobs pause dialog");
        let pause_buttons = descendant_widgets(pause_dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .filter(|button| {
                button
                    .label()
                    .is_some_and(|label| label.contains("Pause document"))
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(pause_buttons.len(), 1);
        pause_buttons[0].emit_clicked();
        assert_eq!(
            bindings.document_job_id.borrow().as_deref(),
            Some("gtk-queue-first")
        );
        assert_eq!(
            bindings.document_job_state.get(),
            Some(DocumentJobState::Pending)
        );
        assert!(
            !application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Document jobs"))
        );

        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
    }

    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_connection_test_reports_models_and_redacts_credential() {
        const EXPECTED_SECRET: &str = "GTK_EXPECTED_CONNECTION_TEST_SECRET";
        const WRONG_SECRET: &str = "GTK_WRONG_CONNECTION_TEST_SECRET_CANARY";
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.GtkConnectionTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK test application");

        let external = ExternalFakeProvider::start(EXPECTED_SECRET);
        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn());
        let (window, bindings, theme, locale) = create_window(&application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        let context = glib::MainContext::default();
        window.present();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
                && bindings.provider_endpoint.text() != DEFAULT_PROVIDER_ENDPOINT
        });

        bindings.locale.set_selected(1);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            state.borrow().locale() == UiLocale::SimplifiedChinese
        });
        bindings.provider_name.set_text("连接测试提供商");
        bindings.provider_endpoint.set_text(&external.endpoint);
        bindings.provider_credential.set_text(EXPECTED_SECRET);
        bindings.test_connection.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        spin_main_context_until(&context, Duration::from_secs(5), || {
            bindings.connection_test_notice.get()
        });
        let model_count = bindings
            .connection_test_model_count
            .get()
            .expect("connection test model count");
        assert!(model_count >= 1);
        assert!(
            bindings
                .connection_test_profile_id
                .borrow()
                .as_deref()
                .is_some_and(|profile_id| !profile_id.is_empty())
        );
        assert!(bindings.locale_note.label().contains("连接测试"));
        assert!(!bindings.locale_note.label().contains(EXPECTED_SECRET));

        bindings.provider_credential.set_text(WRONG_SECRET);
        bindings.test_connection.emit_clicked();
        assert!(bindings.provider_credential.text().is_empty());
        spin_main_context_until(&context, Duration::from_secs(5), || {
            !bindings.connection_test_notice.get() && bindings.error.is_visible()
        });
        let error_text = bindings.error.label().to_string();
        assert!(error_text.contains("身份验证"));
        assert!(!error_text.contains(WRONG_SECRET));
        assert!(!error_text.contains("401"));
        assert!(!error_text.contains("403"));

        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
        drop(state);
        drop(worker);
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn gtk_buttons_explicitly_connect_select_and_translate_with_session_credential() {
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.GtkTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK test application");

        let state = Rc::new(RefCell::new(AppState::default()));
        let worker = Rc::new(CoreWorker::spawn());
        let (window, bindings, theme, locale) = create_window(&application);
        connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
        connect_action_handlers(&bindings, &state, &worker);
        start_event_pump(&bindings, &state, &worker);
        apply_worker_event(
            &bindings,
            &state,
            &worker,
            WorkerEvent::ProfilesRestored {
                profiles: Vec::new(),
                active_profile_id: None,
            },
        );
        refresh_ui(&bindings, &state.borrow());
        window.present();
        state.borrow_mut().set_locale(UiLocale::SimplifiedChinese);
        refresh_ui(&bindings, &state.borrow());
        assert_eq!(bindings.translate.label().as_deref(), Some("_翻译"));
        assert_eq!(bindings.export_output.label().as_deref(), Some("_导出翻译"));
        assert!(!bindings.export_output.is_sensitive());
        assert_eq!(
            bindings.open_output.label().as_deref(),
            Some("_打开已导出输出")
        );
        assert!(!bindings.open_output.is_sensitive());
        assert_eq!(bindings.stop.label().as_deref(), Some("_停止翻译"));
        assert_eq!(bindings.window.title().as_deref(), Some("LinguaMesh"));
        assert_eq!(bindings.source_label.label(), "源文本");
        assert_eq!(bindings.output_label.label(), "译文");
        assert_eq!(bindings.status.label(), "状态: 正在启动");
        assert_eq!(
            bindings.open_source.label().as_deref(),
            Some("_打开文本文件")
        );
        assert_eq!(bindings.provider_title.label(), "提供商配置");
        assert_eq!(bindings.provider_preset.selected(), 0);
        assert_labeled_control(bindings.provider_preset.upcast_ref::<gtk::Widget>());
        let provider_preset_model = bindings
            .provider_preset
            .model()
            .and_then(|model| model.downcast::<gtk::StringList>().ok())
            .expect("provider preset labels");
        assert_eq!(
            provider_preset_model.string(0).as_deref(),
            Some("OpenAI 兼容")
        );
        assert_eq!(
            provider_preset_model.string(1).as_deref(),
            Some("Ollama（原生 /api）")
        );
        assert_eq!(
            provider_preset_model.string(4).as_deref(),
            Some("Azure OpenAI")
        );
        assert_eq!(bindings.connect.label().as_deref(), Some("_连接"));
        assert_eq!(
            bindings.test_connection.label().as_deref(),
            Some("_测试连接")
        );
        assert_eq!(
            bindings.remove_saved_profile.label().as_deref(),
            Some("移除已保存配置")
        );
        assert_eq!(
            bindings.remember_profile.label().as_deref(),
            Some("通过 Secret Service 记住配置、模型和凭据")
        );
        assert_eq!(
            bindings.fallback_enabled.label().as_deref(),
            Some("_允许使用已批准的回退")
        );
        assert!(!bindings.fallback_enabled.is_sensitive());
        assert_eq!(bindings.fallback_profile_label.label(), "_回退提供商");
        let source_language_model = bindings
            .source_locale
            .model()
            .and_then(|model| model.downcast::<gtk::StringList>().ok())
            .expect("source language labels");
        assert_eq!(source_language_model.string(0).as_deref(), Some("自动"));
        let target_language_model = bindings
            .target_locale
            .model()
            .and_then(|model| model.downcast::<gtk::StringList>().ok())
            .expect("target language labels");
        assert_eq!(target_language_model.string(0).as_deref(), Some("简体中文"));
        let theme_model = bindings
            .theme
            .model()
            .and_then(|model| model.downcast::<gtk::StringList>().ok())
            .expect("theme labels");
        assert_eq!(theme_model.string(1).as_deref(), Some("浅色"));
        let locale_model = bindings
            .locale
            .model()
            .and_then(|model| model.downcast::<gtk::StringList>().ok())
            .expect("locale labels");
        assert_eq!(locale_model.string(0).as_deref(), Some("英语"));
        assert_eq!(locale_model.string(10).as_deref(), Some("阿拉伯语"));
        bindings.source.set_text("保留源文本");
        state.borrow_mut().set_locale(UiLocale::Arabic);
        refresh_ui(&bindings, &state.borrow());
        assert_eq!(bindings.status.label(), "الحالة: جارٍ البدء");
        assert_eq!(bindings.workspace.direction(), gtk::TextDirection::Rtl);
        let source_text = bindings.source.text(
            &bindings.source.start_iter(),
            &bindings.source.end_iter(),
            true,
        );
        assert_eq!(source_text.as_str(), "保留源文本");
        state.borrow_mut().set_locale(UiLocale::English);
        refresh_ui(&bindings, &state.borrow());

        assert!(gtk::test_accessible_has_role(
            &bindings.workspace,
            gtk::AccessibleRole::Main
        ));
        assert!(gtk::test_accessible_has_role(
            &bindings.onboarding_title,
            gtk::AccessibleRole::Heading
        ));
        assert!(gtk::test_accessible_has_role(
            &bindings.status,
            gtk::AccessibleRole::Status
        ));
        assert!(gtk::test_accessible_has_role(
            &bindings.error,
            gtk::AccessibleRole::Alert
        ));
        assert!(gtk::test_accessible_has_role(
            &bindings.progress,
            gtk::AccessibleRole::ProgressBar
        ));
        assert!(!bindings.progress.is_visible());
        bindings.document_progress.set(Some((2, 4)));
        refresh_ui(&bindings, &state.borrow());
        assert!(bindings.progress.is_visible());
        assert!((bindings.progress.fraction() - 0.5).abs() < f64::EPSILON);
        let expected_progress = localized_template(
            UiLocale::English,
            "status.document_progress",
            "{completed} of {total} segments translated",
            &[("{completed}", "2"), ("{total}", "4")],
        );
        assert_eq!(
            bindings.progress.text().as_deref(),
            Some(expected_progress.as_str())
        );
        bindings.document_progress.set(None);
        refresh_ui(&bindings, &state.borrow());
        assert!(!bindings.progress.is_visible());
        for control in [
            bindings.saved_profile.upcast_ref::<gtk::Widget>(),
            bindings.provider_preset.upcast_ref::<gtk::Widget>(),
            bindings.provider_name.upcast_ref::<gtk::Widget>(),
            bindings.provider_endpoint.upcast_ref::<gtk::Widget>(),
            bindings.manual_model.upcast_ref::<gtk::Widget>(),
            bindings.provider_credential.upcast_ref::<gtk::Widget>(),
            bindings.model.upcast_ref::<gtk::Widget>(),
            bindings.source_locale.upcast_ref::<gtk::Widget>(),
            bindings.target_locale.upcast_ref::<gtk::Widget>(),
            theme.upcast_ref::<gtk::Widget>(),
            locale.upcast_ref::<gtk::Widget>(),
        ] {
            assert_labeled_control(control);
        }
        for button in [
            &bindings.remove_saved_profile,
            &bindings.test_connection,
            &bindings.connect,
            &bindings.open_source,
            &bindings.translate,
            &bindings.retry_translation,
            &bindings.export_output,
            &bindings.open_output,
            &bindings.stop,
            &bindings.document_jobs,
            &bindings.pause_document,
            &bindings.resume_document,
            &bindings.retry_document,
        ] {
            assert!(button.is_focusable());
        }
        assert!(bindings.ocr_enabled.is_focusable());
        assert!(bindings.remember_profile.is_focusable());
        assert!(bindings.fallback_enabled.is_focusable());
        assert!(gtk::test_accessible_has_property(
            &bindings.fallback_enabled,
            gtk::AccessibleProperty::Label
        ));
        assert!(bindings.fallback_profile.is_focusable());
        assert!(gtk::test_accessible_has_relation(
            &bindings.fallback_profile,
            gtk::AccessibleRelation::LabelledBy
        ));
        assert_eq!(
            bindings.fallback_profile_label.mnemonic_widget(),
            Some(bindings.fallback_profile.clone().upcast::<gtk::Widget>())
        );
        assert!(bindings.source_view.is_focusable());
        assert!(bindings.output_view.is_focusable());
        assert!(gtk::test_accessible_has_role(
            &bindings.source_view,
            gtk::AccessibleRole::TextBox
        ));
        assert!(gtk::test_accessible_has_role(
            &bindings.output_view,
            gtk::AccessibleRole::TextBox
        ));
        for property in [
            gtk::AccessibleProperty::Label,
            gtk::AccessibleProperty::MultiLine,
            gtk::AccessibleProperty::ReadOnly,
        ] {
            assert!(gtk::test_accessible_has_property(
                &bindings.source_view,
                property
            ));
            assert!(gtk::test_accessible_has_property(
                &bindings.output_view,
                property
            ));
        }
        assert!(gtk::test_accessible_has_relation(
            &bindings.source_view,
            gtk::AccessibleRelation::LabelledBy
        ));
        assert!(gtk::test_accessible_has_relation(
            &bindings.output_view,
            gtk::AccessibleRelation::LabelledBy
        ));
        assert_eq!(
            bindings.source_label.mnemonic_widget(),
            Some(bindings.source_view.clone().upcast::<gtk::Widget>())
        );
        assert_eq!(
            bindings.output_label.mnemonic_widget(),
            Some(bindings.output_view.clone().upcast::<gtk::Widget>())
        );
        assert!(gtk::test_accessible_has_property(
            &bindings.stop,
            gtk::AccessibleProperty::Label
        ));
        assert!(!gtk::test_accessible_has_state(
            &bindings.workspace,
            gtk::AccessibleState::Busy
        ));
        assert!(!gtk::test_accessible_has_state(
            &bindings.output_view,
            gtk::AccessibleState::Busy
        ));
        assert!(gtk::test_accessible_has_state(
            &bindings.error,
            gtk::AccessibleState::Hidden
        ));
        assert!(!bindings.error.is_visible());

        assert!(!state.borrow().worker_ready());
        assert_eq!(state.borrow().onboarding_stage(), OnboardingStage::Starting);
        assert_eq!(
            bindings.onboarding_title.label(),
            "Provider setup · Starting"
        );
        assert_eq!(bindings.status.label(), "Status: Starting");
        assert!(!bindings.provider_name.is_sensitive());
        assert!(!bindings.provider_endpoint.is_sensitive());
        assert!(!bindings.provider_credential.is_sensitive());
        assert!(!bindings.saved_profile.is_sensitive());
        assert!(!bindings.remember_profile.is_sensitive());
        assert!(!bindings.remove_saved_profile.is_sensitive());
        assert!(!bindings.connect.is_sensitive());

        let context = glib::MainContext::default();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().worker_ready()
                && bindings.provider_endpoint.text() != DEFAULT_PROVIDER_ENDPOINT
        });
        assert_eq!(state.borrow().status(), AppStatus::Disconnected);
        assert_eq!(
            state.borrow().onboarding_stage(),
            OnboardingStage::ConfigureProvider
        );
        assert_eq!(
            bindings.onboarding_title.label(),
            "Provider setup · Step 1 of 2"
        );
        assert!(bindings.onboarding_detail.label().contains("session only"));
        assert!(state.borrow().active_provider().is_none());
        assert!(!bindings.remember_profile.is_active());
        assert!(bindings.provider_name.is_sensitive());
        assert!(bindings.provider_endpoint.is_sensitive());
        assert!(bindings.provider_credential.is_sensitive());
        assert!(bindings.saved_profile.is_sensitive());
        assert!(bindings.remember_profile.is_sensitive());
        assert!(!bindings.remove_saved_profile.is_sensitive());
        assert!(bindings.connect.is_sensitive());
        let demo_endpoint = bindings.provider_endpoint.text().to_string();
        bindings.locale.set_selected(1);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            state.borrow().locale() == UiLocale::SimplifiedChinese
        });
        show_new_profile_in_form(&bindings, &state.borrow())
            .expect("initialize localized provider profile form");
        assert_eq!(
            bindings.provider_name.text().as_str(),
            "本地 OpenAI 兼容提供商"
        );
        bindings.provider_preset.set_selected(2);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            bindings.manual_model_row.is_visible()
        });
        bindings
            .provider_endpoint
            .set_text(DEFAULT_ANTHROPIC_ENDPOINT);
        bindings.manual_model.set_text("");
        bindings.connect.emit_clicked();
        assert_eq!(state.borrow().status(), AppStatus::Failed);
        assert!(
            state
                .borrow()
                .error_text()
                .is_some_and(|text| text.contains("Anthropic model ID"))
        );
        assert!(state.borrow().active_provider().is_none());
        show_new_profile_in_form(&bindings, &state.borrow())
            .expect("restore custom provider profile after Anthropic validation");
        bindings.provider_preset.set_selected(1);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            bindings.provider_name.text().as_str() == "本地 Ollama 提供商"
        });
        assert_eq!(bindings.provider_name.text().as_str(), "本地 Ollama 提供商");
        bindings.provider_name.set_text("用户自定义提供商");
        bindings.provider_preset.set_selected(0);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            bindings.provider_name.text().as_str() == "用户自定义提供商"
        });
        assert_eq!(bindings.provider_name.text().as_str(), "用户自定义提供商");
        bindings.provider_endpoint.set_text(&demo_endpoint);
        bindings.locale.set_selected(0);
        spin_main_context_until(&context, Duration::from_secs(1), || {
            state.borrow().locale() == UiLocale::English
        });
        apply_worker_event(
            &bindings,
            &state,
            &worker,
            WorkerEvent::ProfileStorageUnavailable(TranslationError::new(
                ErrorKind::Persistence,
                "Saved profile storage is unavailable.",
            )),
        );
        refresh_ui(&bindings, &state.borrow());
        assert_eq!(
            state.borrow().onboarding_stage(),
            OnboardingStage::ConfigureProvider
        );
        assert!(
            bindings
                .onboarding_detail
                .label()
                .contains("Saved profile storage is unavailable")
        );
        assert!(!bindings.saved_profile.is_sensitive());
        assert!(!bindings.remember_profile.is_sensitive());
        assert!(bindings.connect.is_sensitive());

        bindings.provider_name.set_text("Unavailable provider");
        bindings.provider_endpoint.set_text("not a valid endpoint");
        bindings.connect.emit_clicked();
        assert_eq!(state.borrow().status(), AppStatus::Failed);
        assert!(state.borrow().active_provider().is_none());
        assert!(state.borrow().error_text().is_some());
        assert!(bindings.error.is_visible());
        assert!(!gtk::test_accessible_has_state(
            &bindings.error,
            gtk::AccessibleState::Hidden
        ));

        bindings.provider_name.set_text("GTK fake provider");
        bindings.provider_endpoint.set_text(&demo_endpoint);
        bindings
            .provider_credential
            .set_text("GTK_SESSION_CREDENTIAL_SENTINEL");
        bindings.connect.emit_clicked();
        assert_eq!(state.borrow().status(), AppStatus::Connecting);
        assert_eq!(
            state.borrow().onboarding_stage(),
            OnboardingStage::Connecting
        );
        assert_eq!(
            bindings.onboarding_title.label(),
            "Provider setup · Connecting"
        );
        assert!(
            bindings
                .onboarding_detail
                .label()
                .contains("GTK fake provider")
        );
        assert!(
            bindings
                .onboarding_detail
                .label()
                .contains("Saved profile storage is unavailable")
        );
        assert!(bindings.provider_credential.text().is_empty());
        assert!(!bindings.connect.is_sensitive());
        assert!(!bindings.open_source.is_sensitive());
        assert!(!bindings.translate.is_sensitive());

        spin_main_context_until(&context, Duration::from_secs(5), || {
            let state = state.borrow();
            state.status() == AppStatus::Ready
                && state
                    .active_provider()
                    .is_some_and(|profile| profile.display_name() == "GTK fake provider")
        });
        assert_eq!(
            state.borrow().onboarding_stage(),
            OnboardingStage::SelectModel
        );
        assert_eq!(
            bindings.onboarding_title.label(),
            "Provider setup · Step 2 of 2"
        );
        assert!(
            bindings
                .onboarding_detail
                .label()
                .contains("Saved profile storage is unavailable")
        );
        assert_eq!(state.borrow().selected_model(), None);
        assert!(!bindings.translate.is_sensitive());
        assert!(
            bindings
                .active_provider
                .label()
                .contains("GTK fake provider")
        );
        assert!(bindings.active_provider.label().contains("session only"));
        assert!(!bindings.error.is_visible());
        assert!(gtk::test_accessible_has_state(
            &bindings.error,
            gtk::AccessibleState::Hidden
        ));

        bindings.model.set_selected(1);
        assert_eq!(
            state.borrow().pending_model_selection(),
            Some("fake-translator")
        );
        assert!(!bindings.translate.is_sensitive());
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().selected_model() == Some("fake-translator")
        });
        let ready_identity = {
            let state = state.borrow();
            let profile_id = state.provider_id().expect("active provider ID");
            format!(
                "GTK fake provider [{}] · fake-translator",
                profile_id.as_str()
            )
        };
        assert_eq!(state.borrow().onboarding_stage(), OnboardingStage::Ready);
        assert_eq!(bindings.onboarding_title.label(), "Provider setup · Ready");
        assert!(bindings.onboarding_detail.label().contains(&ready_identity));
        assert!(
            bindings
                .onboarding_detail
                .label()
                .contains("Saved profile storage is unavailable")
        );
        assert!(bindings.translate.is_sensitive());

        bindings.model.set_selected(0);
        assert_eq!(bindings.model.selected(), 1);
        assert_eq!(state.borrow().selected_model(), Some("fake-translator"));
        assert!(bindings.translate.is_sensitive());

        let rejected_model = state.borrow().models()[1].id.clone();
        state
            .borrow_mut()
            .begin_model_selection(&rejected_model)
            .expect("begin rejected model selection");
        refresh_ui(&bindings, &state.borrow());
        assert_eq!(
            state.borrow().onboarding_stage(),
            OnboardingStage::SelectModel
        );
        assert_eq!(
            bindings.onboarding_title.label(),
            "Provider setup · Step 2 of 2"
        );
        assert!(
            bindings
                .onboarding_detail
                .label()
                .contains(&format!("Confirming model {rejected_model}"))
        );
        assert!(!bindings.translate.is_sensitive());
        bindings.model.set_selected(2);
        let profile_id = state
            .borrow()
            .provider_id()
            .cloned()
            .expect("active provider ID");
        apply_worker_event(
            &bindings,
            &state,
            &worker,
            WorkerEvent::ModelSelectionRejected {
                profile_id,
                model_id: rejected_model,
                error: TranslationError::new(
                    ErrorKind::Persistence,
                    "The saved model could not be updated.",
                ),
            },
        );
        refresh_ui(&bindings, &state.borrow());
        assert_eq!(bindings.model.selected(), 1);
        assert_eq!(state.borrow().selected_model(), Some("fake-translator"));
        assert!(state.borrow().pending_model_selection().is_none());
        assert_eq!(state.borrow().onboarding_stage(), OnboardingStage::Ready);
        assert!(bindings.onboarding_detail.label().contains(&ready_identity));

        bindings.provider_name.set_text("Unavailable provider");
        bindings.provider_endpoint.set_text("not a valid endpoint");
        bindings.connect.emit_clicked();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            let state = state.borrow();
            state.status() == AppStatus::Ready && state.error_text().is_some()
        });
        assert_eq!(
            state
                .borrow()
                .active_provider()
                .map(|profile| profile.display_name().to_owned()),
            Some("GTK fake provider".to_owned())
        );
        assert!(state.borrow().selected_model().is_some());
        assert_eq!(state.borrow().onboarding_stage(), OnboardingStage::Ready);
        assert!(bindings.onboarding_detail.label().contains(&ready_identity));

        bindings.source.set_text("Hello");
        bindings.translate.emit_clicked();
        assert_eq!(state.borrow().status(), AppStatus::Translating);
        assert!(!bindings.connect.is_sensitive());
        assert!(!bindings.translate.is_sensitive());
        assert!(gtk::test_accessible_has_state(
            &bindings.workspace,
            gtk::AccessibleState::Busy
        ));
        assert!(gtk::test_accessible_has_state(
            &bindings.output_view,
            gtk::AccessibleState::Busy
        ));

        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Completed
        });
        assert_eq!(state.borrow().output(), "你好，LinguaMesh！");
        assert!(!state.borrow().has_partial_output());
        assert_eq!(bindings.status.label(), "Status: Completed");
        assert!(!gtk::test_accessible_has_state(
            &bindings.workspace,
            gtk::AccessibleState::Busy
        ));
        assert!(!gtk::test_accessible_has_state(
            &bindings.output_view,
            gtk::AccessibleState::Busy
        ));
        assert_eq!(state.borrow().onboarding_stage(), OnboardingStage::Ready);
        assert!(bindings.onboarding_detail.label().contains(&ready_identity));
        assert!(
            bindings
                .onboarding_detail
                .label()
                .contains("Saved profile storage is unavailable")
        );
        assert!(!bindings.remember_profile.is_sensitive());
        assert!(bindings.translate.is_sensitive());
        assert!(!bindings.retry_translation.is_sensitive());
        state
            .borrow_mut()
            .record_operation_failure(TranslationError::new(
                ErrorKind::Network,
                "The provider could not be reached.",
            ));
        refresh_ui(&bindings, &state.borrow());
        assert!(bindings.retry_translation.is_sensitive());
        bindings.retry_translation.emit_clicked();
        assert_eq!(state.borrow().status(), AppStatus::Translating);
        spin_main_context_until(&context, Duration::from_secs(5), || {
            state.borrow().status() == AppStatus::Completed
        });
        assert_eq!(state.borrow().output(), "你好，LinguaMesh！");
        assert!(!bindings.retry_translation.is_sensitive());
        apply_worker_event(&bindings, &state, &worker, WorkerEvent::Stopped);
        refresh_ui(&bindings, &state.borrow());
        assert!(state.borrow().worker_unavailable());
        assert_eq!(
            state.borrow().onboarding_stage(),
            OnboardingStage::Unavailable
        );
        assert_eq!(
            bindings.onboarding_title.label(),
            "Provider setup · Unavailable"
        );
        assert_eq!(bindings.status.label(), "Status: Unavailable");
        assert!(!bindings.connect.is_sensitive());
        assert!(!bindings.translate.is_sensitive());
        assert!(!bindings.model.is_sensitive());
        assert!(!bindings.stop.is_sensitive());

        let restored_state = Rc::new(RefCell::new(AppState::default()));
        let restored_database_directory =
            std::env::temp_dir().join(format!("linguamesh-linux-gtk-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&restored_database_directory);
        let restored_worker = Rc::new(CoreWorker::spawn_with_database(
            restored_database_directory.join("state.sqlite3"),
        ));
        let (restored_window, restored_bindings, restored_theme, restored_locale) =
            create_window(&application);
        connect_selection_handlers(
            &restored_bindings,
            &restored_theme,
            &restored_locale,
            &restored_state,
            &restored_worker,
        );
        connect_action_handlers(&restored_bindings, &restored_state, &restored_worker);
        start_event_pump(&restored_bindings, &restored_state, &restored_worker);
        let restored_profile_a = custom_provider_profile(
            ProviderProfileId::parse("profile-a").expect("first profile ID"),
            "Restored provider A".to_owned(),
            CUSTOM_PROVIDER_PRESET_ID.to_owned(),
            OPENAI_ADAPTER_TYPE.to_owned(),
            "http://127.0.0.1:4242/v1/".to_owned(),
            None,
            Some("fake-translator".to_owned()),
        )
        .expect("first restored profile");
        let restored_profile_a_disabled = restored_profile_a.clone().with_enabled(false);
        let restored_profile_b = custom_provider_profile(
            ProviderProfileId::parse("profile-b").expect("second profile ID"),
            "Restored provider B".to_owned(),
            CUSTOM_PROVIDER_PRESET_ID.to_owned(),
            OPENAI_ADAPTER_TYPE.to_owned(),
            "http://127.0.0.1:4243/v1/".to_owned(),
            Some(SecretRef::new(SecretRefNamespace::SecretService)),
            Some("fake-slow-translator".to_owned()),
        )
        .expect("second restored profile");
        restored_window.present();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            restored_state.borrow().worker_ready()
        });
        apply_worker_event(
            &restored_bindings,
            &restored_state,
            &restored_worker,
            WorkerEvent::ProfilesRestored {
                profiles: vec![restored_profile_b.clone(), restored_profile_a.clone()],
                active_profile_id: Some(restored_profile_b.id().clone()),
            },
        );
        refresh_ui(&restored_bindings, &restored_state.borrow());
        assert_eq!(restored_state.borrow().status(), AppStatus::Disconnected);
        assert_eq!(
            restored_state.borrow().onboarding_stage(),
            OnboardingStage::ConfigureProvider
        );
        assert!(
            restored_bindings
                .onboarding_detail
                .label()
                .contains("Restored profiles never connect automatically")
        );
        assert!(restored_state.borrow().active_provider().is_none());
        assert_eq!(restored_state.borrow().saved_profiles().len(), 2);
        assert_eq!(
            restored_state
                .borrow()
                .selected_saved_profile_id()
                .map(ProviderProfileId::as_str),
            Some("profile-b")
        );
        assert_eq!(restored_bindings.saved_profile.selected(), 2);
        assert_eq!(
            restored_bindings.provider_name.text(),
            "Restored provider B"
        );
        assert_eq!(
            restored_bindings.provider_endpoint.text(),
            "http://127.0.0.1:4243/v1/"
        );
        assert!(restored_bindings.remember_profile.is_active());
        assert_eq!(restored_bindings.model.selected(), 0);
        let candidate_a = RoutingCandidate::new("profile-a", "fake-translator", true, 64 * 1024)
            .expect("first routing candidate");
        let candidate_b =
            RoutingCandidate::new("profile-b", "fake-slow-translator", true, 64 * 1024)
                .expect("second routing candidate");
        let routing_profile = RoutingProfile::new(
            "gtk-routing-fixture",
            RoutingMode::Ordered,
            vec![candidate_b, candidate_a],
            RoutingConstraints::default(),
        )
        .expect("routing profile");
        show_routing_profiles_dialog(
            &restored_bindings,
            &restored_state,
            &restored_worker.command_handle(),
            vec![RoutingProfileRecord {
                id: "gtk-routing-fixture".to_owned(),
                profile: routing_profile,
                created_at: 0,
                updated_at: 0,
            }],
        );
        spin_main_context_until(&context, Duration::from_secs(1), || {
            application
                .windows()
                .iter()
                .any(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
        });
        let routing_dialog = application
            .windows()
            .into_iter()
            .find(|candidate| candidate.title().as_deref() == Some("Routing profiles"))
            .expect("routing profile dialog");
        let routing_widgets = descendant_widgets(routing_dialog.upcast_ref::<gtk::Widget>());
        let movement_tooltips = routing_widgets
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Button>())
            .filter_map(gtk::prelude::WidgetExt::tooltip_text)
            .collect::<Vec<_>>();
        assert!(
            movement_tooltips
                .iter()
                .any(|text| text == "Move candidate up")
        );
        assert!(
            movement_tooltips
                .iter()
                .any(|text| text == "Move candidate down")
        );
        let candidate_labels = || {
            descendant_widgets(routing_dialog.upcast_ref::<gtk::Widget>())
                .iter()
                .filter_map(|widget| widget.downcast_ref::<gtk::CheckButton>())
                .filter_map(gtk::prelude::CheckButtonExt::label)
                .map(|label| label.to_string())
                .filter(|label| label.starts_with("Restored provider "))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            candidate_labels(),
            vec![
                "Restored provider A · fake-translator",
                "Restored provider B · fake-slow-translator",
            ]
        );
        let candidate_a_row = descendant_widgets(routing_dialog.upcast_ref::<gtk::Widget>())
            .iter()
            .filter_map(|widget| widget.downcast_ref::<gtk::Box>())
            .find(|row| {
                row.first_child()
                    .and_then(|child| child.downcast::<gtk::CheckButton>().ok())
                    .and_then(|check| check.label())
                    .is_some_and(|label| label == "Restored provider A · fake-translator")
            })
            .cloned()
            .expect("first routing candidate row");
        let candidate_a_check = candidate_a_row
            .first_child()
            .and_then(|child| child.downcast::<gtk::CheckButton>().ok())
            .expect("candidate checkbox");
        let candidate_a_up = candidate_a_check
            .next_sibling()
            .and_then(|child| child.downcast::<gtk::Button>().ok())
            .expect("candidate up button");
        let candidate_a_down = candidate_a_up
            .next_sibling()
            .and_then(|child| child.downcast::<gtk::Button>().ok())
            .expect("candidate down button");
        candidate_a_down.emit_clicked();
        assert_eq!(
            candidate_labels(),
            vec![
                "Restored provider B · fake-slow-translator",
                "Restored provider A · fake-translator",
            ]
        );
        candidate_a_up.emit_clicked();
        assert_eq!(
            candidate_labels(),
            vec![
                "Restored provider A · fake-translator",
                "Restored provider B · fake-slow-translator",
            ]
        );
        routing_dialog.close();
        apply_worker_event(
            &restored_bindings,
            &restored_state,
            &restored_worker,
            WorkerEvent::ProfilesRestored {
                profiles: vec![restored_profile_b.clone(), restored_profile_a_disabled],
                active_profile_id: Some(restored_profile_b.id().clone()),
            },
        );
        refresh_ui(&restored_bindings, &restored_state.borrow());
        apply_worker_event(
            &restored_bindings,
            &restored_state,
            &restored_worker,
            WorkerEvent::DemoProviderReady {
                endpoint: "http://127.0.0.1:4343/v1/".to_owned(),
            },
        );
        assert_eq!(
            restored_bindings.provider_endpoint.text(),
            "http://127.0.0.1:4243/v1/"
        );
        assert!(restored_bindings.remove_saved_profile.is_sensitive());
        let restored_snapshot = restored_state.borrow().saved_profiles().to_vec();
        restored_bindings.connect.emit_clicked();
        assert_eq!(restored_state.borrow().status(), AppStatus::Connecting);
        spin_main_context_until(&context, Duration::from_secs(5), || {
            restored_state.borrow().status() == AppStatus::Failed
        });
        assert!(restored_state.borrow().error_text().is_some_and(|error| {
            error.starts_with("Secure storage unavailable:")
                || error.starts_with("Secret unavailable:")
        }));
        assert!(restored_state.borrow().active_provider().is_none());
        assert!(restored_state.borrow().pending_provider().is_none());
        assert!(restored_state.borrow().models().is_empty());
        assert_eq!(
            restored_state.borrow().saved_profiles(),
            restored_snapshot.as_slice()
        );

        restored_bindings.saved_profile.set_selected(1);
        assert_eq!(
            restored_state
                .borrow()
                .selected_saved_profile_id()
                .map(ProviderProfileId::as_str),
            Some("profile-a")
        );
        assert!(restored_state.borrow().active_provider().is_none());
        assert_eq!(
            restored_bindings.provider_name.text(),
            "Restored provider A"
        );
        assert_eq!(
            restored_state.borrow().persisted_active_profile_id(),
            Some(restored_profile_b.id())
        );
        assert!(
            !restored_state
                .borrow()
                .selected_saved_profile()
                .unwrap()
                .enabled()
        );
        restored_bindings.connect.emit_clicked();
        assert_eq!(restored_state.borrow().status(), AppStatus::Failed);
        assert!(restored_state.borrow().pending_provider().is_none());
        assert_eq!(
            restored_state.borrow().error_text().as_deref(),
            Some("Invalid configuration: The selected provider profile is disabled.")
        );
        assert_eq!(
            restored_state.borrow().saved_profiles(),
            restored_snapshot.as_slice()
        );

        restored_state
            .borrow_mut()
            .begin_profile_deletion(restored_profile_a.id())
            .expect("begin profile removal");
        refresh_ui(&restored_bindings, &restored_state.borrow());
        assert_eq!(
            restored_bindings.status.label(),
            "Status: Removing saved profile"
        );
        assert!(!restored_bindings.saved_profile.is_sensitive());
        apply_worker_event(
            &restored_bindings,
            &restored_state,
            &restored_worker,
            WorkerEvent::ProfileDeleted {
                profile_id: restored_profile_a.id().clone(),
            },
        );
        refresh_ui(&restored_bindings, &restored_state.borrow());
        assert_eq!(restored_state.borrow().saved_profiles().len(), 1);
        assert!(restored_state.borrow().selected_saved_profile().is_none());
        assert_eq!(restored_bindings.saved_profile.selected(), 0);
        assert_eq!(
            restored_bindings.provider_name.text(),
            DEFAULT_PROVIDER_NAME
        );
        let draft_profile_id = restored_bindings
            .draft_profile_id
            .borrow()
            .clone()
            .expect("new draft profile ID");
        assert!(draft_profile_id.as_str().starts_with("profile-"));
        assert_ne!(&draft_profile_id, restored_profile_b.id());

        let generated_a = generate_custom_provider_id(restored_state.borrow().saved_profiles())
            .expect("first generated ID");
        let generated_b = generate_custom_provider_id(restored_state.borrow().saved_profiles())
            .expect("second generated ID");
        assert_ne!(generated_a, generated_b);
        assert!(generated_a.as_str().starts_with("profile-"));
        apply_worker_event(
            &restored_bindings,
            &restored_state,
            &restored_worker,
            WorkerEvent::ProfileStorageUnavailable(TranslationError::new(
                ErrorKind::Persistence,
                "Saved profile storage is unavailable.",
            )),
        );
        refresh_ui(&restored_bindings, &restored_state.borrow());
        assert_eq!(
            restored_state.borrow().onboarding_stage(),
            OnboardingStage::ConfigureProvider
        );
        assert!(
            restored_bindings
                .onboarding_detail
                .label()
                .contains("Saved profile storage is unavailable")
        );
        apply_worker_event(
            &restored_bindings,
            &restored_state,
            &restored_worker,
            WorkerEvent::Stopped,
        );
        refresh_ui(&restored_bindings, &restored_state.borrow());
        assert!(restored_state.borrow().worker_unavailable());
        assert_eq!(
            restored_state.borrow().onboarding_stage(),
            OnboardingStage::Unavailable
        );
        assert_eq!(
            restored_bindings.onboarding_title.label(),
            "Provider setup · Unavailable"
        );
        assert_eq!(restored_bindings.status.label(), "Status: Unavailable");
        assert!(!restored_bindings.connect.is_sensitive());
        run_gtk_native_ollama_preset_flow(&application);
        let _ = restored_worker.try_send(WorkerCommand::Shutdown);
        restored_window.close();
        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        let _ = fs::remove_dir_all(restored_database_directory);
    }

    // 验证 Linux 客户端沿用桌面高对比度和减少动画设置，不覆盖用户的系统偏好。
    #[ignore = "run in dedicated serialized GTK fixture"]
    #[test]
    fn gtk_accessibility_preferences_follow_desktop_settings() {
        adw::init().expect("initialize GTK and libadwaita");
        let application = adw::Application::builder()
            .application_id("dev.linguamesh.LinguaMesh.AccessibilityPreferencesTest")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        application
            .register(None::<&gtk::gio::Cancellable>)
            .expect("register GTK accessibility test application");

        let (window, bindings, theme, locale) = create_window(&application);
        let settings = gtk::Settings::default().expect("GTK settings");
        let previous_theme = settings.gtk_theme_name().map(|value| value.to_string());
        let previous_animations = settings.is_gtk_enable_animations();
        let previous_font = settings.gtk_font_name().map(|value| value.to_string());
        settings.set_gtk_theme_name(Some("HighContrast"));
        settings.set_gtk_enable_animations(false);
        settings.set_gtk_font_name(Some("Sans 24"));

        let display = gtk::prelude::RootExt::display(&window);
        let manager = adw::StyleManager::for_display(&display);
        assert!(
            manager.is_high_contrast(),
            "libadwaita did not detect the desktop high-contrast theme"
        );
        assert!(
            !adw::is_animations_enabled(window.upcast_ref::<gtk::Widget>()),
            "libadwaita did not follow the desktop reduced-motion setting"
        );
        let title_context = bindings.onboarding_title.pango_context();
        title_context.changed();
        let title_font = title_context
            .font_description()
            .expect("Pango title font description");
        assert!(
            title_font.size() >= 24 * gtk::pango::SCALE,
            "GTK text scaling did not reach the title Pango context: {}",
            title_font.size()
        );

        settings.set_gtk_theme_name(previous_theme.as_deref());
        settings.set_gtk_enable_animations(previous_animations);
        settings.set_gtk_font_name(previous_font.as_deref());
        window.close();
        drop(bindings);
        drop(theme);
        drop(locale);
    }
}
