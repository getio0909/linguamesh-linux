use adw::prelude::*;
use gtk::glib;
use linguamesh_domain::{
    ErrorKind, ProviderProfileId, SecretRef, SecretRefNamespace, SecretValue, TranslationError,
};
use linguamesh_linux::model::{
    AppState, AppStatus, OnboardingStage, ProfileStorageStatus, ProviderProfile, StateError,
    ThemePreference, UiLocale,
};
use linguamesh_linux::worker::{CoreWorker, PersistenceIntent, WorkerCommand, WorkerEvent};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

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
    workspace: gtk::Box,
    onboarding: gtk::Box,
    onboarding_title: gtk::Label,
    onboarding_detail: gtk::Label,
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
    source: gtk::TextBuffer,
    output: gtk::TextBuffer,
    #[cfg(test)]
    source_view: gtk::TextView,
    output_view: gtk::TextView,
    #[cfg(test)]
    source_label: gtk::Label,
    #[cfg(test)]
    output_label: gtk::Label,
    translate: gtk::Button,
    stop: gtk::Button,
    status: gtk::Label,
    partial: gtk::Label,
    error: gtk::Label,
    locale_note: gtk::Label,
    diagnostics: gtk::Label,
    profile_selection_guard: Rc<Cell<bool>>,
    draft_profile_id: Rc<RefCell<Option<ProviderProfileId>>>,
}

struct EditorBindings {
    editors: gtk::Paned,
    source: gtk::TextBuffer,
    output: gtk::TextBuffer,
    #[cfg(test)]
    source_view: gtk::TextView,
    output_view: gtk::TextView,
    #[cfg(test)]
    source_label: gtk::Label,
    #[cfg(test)]
    output_label: gtk::Label,
}

fn main() -> glib::ExitCode {
    let application = adw::Application::builder()
        .application_id("dev.linguamesh.LinguaMesh")
        .build();
    application.connect_activate(build_ui);
    application.run()
}

fn build_ui(application: &adw::Application) {
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
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("LinguaMesh")
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
    ) = create_provider_session();
    root.append(&provider_session);
    let (controls, model, source_locale, target_locale, theme, locale) = create_controls();
    root.append(&controls);

    let editor_bindings = create_editors();
    root.append(&editor_bindings.editors);

    let action_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let translate = gtk::Button::with_mnemonic("_Translate");
    translate.add_css_class("suggested-action");
    let stop = gtk::Button::with_mnemonic("_Stop");
    stop.add_css_class("destructive-action");
    stop.update_property(&[gtk::accessible::Property::Label("Stop translation")]);
    action_row.append(&translate);
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
        .label("Diagnostics")
        .child(&diagnostics)
        .build();
    root.append(&diagnostics_panel);

    toolbar.set_content(Some(&root));
    window.set_content(Some(&toolbar));
    (
        window,
        UiBindings {
            workspace: root,
            onboarding,
            onboarding_title,
            onboarding_detail,
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
            source: editor_bindings.source,
            output: editor_bindings.output,
            #[cfg(test)]
            source_view: editor_bindings.source_view,
            output_view: editor_bindings.output_view,
            #[cfg(test)]
            source_label: editor_bindings.source_label,
            #[cfg(test)]
            output_label: editor_bindings.output_label,
            translate,
            stop,
            status,
            partial,
            error,
            locale_note,
            diagnostics,
            profile_selection_guard: Rc::new(Cell::new(false)),
            draft_profile_id: Rc::new(RefCell::new(None)),
        },
        theme,
        locale,
    )
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
    source_view.update_property(&[
        gtk::accessible::Property::Label("Source text"),
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
    output_view.update_property(&[
        gtk::accessible::Property::Label("Streamed translation"),
        gtk::accessible::Property::MultiLine(true),
        gtk::accessible::Property::ReadOnly(true),
    ]);
    let editors = gtk::Paned::new(gtk::Orientation::Horizontal);
    editors.set_wide_handle(true);
    let (source_panel, source_label) = editor_panel("Source te_xt", &source_view);
    let (output_panel, output_label) = editor_panel("Streamed translatio_n", &output_view);
    editors.set_start_child(Some(&source_panel));
    editors.set_end_child(Some(&output_panel));
    editors.set_vexpand(true);
    #[cfg(not(test))]
    {
        drop(source_label);
        drop(output_label);
    }
    EditorBindings {
        editors,
        source,
        output,
        #[cfg(test)]
        source_view,
        output_view,
        #[cfg(test)]
        source_label,
        #[cfg(test)]
        output_label,
    }
}

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
) {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let title = gtk::Label::new(Some("Provider profiles"));
    title.set_accessible_role(gtk::AccessibleRole::Heading);
    title.set_xalign(0.0);
    title.add_css_class("heading");
    section.append(&title);

    let note = gtk::Label::new(Some(
        "Multiple names, endpoints, and model preferences can be remembered. Credentials remain session-only, are cleared from this form immediately, and must be entered again after restart. Removing a saved profile does not disconnect its current session. Secret Service storage is not available yet.",
    ));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("dim-label");
    section.append(&note);

    let profile_actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let saved_profile = gtk::DropDown::from_strings(&["New profile..."]);
    saved_profile.set_hexpand(true);
    saved_profile.set_tooltip_text(Some(
        "Choose a saved non-secret profile or create a new profile",
    ));
    let remove_saved_profile = gtk::Button::with_label("Remove saved profile");
    remove_saved_profile.add_css_class("destructive-action");
    remove_saved_profile.set_tooltip_text(Some(
        "Remove the selected saved profile without disconnecting its current session",
    ));
    profile_actions.append(&labeled_control(
        "Sa_ved profile",
        saved_profile.upcast_ref::<gtk::Widget>(),
    ));
    profile_actions.append(&remove_saved_profile);
    section.append(&profile_actions);

    let fields = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let provider_name = gtk::Entry::builder()
        .text(DEFAULT_PROVIDER_NAME)
        .hexpand(true)
        .build();
    provider_name.set_tooltip_text(Some("Session-only provider display name"));
    let provider_endpoint = gtk::Entry::builder()
        .text(DEFAULT_PROVIDER_ENDPOINT)
        .hexpand(true)
        .build();
    provider_endpoint.set_tooltip_text(Some(
        "HTTPS or loopback HTTP OpenAI-compatible base endpoint",
    ));
    let provider_credential = gtk::PasswordEntry::builder()
        .show_peek_icon(true)
        .hexpand(true)
        .build();
    provider_credential.set_tooltip_text(Some(
        "Optional session credential; it is never written to local storage",
    ));
    let remember_profile = gtk::CheckButton::with_label("Remember non-secret profile and model");
    remember_profile.set_tooltip_text(Some(
        "Save only the provider name, endpoint, and model preference",
    ));
    let connect = gtk::Button::with_mnemonic("_Connect");
    fields.append(&labeled_control(
        "_Provider name",
        provider_name.upcast_ref::<gtk::Widget>(),
    ));
    fields.append(&labeled_control(
        "_Endpoint (loopback example)",
        provider_endpoint.upcast_ref::<gtk::Widget>(),
    ));
    fields.append(&labeled_control(
        "C_redential (optional, session only)",
        provider_credential.upcast_ref::<gtk::Widget>(),
    ));
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
        provider_name,
        provider_endpoint,
        provider_credential,
        remember_profile,
        remove_saved_profile,
        connect,
        active_provider,
    )
}

