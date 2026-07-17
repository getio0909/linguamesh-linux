use adw::prelude::*;
use gtk::glib;
use linguamesh_linux::model::{AppState, AppStatus, ThemePreference, UiLocale};
use linguamesh_linux::worker::{CoreWorker, WorkerCommand, WorkerEvent};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

const SOURCE_LOCALES: [Option<&str>; 3] = [None, Some("en"), Some("zh-CN")];
const TARGET_LOCALES: [&str; 3] = ["zh-CN", "en", "ja"];
const MAX_EVENTS_PER_TICK: usize = 64;

#[derive(Clone)]
struct UiBindings {
    model: gtk::DropDown,
    source_locale: gtk::DropDown,
    target_locale: gtk::DropDown,
    source: gtk::TextBuffer,
    output: gtk::TextBuffer,
    translate: gtk::Button,
    stop: gtk::Button,
    status: gtk::Label,
    partial: gtk::Label,
    error: gtk::Label,
    locale_note: gtk::Label,
    diagnostics: gtk::Label,
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
    let worker = Rc::new(CoreWorker::spawn());
    let (window, bindings, theme, locale) = create_window(application);
    connect_selection_handlers(&bindings, &theme, &locale, &state);
    connect_action_handlers(&bindings, &state, &worker);
    start_event_pump(&bindings, &state, &worker);

    let shutdown_worker = Rc::clone(&worker);
    window.connect_destroy(move |_| {
        let _ = shutdown_worker.try_send(WorkerCommand::Shutdown);
    });
    refresh_ui(&bindings, &state.borrow());
    window.present();
}

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

    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);
    let (controls, model, source_locale, target_locale, theme, locale) = create_controls();
    root.append(&controls);

    let source = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    let source_view = gtk::TextView::builder()
        .buffer(&source)
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(8)
        .bottom_margin(8)
        .left_margin(8)
        .right_margin(8)
        .build();
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
    let editors = gtk::Paned::new(gtk::Orientation::Horizontal);
    editors.set_wide_handle(true);
    editors.set_start_child(Some(&editor_panel("Source text", &source_view)));
    editors.set_end_child(Some(&editor_panel("Streamed translation", &output_view)));
    editors.set_vexpand(true);
    root.append(&editors);

    let action_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let translate = gtk::Button::with_mnemonic("_Translate");
    translate.add_css_class("suggested-action");
    let stop = gtk::Button::with_mnemonic("_Stop");
    stop.add_css_class("destructive-action");
    action_row.append(&translate);
    action_row.append(&stop);
    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.set_hexpand(true);
    action_row.append(&status);
    let partial = gtk::Label::new(None);
    partial.add_css_class("dim-label");
    action_row.append(&partial);
    root.append(&action_row);

    let error = gtk::Label::new(None);
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
            model,
            source_locale,
            target_locale,
            source,
            output,
            translate,
            stop,
            status,
            partial,
            error,
            locale_note,
            diagnostics,
        },
        theme,
        locale,
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
    let provider = gtk::DropDown::from_strings(&["Local fake provider"]);
    provider.set_tooltip_text(Some("Provider receiving the next translation request"));
    let model = gtk::DropDown::from_strings(&["Connecting"]);
    let source_locale = gtk::DropDown::from_strings(&["Auto", "English", "Chinese"]);
    let target_locale =
        gtk::DropDown::from_strings(&["Chinese (Simplified)", "English", "Japanese"]);
    let theme = gtk::DropDown::from_strings(&["System", "Light", "Dark"]);
    let locale = gtk::DropDown::from_strings(&["English", "Simplified Chinese"]);
    for (label, control) in [
        ("Provider", provider.upcast_ref::<gtk::Widget>()),
        ("Model", model.upcast_ref::<gtk::Widget>()),
        ("Source language", source_locale.upcast_ref::<gtk::Widget>()),
        ("Target language", target_locale.upcast_ref::<gtk::Widget>()),
        ("Theme", theme.upcast_ref::<gtk::Widget>()),
        ("UI locale", locale.upcast_ref::<gtk::Widget>()),
    ] {
        controls.append(&labeled_control(label, control));
    }
    (controls, model, source_locale, target_locale, theme, locale)
}

