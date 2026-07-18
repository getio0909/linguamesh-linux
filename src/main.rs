use adw::prelude::*;
use gtk::glib;
use linguamesh_document::{
    DEFAULT_SUBTITLE_MAX_LINE_CHARS, DEFAULT_SUBTITLE_MAX_READING_SPEED, DocumentJobState,
    DocumentWarning, DocumentWarningKind,
};
use linguamesh_domain::{
    ErrorKind, Glossary, GlossaryEntry, MAX_GLOSSARY_CSV_BYTES, OperationId, ProviderProfileId,
    SecretRef, SecretRefNamespace, SecretValue, TranslationError, TranslationEvent,
    TranslationPrivacyMode,
};
use linguamesh_linux::file_import;
use linguamesh_linux::localization;
use linguamesh_linux::model::{
    AppState, AppStatus, OnboardingStage, ProfileStorageStatus, ProviderProfile, StateError,
    ThemePreference, UiLocale,
};
use linguamesh_linux::secret_service;
use linguamesh_linux::worker::{
    CoreWorker, PersistenceIntent, WorkerCommand, WorkerCommandHandle, WorkerEvent,
};
use linguamesh_storage::{DocumentJobSnapshot, TranslationHistoryEntry, TranslationMemoryEntry};
use std::cell::{Cell, RefCell};
use std::fs;
use std::io::Write;
use std::rc::Rc;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

const SOURCE_LOCALES: [Option<&str>; 3] = [None, Some("en"), Some("zh-CN")];
const TARGET_LOCALES: [&str; 3] = ["zh-CN", "en", "ja"];
const MAX_EVENTS_PER_TICK: usize = 64;
const PROFILE_ID_GENERATION_ATTEMPTS: usize = 8;
const CUSTOM_PROVIDER_PRESET_ID: &str = "custom-openai-compatible";
const OPENAI_ADAPTER_TYPE: &str = "openai_chat_completions";
const DEFAULT_PROVIDER_NAME: &str = "Local OpenAI-compatible provider";
const DEFAULT_PROVIDER_ENDPOINT: &str = "http://127.0.0.1:11434/v1/";

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
    provider_name: gtk::Entry,
    provider_endpoint: gtk::Entry,
    provider_credential: gtk::PasswordEntry,
    remember_profile: gtk::CheckButton,
    remove_saved_profile: gtk::Button,
    connect: gtk::Button,
    active_provider: gtk::Label,
    model: gtk::DropDown,
    source_locale: gtk::DropDown,
    target_locale: gtk::DropDown,
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
    translate: gtk::Button,
    export_output: gtk::Button,
    open_output: gtk::Button,
    open_source: gtk::Button,
    document_jobs: gtk::Button,
    stop: gtk::Button,
    pause_document: gtk::Button,
    resume_document: gtk::Button,
    retry_document: gtk::Button,
    status: gtk::Label,
    partial: gtk::Label,
    error: gtk::Label,
    locale_note: gtk::Label,
    diagnostics_panel: gtk::Expander,
    diagnostics: gtk::Label,
    profile_selection_guard: Rc<Cell<bool>>,
    draft_profile_id: Rc<RefCell<Option<ProviderProfileId>>>,
    source_uri: Rc<RefCell<Option<String>>>,
    output_uri: Rc<RefCell<Option<String>>>,
    fallback_profile_ids: Rc<RefCell<Vec<Option<ProviderProfileId>>>>,
    document_job_id: Rc<RefCell<Option<String>>>,
    document_job_guard: Rc<Cell<bool>>,
    document_job_state: Rc<Cell<Option<DocumentJobState>>>,
    document_progress: Rc<Cell<Option<(usize, usize)>>>,
    document_warnings: Rc<RefCell<Vec<DocumentWarning>>>,
    export_notice: Rc<Cell<bool>>,
    fallback_notice: Rc<Cell<bool>>,
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