fn create_controls() -> (
    gtk::Box,
    gtk::DropDown,
    gtk::DropDown,
    gtk::DropDown,
    gtk::DropDown,
    gtk::DropDown,
) {
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let model = gtk::DropDown::from_strings(&["Select a model..."]);
    let source_locale = gtk::DropDown::from_strings(&["Auto", "English", "Chinese"]);
    let target_locale =
        gtk::DropDown::from_strings(&["Chinese (Simplified)", "English", "Japanese"]);
    let theme = gtk::DropDown::from_strings(&["System", "Light", "Dark"]);
    let locale = gtk::DropDown::from_strings(&["English", "Simplified Chinese"]);
    for (label, control) in [
        ("_Model", model.upcast_ref::<gtk::Widget>()),
        (
            "Source _language",
            source_locale.upcast_ref::<gtk::Widget>(),
        ),
        (
            "Target l_anguage",
            target_locale.upcast_ref::<gtk::Widget>(),
        ),
        ("T_heme", theme.upcast_ref::<gtk::Widget>()),
        ("_UI locale", locale.upcast_ref::<gtk::Widget>()),
    ] {
        controls.append(&labeled_control(label, control));
    }
    (controls, model, source_locale, target_locale, theme, locale)
}

fn labeled_control(label: &str, control: &gtk::Widget) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let label = gtk::Label::with_mnemonic(label);
    label.set_xalign(0.0);
    label.add_css_class("caption");
    label.set_mnemonic_widget(Some(control));
    control.update_relation(&[gtk::accessible::Relation::LabelledBy(&[label.upcast_ref()])]);
    container.append(&label);
    container.append(control);
    container
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
    let mut labels = vec!["New profile...".to_owned()];
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
                            "The selected saved profile is unavailable.",
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
                        "The active provider ID is unavailable.",
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
        let selected = if drop_down.selected() == 1 {
            UiLocale::SimplifiedChinese
        } else {
            UiLocale::English
        };
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
            state.provider_failed(TranslationError::new(
                ErrorKind::InvalidEndpoint,
                "Enter a provider name.",
            ));
            refresh_ui(&connect_bindings, &state);
            return;
        }
        if endpoint.is_empty() {
            state.provider_failed(TranslationError::new(
                ErrorKind::InvalidEndpoint,
                "Enter a provider endpoint.",
            ));
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
        let profile = match custom_provider_profile(
            profile_id,
            display_name,
            preset_id,
            adapter_type,
            endpoint,
            if has_credential {
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
                state.provider_failed(TranslationError::new(
                    ErrorKind::InvalidConfiguration,
                    "The selected provider profile is disabled.",
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
        let source = translate_bindings.source.text(
            &translate_bindings.source.start_iter(),
            &translate_bindings.source.end_iter(),
            true,
        );
        let mut state = translate_state.borrow_mut();
        state.set_source_text(source.as_str());
        state.set_source_locale(
            SOURCE_LOCALES[translate_bindings.source_locale.selected() as usize].map(str::to_owned),
        );
        state.set_target_locale(
            TARGET_LOCALES[translate_bindings.target_locale.selected() as usize],
        );
        match state.begin_translation() {
            Ok(request) => {
                if let Err(error) = translate_worker.try_send(WorkerCommand::Translate(request)) {
                    state.record_client_error(error.to_string());
                }
            }
            Err(error) => state.record_client_error(error.to_string()),
        }
        refresh_ui(&translate_bindings, &state);
    });

    let stop_bindings = bindings.clone();
    let stop_state = Rc::clone(state);
    let stop_worker = Rc::clone(worker);
    bindings.stop.connect_clicked(move |_| {
        let mut state = stop_state.borrow_mut();
        let can_cancel = if state.status() == AppStatus::Connecting {
            true
        } else {
            state.request_cancellation().is_ok()
        };
        if can_cancel && let Err(error) = stop_worker.try_send(WorkerCommand::Cancel) {
            state.record_client_error(error.to_string());
        }
        refresh_ui(&stop_bindings, &state);
    });
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
                            state.record_client_error("The core worker disconnected.");
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
            rebuild_saved_profile_dropdown(bindings, &state.borrow());
        }
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
            let result = state.borrow_mut().apply_translation_event(event);
            if let Err(error) = result {
                state.borrow_mut().record_stream_error(error.to_string());
                if let Err(error) = worker.try_send(WorkerCommand::Cancel) {
                    state.borrow_mut().record_client_error(error.to_string());
                }
            }
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
                state.record_client_error("The core worker stopped.");
            }
        }
    }
}