fn labeled_control(label: &str, control: &gtk::Widget) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let label = gtk::Label::new(Some(label));
    label.set_xalign(0.0);
    label.add_css_class("caption");
    container.append(&label);
    container.append(control);
    container
}

fn editor_panel(label: &str, editor: &gtk::TextView) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let label = gtk::Label::new(Some(label));
    label.set_xalign(0.0);
    label.add_css_class("heading");
    let scroller = gtk::ScrolledWindow::builder()
        .child(editor)
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();
    scroller.set_vexpand(true);
    container.append(&label);
    container.append(&scroller);
    container
}

fn connect_selection_handlers(
    bindings: &UiBindings,
    theme: &gtk::DropDown,
    locale: &gtk::DropDown,
    state: &Rc<RefCell<AppState>>,
) {
    let model_bindings = bindings.clone();
    let model_state = Rc::clone(state);
    bindings.model.connect_selected_notify(move |drop_down| {
        let model_id = model_state
            .borrow()
            .models()
            .get(drop_down.selected() as usize)
            .map(|model| model.id.clone());
        if let Some(model_id) = model_id {
            let _ = model_state.borrow_mut().select_model(&model_id);
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

fn connect_action_handlers(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &Rc<CoreWorker>,
) {
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
        if state.request_cancellation().is_ok()
            && let Err(error) = stop_worker.try_send(WorkerCommand::Cancel)
        {
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
                    if !worker_reported_stopped {
                        event_state
                            .borrow_mut()
                            .record_client_error("The core worker disconnected.");
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

fn apply_worker_event(
    bindings: &UiBindings,
    state: &Rc<RefCell<AppState>>,
    worker: &CoreWorker,
    event: WorkerEvent,
) {
    match event {
        WorkerEvent::Connected(models) => {
            let labels = models
                .iter()
                .map(|model| model.display_name.as_str())
                .collect::<Vec<_>>();
            let model_list = gtk::StringList::new(&labels);
            state.borrow_mut().provider_connected(models);
            bindings.model.set_model(Some(&model_list));
            bindings.model.set_selected(0);
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
        WorkerEvent::Rejected(error) => {
            let should_cancel = matches!(
                state.borrow().status(),
                AppStatus::Translating | AppStatus::Cancelling
            );
            state.borrow_mut().provider_failed(error);
            if should_cancel {
                let _ = worker.try_send(WorkerCommand::Cancel);
            }
        }
        WorkerEvent::Stopped => {
            if !operation_is_terminal(state.borrow().status()) {
                state
                    .borrow_mut()
                    .record_client_error("The core worker stopped.");
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

fn refresh_ui(bindings: &UiBindings, state: &AppState) {
    bindings.output.set_text(state.output());
    bindings
        .status
        .set_label(&format!("Status: {}", state.status().label()));
    bindings.partial.set_label(if state.has_partial_output() {
        "Partial output"
    } else {
        ""
    });
    bindings
        .error
        .set_label(&state.error_text().unwrap_or_default());
    bindings
        .locale_note
        .set_label(if state.locale() == UiLocale::SimplifiedChinese {
            "Simplified Chinese resources are not wired into the runtime; English fallback remains active."
        } else {
            ""
        });
    bindings.diagnostics.set_label(&state.diagnostics_text());
    let running = matches!(
        state.status(),
        AppStatus::Translating | AppStatus::Cancelling
    );
    bindings.translate.set_sensitive(!running);
    bindings
        .stop
        .set_sensitive(state.status() == AppStatus::Translating);
    bindings.model.set_sensitive(!running);
    bindings.source_locale.set_sensitive(!running);
    bindings.target_locale.set_sensitive(!running);
}