struct EditorBindings {
    editors: gtk::Paned,
    source: gtk::TextBuffer,
    output: gtk::TextBuffer,
    source_view: gtk::TextView,
    output_view: gtk::TextView,
    source_label: gtk::Label,
    output_label: gtk::Label,
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

fn build_ui(application: &adw::Application, file_dialog_fixture: bool, file_drop_fixture: bool) {
    if let Some(window) = application.active_window() {
        window.present();
        return;
    }
    let state = Rc::new(RefCell::new(AppState::default()));
    let database_path = glib::user_data_dir()
        .join("dev.linguamesh.LinguaMesh")
        .join("linguamesh.sqlite3");
    let worker = Rc::new(CoreWorker::spawn_with_database(database_path));
    let (window, bindings, theme, locale) = create_window(application);
    connect_selection_handlers(&bindings, &theme, &locale, &state, &worker);
    connect_action_handlers(&bindings, &state, &worker);
    start_event_pump(&bindings, &state, &worker);

    let shutdown_worker = Rc::clone(&worker);
    window.connect_destroy(move |_| {
        let _ = shutdown_worker.try_send(WorkerCommand::Shutdown);
    });
    refresh_ui(&bindings, &state.borrow());
    window.present();
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
    let drag_button = gtk::Button::with_label("Drag fixture");
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
        provider_name,
        provider_endpoint,
        provider_credential,
        remember_profile,
        remove_saved_profile,
        connect,
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
    let document_jobs = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.document_jobs",
        "Document jobs",
    ));
    document_jobs.set_focusable(true);
    document_jobs.set_tooltip_text(Some(&localization::text(
        display_locale,
        "tooltip.document_jobs",
        "View and select persisted document jobs",
    )));
    let translate = gtk::Button::with_mnemonic(&localized_mnemonic(
        display_locale,
        "action.translate",
        "Translate",
    ));
    translate.add_css_class("suggested-action");
    translate.set_focusable(true);
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
    action_row.append(&document_jobs);
    action_row.append(&translate);
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
        provider_name,
        provider_endpoint,
        provider_credential,
        remember_profile,
        remove_saved_profile,
        connect,
        active_provider,
        model,
        source_locale,
        target_locale,
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
        translate,
        export_output,
        open_output,
        open_source,
        document_jobs,
        stop,
        pause_document,
        resume_document,
        retry_document,
        status,
        partial,
        error,
        locale_note,
        diagnostics_panel,
        diagnostics,
        profile_selection_guard: Rc::new(Cell::new(false)),
        draft_profile_id: Rc::new(RefCell::new(None)),
        source_uri: Rc::new(RefCell::new(None)),
        output_uri: Rc::new(RefCell::new(None)),
        fallback_profile_ids: Rc::new(RefCell::new(vec![None])),
        document_job_id: Rc::new(RefCell::new(None)),
        document_job_guard: Rc::new(Cell::new(false)),
        document_job_state: Rc::new(Cell::new(None)),
        document_progress: Rc::new(Cell::new(None)),
        document_warnings: Rc::new(RefCell::new(Vec::new())),
        export_notice: Rc::new(Cell::new(false)),
        fallback_notice: Rc::new(Cell::new(false)),
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
    let focus_start_path = std::env::var_os("LINGUAMESH_KEYBOARD_FOCUS_START");
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
            let _ = writeln!(log, "__ready__");
            let _ = log.flush();
            ready_logged.set(true);
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
        }
        gtk::prelude::GtkWindowExt::set_focus(&focus_window, Some(&initial_focus));
        let grabbed = initial_focus.grab_focus();
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
    let (source_panel, source_label) = editor_panel(&source_label, &source_view);
    let (output_panel, output_label) = editor_panel(&output_label, &output_view);
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
    }
}

#[allow(clippy::too_many_lines)]
fn create_provider_session() -> (
    gtk::Box,
    gtk::DropDown,
    gtk::Entry,
    gtk::Entry,
    gtk::PasswordEntry,
    gtk::CheckButton,
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
    fields.append(&labeled_control(
        &localized_mnemonic(
            locale,
            "label.credential",
            "Credential (optional; secure when remembered)",
        ),
        provider_credential.upcast_ref::<gtk::Widget>(),
    ));
    fields.append(&connect);
    section.append(&fields);
    section.append(&remember_profile);
    let focus_order = vec![
        saved_profile.clone().upcast::<gtk::Widget>(),
        remove_saved_profile.clone().upcast::<gtk::Widget>(),
        provider_name.clone().upcast::<gtk::Widget>(),
        provider_endpoint.clone().upcast::<gtk::Widget>(),
        provider_credential.clone().upcast::<gtk::Widget>(),
        connect.clone().upcast::<gtk::Widget>(),
        remember_profile.clone().upcast::<gtk::Widget>(),
    ];
    install_provider_focus_traversal(&focus_order);

    let active_provider = gtk::Label::new(None);
    active_provider.set_xalign(0.0);
    active_provider.set_wrap(true);
    section.append(&active_provider);
    (
        section,
        saved_profile,
        provider_name,
        provider_endpoint,
        provider_credential,
        remember_profile,
        remove_saved_profile,
        connect,
        active_provider,
        title,
        note,
    )
}