const fn operation_is_terminal(status: AppStatus) -> bool {
    matches!(
        status,
        AppStatus::Completed | AppStatus::Cancelled | AppStatus::Failed
    )
}

fn refresh_active_provider_label(bindings: &UiBindings, state: &AppState) {
    let active_mode = if state.active_provider_is_saved() {
        "saved"
    } else {
        "session only"
    };
    let pending_mode = if state.pending_provider_will_be_saved() {
        "will be saved without credentials"
    } else {
        "session only"
    };
    match (state.active_provider(), state.pending_provider()) {
        (Some(active), Some(pending)) => bindings.active_provider.set_label(&format!(
            "Active provider remains {} ({active_mode}); connecting {} ({pending_mode}).",
            active.display_name(),
            pending.display_name()
        )),
        (None, Some(pending)) => bindings.active_provider.set_label(&format!(
            "Connecting {} ({pending_mode}).",
            pending.display_name(),
        )),
        (Some(active), None) => bindings.active_provider.set_label(&format!(
            "Active provider: {} ({active_mode})",
            active.display_name(),
        )),
        (None, None) if !state.saved_profiles().is_empty() => {
            bindings.active_provider.set_label(
                "Saved non-secret profiles were restored. Choose one, enter its credential if required, then connect.",
            );
        }
        (None, None) => bindings.active_provider.set_label(
            "No provider connected. Credentials are always session-only and are never saved.",
        ),
    }
}

fn refresh_onboarding(bindings: &UiBindings, state: &AppState) {
    let onboarding_phase = state.onboarding_stage();
    let (title, mut detail) = match onboarding_phase {
        OnboardingStage::Starting => (
            "Provider setup · Starting",
            "Checking profile storage and starting the local validation provider. No provider connection is made automatically."
                .to_owned(),
        ),
        OnboardingStage::Unavailable => (
            "Provider setup · Unavailable",
            "The core worker is unavailable. Restart the application and review any error below; no provider request can be sent."
                .to_owned(),
        ),
        OnboardingStage::ConfigureProvider => {
            let detail = if state.profile_storage_status() == ProfileStorageStatus::Unavailable {
                "Saved profile storage is unavailable. Configure a provider below and leave Remember off; credentials stay in memory for this session only."
            } else if state.saved_profiles().is_empty() {
                "Create a provider profile below, enter a credential only if required, then choose Connect. Credentials remain in memory for this session only."
            } else {
                "Choose a saved profile below, re-enter its credential if required, then choose Connect. Restored profiles never connect automatically."
            };
            ("Provider setup · Step 1 of 2", detail.to_owned())
        }
        OnboardingStage::Connecting => {
            let detail = state.pending_provider().map_or_else(
                || {
                    "Validating the provider and discovering models. The previous active provider remains unchanged until this succeeds."
                        .to_owned()
                },
                |profile| {
                    format!(
                        "Validating {} [{}] and discovering models. The previous active provider remains unchanged until this succeeds.",
                        profile.display_name(),
                        profile.id().as_str()
                    )
                },
            );
            ("Provider setup · Connecting", detail)
        }
        OnboardingStage::SelectModel => {
            let detail = state.pending_model_selection().map_or_else(
                || {
                    "Choose a discovered model. Translation remains disabled until the selection is confirmed."
                        .to_owned()
                },
                |model| {
                    format!(
                        "Confirming model {model}. Translation remains disabled until this selection is committed."
                    )
                },
            );
            ("Provider setup · Step 2 of 2", detail)
        }
        OnboardingStage::Ready => {
            let provider = state
                .active_provider()
                .map_or_else(|| "Unavailable".to_owned(), |profile| {
                    format!("{} [{}]", profile.display_name(), profile.id().as_str())
                });
            let model = state.selected_model().unwrap_or("Unavailable");
            (
                "Provider setup · Ready",
                format!(
                    "Next request: {provider} · {model}. Use the saved-profile list and Connect to switch deliberately."
                ),
            )
        }
    };
    if state.profile_storage_status() == ProfileStorageStatus::Unavailable
        && onboarding_phase != OnboardingStage::ConfigureProvider
    {
        detail.push_str(
            " Saved profile storage is unavailable; profile persistence is disabled, Remember stays off, and credentials remain session only.",
        );
    }
    bindings.onboarding.set_visible(true);
    bindings.onboarding_title.set_label(title);
    bindings.onboarding_detail.set_label(&detail);
}

#[allow(clippy::too_many_lines)]
fn refresh_ui(bindings: &UiBindings, state: &AppState) {
    bindings.output.set_text(state.output());
    let status_label = if state.worker_unavailable() {
        "Unavailable"
    } else if !state.worker_ready() {
        "Starting"
    } else if state.pending_profile_deletion().is_some() {
        "Removing saved profile"
    } else if state.pending_model_selection().is_some() {
        "Selecting model"
    } else {
        state.status().label()
    };
    bindings
        .status
        .set_label(&format!("Status: {status_label}"));
    bindings.partial.set_label(if state.has_partial_output() {
        "Partial output"
    } else {
        ""
    });
    let error_text = state.error_text();
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
    bindings
        .locale_note
        .set_label(if state.locale() == UiLocale::SimplifiedChinese {
            "Simplified Chinese resources are not wired into the runtime; English fallback remains active."
        } else {
            ""
        });
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
}

#[cfg(test)]
mod tests {
    use super::{
        AppState, AppStatus, CUSTOM_PROVIDER_PRESET_ID, CoreWorker, DEFAULT_PROVIDER_ENDPOINT,
        DEFAULT_PROVIDER_NAME, ErrorKind, OPENAI_ADAPTER_TYPE, OnboardingStage, ProviderProfileId,
        SecretRef, SecretRefNamespace, TranslationError, WorkerCommand, WorkerEvent,
        apply_worker_event, connect_action_handlers, connect_selection_handlers, create_window,
        custom_provider_profile, generate_custom_provider_id, refresh_ui, start_event_pump,
    };
    use adw::prelude::*;
    use gtk::glib;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

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
            &bindings.translate,
            &bindings.stop,
        ] {
            assert!(button.is_focusable());
        }
        assert!(bindings.remember_profile.is_focusable());
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
        let restored_worker = Rc::new(CoreWorker::spawn());
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
        apply_worker_event(
            &restored_bindings,
            &restored_state,
            &restored_worker,
            WorkerEvent::ProfilesRestored {
                profiles: vec![restored_profile_b.clone(), restored_profile_a.clone()],
                active_profile_id: Some(restored_profile_b.id().clone()),
            },
        );
        restored_window.present();
        spin_main_context_until(&context, Duration::from_secs(5), || {
            restored_state.borrow().worker_ready()
        });
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
        assert!(
            restored_state
                .borrow()
                .error_text()
                .is_some_and(|error| error.starts_with("Secure storage unavailable:"))
        );
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
    }
}