// 为 provider 表单提供稳定的 Tab 与 Shift+Tab 焦点顺序。
fn install_provider_focus_traversal(focus_order: &[gtk::Widget]) {
    for widget in focus_order {
        let order = focus_order.to_owned();
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        controller.connect_key_pressed(move |_, key, _, state| {
            if key != gtk::gdk::Key::Tab {
                return gtk::glib::Propagation::Proceed;
            }
            let reverse = state.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            let Some(current) = order.iter().position(gtk::prelude::WidgetExt::has_focus) else {
                return gtk::glib::Propagation::Proceed;
            };
            let step: isize = if reverse { -1 } else { 1 };
            let mut next = current.cast_signed() + step;
            while let Ok(index) = usize::try_from(next) {
                let Some(widget) = order.get(index) else {
                    break;
                };
                if widget.is_visible()
                    && widget.is_sensitive()
                    && widget.is_focusable()
                    && widget.grab_focus()
                {
                    return gtk::glib::Propagation::Stop;
                }
                next += step;
            }
            gtk::glib::Propagation::Proceed
        });
        widget.add_controller(controller);
    }
}

#[allow(clippy::too_many_lines, clippy::type_complexity)]
fn create_controls() -> (
    gtk::Box,
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
    (
        controls,
        model,
        source_locale,
        target_locale,
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
                    "The imported glossary is no longer available.",
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
    bindings.provider_name.set_text(profile.display_name());
    bindings.provider_endpoint.set_text(profile.base_endpoint());
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
    bindings.provider_name.set_text(DEFAULT_PROVIDER_NAME);
    bindings
        .provider_endpoint
        .set_text(DEFAULT_PROVIDER_ENDPOINT);
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

fn editor_panel(label: &str, editor: &gtk::TextView) -> (gtk::Box, gtk::Label) {
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
    container.append(&label);
    container.append(&scroller);
    (container, label)
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

fn connect_selection_handlers(
    bindings: &UiBindings,
    theme: &gtk::DropDown,
    locale: &gtk::DropDown,
    state: &Rc<RefCell<AppState>>,
    worker: &Rc<CoreWorker>,
) {
    connect_profile_selection_handler(bindings, state);

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
        let (profile_id, preset_id, adapter_type, saved_secret_ref, enabled, selected_model) =
            match state.selected_saved_profile() {
                Some(saved) => (
                    Ok(saved.id().clone()),
                    saved.preset_id().to_owned(),
                    saved.adapter_type().to_owned(),
                    saved.secret_ref().cloned(),
                    saved.enabled(),
                    saved.selected_model().map(str::to_owned),
                ),
                None => (
                    ensure_draft_profile_id(&connect_bindings, &state),
                    CUSTOM_PROVIDER_PRESET_ID.to_owned(),
                    OPENAI_ADAPTER_TYPE.to_owned(),
                    None,
                    true,
                    None,
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
                                let command = WorkerCommand::TranslateDocumentJob {
                                    job_id,
                                    source_locale,
                                    target_locale,
                                    glossary,
                                    privacy_mode: state.privacy_mode(),
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
                } else if fallback_enabled && fallback_profile_id.is_none() {
                    let message = localization::text(
                        state.locale(),
                        "error.fallback_profile_required",
                        "Choose an approved saved fallback provider or turn fallback off.",
                    );
                    state.record_client_error(message);
                } else {
                    match state.begin_translation() {
                        Ok(request) => {
                            let command = fallback_profile_id.map_or(
                                WorkerCommand::Translate(request.clone()),
                                |fallback_profile_id| WorkerCommand::TranslateWithFallback {
                                    request,
                                    fallback_profile_id,
                                },
                            );
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

    let pause_bindings = bindings.clone();
    let pause_worker = Rc::clone(worker);
    bindings.pause_document.connect_clicked(move |_| {
        let Some(job_id) = pause_bindings.document_job_id.borrow().clone() else {
            return;
        };
        if let Err(error) = pause_worker.try_send(WorkerCommand::PauseDocumentJob { job_id }) {
            pause_bindings.error.set_label(&error.to_string());
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
    let contents = glib::Bytes::from_owned(glossary.to_csv().into_bytes());
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
                file.replace_contents_bytes_async(
                    &contents,
                    None,
                    false,
                    gtk::gio::FileCreateFlags::NONE,
                    None::<&gtk::gio::Cancellable>,
                    move |write_result| match write_result {
                        Ok(_) => {
                            callback_bindings.glossary_notice.set(true);
                            refresh_ui(&callback_bindings, &callback_state.borrow());
                        }
                        Err(_) => show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.glossary_export",
                                "The glossary CSV could not be saved.",
                            ),
                        ),
                    },
                );
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

// 将译文异步写入用户选择的新文件，并拒绝覆盖已导入的源文件。
fn begin_translation_export(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &Rc<CoreWorker>,
) {
    let locale = state.borrow().locale();
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
    dialog.set_initial_name(Some("linguamesh-translation.txt"));
    let export_bindings = bindings.clone();
    let export_state = Rc::clone(state);
    let source_uri = bindings.source_uri.borrow().clone();
    dialog.save(
        Some(&bindings.window),
        None::<&gtk::gio::Cancellable>,
        move |result| match result {
            Ok(file) => {
                let destination_uri = file.uri().to_string();
                if source_uri.as_deref() == Some(destination_uri.as_str()) {
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
                let contents = glib::Bytes::from_owned(output.into_bytes());
                let output_uri = destination_uri.clone();
                let callback_bindings = export_bindings.clone();
                let callback_state = Rc::clone(&export_state);
                file.replace_contents_bytes_async(
                    &contents,
                    None,
                    false,
                    gtk::gio::FileCreateFlags::NONE,
                    None::<&gtk::gio::Cancellable>,
                    move |write_result| match write_result {
                        Ok(_) => {
                            *callback_bindings.output_uri.borrow_mut() = Some(output_uri.clone());
                            callback_bindings.export_notice.set(true);
                            refresh_ui(&callback_bindings, &callback_state.borrow());
                        }
                        Err(_) => show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.file_export",
                                "The translated output could not be saved.",
                            ),
                        ),
                    },
                );
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
    contents: Vec<u8>,
) {
    let locale = state.borrow().locale();
    let extension = source_name
        .rsplit_once('.')
        .map_or("txt", |(_, extension)| extension);
    let default_name = format!("linguamesh-translation.{extension}");
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
                let destination_uri = file.uri().to_string();
                if source_uri.as_deref() == Some(destination_uri.as_str()) {
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
                let bytes = glib::Bytes::from_owned(contents);
                let output_uri = destination_uri.clone();
                let callback_bindings = export_bindings.clone();
                let callback_state = Rc::clone(&export_state);
                file.replace_contents_bytes_async(
                    &bytes,
                    None,
                    false,
                    gtk::gio::FileCreateFlags::NONE,
                    None::<&gtk::gio::Cancellable>,
                    move |write_result| match write_result {
                        Ok(_) => {
                            *callback_bindings.output_uri.borrow_mut() = Some(output_uri.clone());
                            callback_bindings.export_notice.set(true);
                            refresh_ui(&callback_bindings, &callback_state.borrow());
                        }
                        Err(_) => show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.file_export",
                                "The translated output could not be saved.",
                            ),
                        ),
                    },
                );
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
#[allow(clippy::too_many_lines)]
fn load_source_file(
    file: &gtk::gio::File,
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &Rc<CoreWorker>,
) {
    let bytes_read = Rc::new(Cell::new(0_usize));
    let too_large = Rc::new(Cell::new(false));
    let source_uri = file.uri().to_string();
    let source_name = file.basename().map_or_else(
        || "source.txt".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let read_bytes = Rc::clone(&bytes_read);
    let read_too_large = Rc::clone(&too_large);
    let load_bindings = bindings.clone();
    let load_state = Rc::clone(state);
    let load_worker = Rc::clone(worker);
    load_bindings.export_notice.set(false);
    *load_bindings.output_uri.borrow_mut() = None;
    file.load_partial_contents_async(
        None::<&gtk::gio::Cancellable>,
        move |chunk| {
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
                    match file_import::decode_document_job(&source_name, contents.as_ref()) {
                    Ok(job) => {
                        let warnings = job.warnings().unwrap_or_default();
                        if std::env::var_os("LINGUAMESH_TEST_FILE_DIALOG").is_some() {
                            println!("GTK file chooser application fixture completed the asynchronous GIO read.");
                        }
                        let text = job.source_text();
                        let job_id = OperationId::new().as_str().to_owned();
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
                        *load_bindings.source_uri.borrow_mut() = Some(source_uri.clone());
                        load_bindings.error.set_label("");
                        load_bindings.error.set_visible(false);
                    }
                    Err(error) => {
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
                            file_import::TextImportError::InvalidStructure => (
                                "error.file_open",
                                "The selected document structure is invalid.",
                            ),
                        };
                        let message = localization::text(locale, key, fallback);
                        show_file_import_error(&load_bindings, &message);
                    }
                    }
                }
                Err(_) => show_file_import_error(
                    &load_bindings,
                    &localization::text(
                        load_state.borrow().locale(),
                        "error.file_read",
                        "The selected text file could not be read.",
                    ),
                ),
            }
            refresh_ui(&load_bindings, &load_state.borrow());
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

// 展示持久化文档任务并允许用户选择当前编辑器绑定的任务。
#[allow(clippy::too_many_lines)]
fn show_document_jobs_dialog(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
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
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_vexpand(true);
    if jobs.is_empty() {
        let empty = gtk::Label::new(Some(&localization::text(
            locale,
            "status.document_jobs_empty",
            "No persisted document jobs are available.",
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
            let metadata = gtk::Label::new(Some(&format!(
                "{} · {:?} · {:?} · {completed}/{total}",
                snapshot.job.source_name, snapshot.job.format, snapshot.state,
            )));
            metadata.set_xalign(0.0);
            metadata.set_hexpand(true);
            metadata.add_css_class("dim-label");
            let select = gtk::Button::with_mnemonic(&localized_mnemonic(
                locale,
                "action.select_document_job",
                "Select",
            ));
            select.set_focusable(true);
            let select_dialog = dialog.clone();
            let select_bindings = bindings.clone();
            let select_state = Rc::clone(state);
            let selected = snapshot.clone();
            select.connect_clicked(move |_| {
                *select_bindings.document_job_id.borrow_mut() = Some(selected.job_id.clone());
                select_bindings.document_job_state.set(Some(selected.state));
                select_bindings
                    .document_progress
                    .set(Some(document_progress(&selected)));
                let source_text = selected.job.source_text();
                select_bindings.document_job_guard.set(true);
                select_bindings.source.set_text(&source_text);
                select_bindings.document_job_guard.set(false);
                *select_bindings.document_warnings.borrow_mut() =
                    selected.job.warnings().unwrap_or_default();
                select_state.borrow_mut().set_source_text(&source_text);
                select_dialog.close();
                refresh_ui(&select_bindings, &select_state.borrow());
            });
            header.append(&metadata);
            header.append(&select);
            row.append(&header);
            let id = gtk::Label::new(Some(&format!("Job: {}", snapshot.job_id)));
            id.set_xalign(0.0);
            id.add_css_class("dim-label");
            row.append(&id);
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
            let metadata = gtk::Label::new(Some(&format!(
                "{} → {} · {} · {}",
                entry.source_locale.as_deref().unwrap_or("auto"),
                entry.target_locale,
                entry.model_id,
                entry.created_at,
            )));
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
            let source = gtk::Label::new(Some(&format!("Source: {}", entry.source_text)));
            source.set_xalign(0.0);
            source.set_wrap(true);
            source.set_selectable(true);
            row.append(&source);
            let translated =
                gtk::Label::new(Some(&format!("Translation: {}", entry.translated_text)));
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
    let contents = glib::Bytes::from_owned(contents.into_bytes());
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
                file.replace_contents_bytes_async(
                    &contents,
                    None,
                    false,
                    gtk::gio::FileCreateFlags::NONE,
                    None::<&gtk::gio::Cancellable>,
                    move |write_result| match write_result {
                        Ok(_) => {
                            callback_bindings.history_export_notice.set(true);
                            refresh_ui(&callback_bindings, &callback_state.borrow());
                        }
                        Err(_) => show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.history_export",
                                "The translation history could not be saved.",
                            ),
                        ),
                    },
                );
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
            let metadata = gtk::Label::new(Some(&format!(
                "{} → {} · {} · {}",
                entry.source_locale.as_deref().unwrap_or("auto"),
                entry.target_locale,
                entry.model_id,
                entry.created_at,
            )));
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
            let source = gtk::Label::new(Some(&format!("Source: {}", entry.source_text)));
            source.set_xalign(0.0);
            source.set_wrap(true);
            source.set_selectable(true);
            row.append(&source);
            let translated =
                gtk::Label::new(Some(&format!("Translation: {}", entry.translated_text)));
            translated.set_xalign(0.0);
            translated.set_wrap(true);
            translated.set_selectable(true);
            row.append(&translated);
            let identity = gtk::Label::new(Some(&format!("Identity: {}", entry.identity_json)));
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
    let contents = glib::Bytes::from_owned(contents.into_bytes());
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
                file.replace_contents_bytes_async(
                    &contents,
                    None,
                    false,
                    gtk::gio::FileCreateFlags::NONE,
                    None::<&gtk::gio::Cancellable>,
                    move |write_result| match write_result {
                        Ok(_) => {
                            callback_bindings.memory_export_notice.set(true);
                            refresh_ui(&callback_bindings, &callback_state.borrow());
                        }
                        Err(_) => show_file_export_error(
                            &callback_bindings,
                            &localization::text(
                                callback_state.borrow().locale(),
                                "error.memory_export",
                                "The translation memory could not be saved.",
                            ),
                        ),
                    },
                );
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
        | WorkerEvent::DocumentJobActionRejected(error)
        | WorkerEvent::DocumentJobStorageUnavailable(error) => {
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
            }
        }
        WorkerEvent::DocumentJobsListed { jobs } => {
            show_document_jobs_dialog(bindings, state, jobs);
        }
        WorkerEvent::DocumentJobExported {
            source_name,
            contents,
        } => {
            begin_document_binary_export(bindings, state, &source_name, contents);
        }
        WorkerEvent::DocumentJobUpdated(snapshot) => {
            *bindings.document_job_id.borrow_mut() = Some(snapshot.job_id.clone());
            bindings.document_job_state.set(Some(snapshot.state));
            bindings
                .document_progress
                .set(Some(document_progress(&snapshot)));
            *bindings.document_warnings.borrow_mut() = snapshot.job.warnings().unwrap_or_default();
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
            let mut labels = vec!["Select a model...".to_owned()];
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
        WorkerEvent::FallbackSelected { .. } => {
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
            let template = localization::text(
                state.locale(),
                "provider.active",
                "Active provider: {provider}",
            );
            let active_label = template.replace("{provider}", active.display_name());
            bindings
                .active_provider
                .set_label(&format!("{active_label} ({active_mode})"));
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
                || "Unavailable".to_owned(),
                |profile| format!("{} [{}]", profile.display_name(), profile.id().as_str()),
            );
            let model = state.selected_model().unwrap_or("Unavailable");
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
    let translate = localization::text(locale, "action.translate", "Translate");
    let stop = localization::text(locale, "accessibility.stop_translation", "Stop translation");
    let pause = localization::text(locale, "action.pause_document", "Pause document");
    let resume = localization::text(locale, "action.resume_document", "Resume document");
    let retry = localization::text(locale, "action.retry_document", "Retry document");
    let export = localization::text(locale, "action.export_output", "Export translation");
    let open_output = localization::text(locale, "action.open_output", "Open exported output");
    let connect = localization::text(locale, "action.connect", "Connect");
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
    bindings.document_jobs.set_label(&document_jobs_label);
    bindings
        .document_jobs
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.document_jobs",
            "View and select persisted document jobs",
        )));
    bindings.translate.set_label(&translate_label);
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
        .fallback_enabled
        .set_label(Some(&format!("_{fallback_action}")));
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
        .provider_name
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.provider_name",
            "Session-only provider display name",
        )));
    bindings
        .provider_endpoint
        .set_tooltip_text(Some(&localization::text(
            locale,
            "tooltip.endpoint",
            "HTTPS or loopback HTTP OpenAI-compatible base endpoint",
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
    let document_state = bindings.document_job_state.get();
    let status_label = if state.worker_unavailable() {
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
    let partial_label = if let Some((completed, total)) = bindings.document_progress.get() {
        localized_template(
            state.locale(),
            "status.document_progress",
            "Document progress: {completed}/{total}",
            &[
                ("{completed}", &completed.to_string()),
                ("{total}", &total.to_string()),
            ],
        )
    } else if state.has_partial_output() {
        localization::text(state.locale(), "status.partial_output", "Partial output")
    } else {
        String::new()
    };
    bindings.partial.set_label(&partial_label);
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
    } else if bindings.export_notice.get() {
        localization::text(
            state.locale(),
            "status.exported",
            "Translation saved to the selected file.",
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
    bindings.diagnostics.set_label(&state.diagnostics_text());
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
    let blocked = state.pending_profile_deletion().is_some()
        || state.pending_model_selection().is_some()
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
        .provider_endpoint
        .set_sensitive(provider_controls_enabled);
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
    bindings.connect.set_sensitive(provider_controls_enabled);
    bindings
        .translate
        .set_sensitive(state.worker_ready() && !blocked && state.selected_model().is_some());
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
        .set_sensitive(source_import_allowed(state));
    bindings
        .document_jobs
        .set_sensitive(state.worker_ready() && !blocked && state.profile_storage_available());
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
    bindings.export_glossary.set_sensitive(
        !blocked && (!bindings.glossary.text().trim().is_empty() || state.glossary().is_some()),
    );
    bindings.stop.set_sensitive(
        state.worker_ready()
            && matches!(
                state.status(),
                AppStatus::Connecting | AppStatus::Translating
            ),
    );
    bindings
        .model
        .set_sensitive(state.worker_ready() && !blocked && !state.models().is_empty());
    bindings.source_locale.set_sensitive(!blocked);
    bindings.target_locale.set_sensitive(!blocked);
    bindings.glossary.set_sensitive(!blocked);
}

#[cfg(test)]
mod tests {
    use super::{
        AppState, AppStatus, CUSTOM_PROVIDER_PRESET_ID, CoreWorker, DEFAULT_PROVIDER_ENDPOINT,
        DEFAULT_PROVIDER_NAME, ErrorKind, OPENAI_ADAPTER_TYPE, OnboardingStage, ProviderProfileId,
        SecretRef, SecretRefNamespace, SecretValue, TranslationError, UiLocale, WorkerCommand,
        WorkerEvent, apply_worker_event, connect_action_handlers, connect_selection_handlers,
        create_window, custom_provider_profile, generate_custom_provider_id,
        localized_document_warnings, refresh_ui, start_event_pump,
    };
    use adw::prelude::*;
    use gtk::glib;
    use linguamesh_document::{DocumentWarning, DocumentWarningKind};
    use linguamesh_testkit::FakeProviderServer;
    use std::cell::RefCell;
    use std::fs;
    use std::rc::Rc;
    use std::sync::mpsc;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};
    use tokio::runtime::Builder;
    use tokio::sync::oneshot;

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

    struct ExternalFakeProvider {
        endpoint: String,
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
        assert_eq!(bindings.connect.label().as_deref(), Some("_连接"));
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
        for control in [
            bindings.saved_profile.upcast_ref::<gtk::Widget>(),
            bindings.provider_name.upcast_ref::<gtk::Widget>(),
            bindings.provider_endpoint.upcast_ref::<gtk::Widget>(),
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
            &bindings.connect,
            &bindings.open_source,
            &bindings.translate,
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
        assert!(bindings.remember_profile.is_focusable());
        assert!(bindings.fallback_enabled.is_focusable());
        assert!(bindings.fallback_profile.is_focusable());
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
        .map(|profile| profile.with_enabled(false))
        .expect("first restored profile");
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
        let _ = restored_worker.try_send(WorkerCommand::Shutdown);
        restored_window.close();

        let _ = worker.try_send(WorkerCommand::Shutdown);
        window.close();
        let _ = fs::remove_dir_all(restored_database_directory);
    }
}
