//! Unprivileged native frontend. All mutations go through the installed CLI.
mod appearance;
mod feedback;
use gtk::{glib, prelude::*};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    sync::mpsc,
    time::Duration,
};
use thinkpad_dashboard::firmware_update::{self, BiosPlan};

const APP_ID: &str = "io.github.thinkhub.ThinkHub";
// Native distro packages use /usr/bin; local developer installs use /usr/local/bin.
// This is fixed at build time, never selected by an untrusted runtime PATH.
const CLI: &str = match option_env!("THINKHUB_CLI_PATH") {
    Some(path) => path,
    None => "/usr/local/bin/thinkpad-dashboard",
};
const PAGES: [&str; 8] = [
    "Overview",
    "Batteries",
    "Power & thermals",
    "Display & input",
    "Firmware",
    "Preferences & diagnostics",
    "Customize",
    "Feedback",
];
type Binding = (gtk::Label, Box<dyn Fn(&Value) -> String>);

enum Event {
    Status(Result<Value, String>),
    Plan(Value, Result<Value, String>),
    Finished(String, Result<String, String>),
    BiosPlan(Result<BiosPlan, String>),
    BiosFinished(Result<String, String>),
}

struct Ui {
    css: gtk::CssProvider,
    appearance: RefCell<appearance::Appearance>,
    appearance_error: RefCell<Option<String>>,
    feedback: Rc<RefCell<feedback::Draft>>,
    window: gtk::ApplicationWindow,
    content: gtk::Box,
    status: gtk::Label,
    data: RefCell<Value>,
    bindings: RefCell<Vec<Binding>>,
    page: Cell<usize>,
    loaded: Cell<bool>,
    healthy: Cell<bool>,
    scanning: Cell<bool>,
    busy: Cell<bool>,
    bios_installing: Cell<bool>,
    inhibit_cookie: Cell<u32>,
    bios_spinner: gtk::Spinner,
    tx: mpsc::Sender<Event>,
    nav: Vec<gtk::Button>,
}

fn main() -> glib::ExitCode {
    let app = gtk::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build);
    app.run()
}

fn configure_theme(theme: appearance::Theme) {
    let settings = gtk::Settings::default().unwrap();
    if let Some(property) = settings.find_property("gtk-interface-color-scheme") {
        if let Some(value) = glib::EnumClass::with_type(property.value_type()).and_then(|c| {
            c.to_value_by_nick(if theme == appearance::Theme::Dark {
                "dark"
            } else {
                "light"
            })
        }) {
            settings.set_property_from_value("gtk-interface-color-scheme", &value);
        }
    } else {
        settings.set_gtk_application_prefer_dark_theme(theme == appearance::Theme::Dark);
    }
}

fn build(app: &gtk::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    let loaded_appearance = appearance::path().and_then(|p| appearance::load(&p));
    let appearance_error = loaded_appearance.as_ref().err().cloned();
    let appearance = loaded_appearance.unwrap_or_default();
    configure_theme(appearance.theme);
    let css = gtk::CssProvider::new();
    css.load_from_data(&appearance.css());
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("ThinkHub")
        .default_width(1120)
        .default_height(780)
        .build();
    window.set_icon_name(Some(APP_ID));
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 8);
    sidebar.add_css_class("sidebar");
    sidebar.set_width_request(220);
    let brand = label("ThinkHub", "brand");
    sidebar.append(&brand);
    sidebar.append(&label("Y O U R   T H I N K P A D", "accent"));
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_height_request(22);
    sidebar.append(&spacer);
    let mut nav = vec![];
    for name in PAGES {
        let b = gtk::Button::with_label(name);
        b.set_halign(gtk::Align::Fill);
        sidebar.append(&b);
        nav.push(b);
    }
    let spring = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spring.set_vexpand(true);
    sidebar.append(&spring);
    sidebar.append(&label(
        &format!("Linux preview · {}", env!("CARGO_PKG_VERSION")),
        "muted",
    ));
    let refresh = gtk::Button::with_label("Refresh live data");
    sidebar.append(&refresh);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.add_css_class("page");
    content.set_hexpand(true);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .child(&content)
        .build();
    body.append(&sidebar);
    body.append(&scroll);
    let status = label("Connecting to the local backend…", "status");
    status.remove_css_class("status");
    let status_bar = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    status_bar.add_css_class("status");
    let bios_spinner = gtk::Spinner::new();
    bios_spinner.set_visible(false);
    status_bar.append(&bios_spinner);
    status_bar.append(&status);
    root.append(&body);
    root.append(&status_bar);
    window.set_child(Some(&root));
    let (tx, rx) = mpsc::channel();
    let ui = Rc::new(Ui {
        css,
        appearance: RefCell::new(appearance),
        appearance_error: RefCell::new(appearance_error),
        feedback: Rc::new(RefCell::new(feedback::Draft::default())),
        window,
        content,
        status,
        data: RefCell::new(Value::Null),
        bindings: RefCell::new(vec![]),
        page: Cell::new(0),
        loaded: Cell::new(false),
        healthy: Cell::new(false),
        scanning: Cell::new(false),
        busy: Cell::new(false),
        bios_installing: Cell::new(false),
        inhibit_cookie: Cell::new(0),
        bios_spinner,
        tx,
        nav,
    });
    let weak = Rc::downgrade(&ui);
    ui.window.connect_close_request(move |_|{
        if let Some(ui)=weak.upgrade(){if ui.bios_installing.get(){ui.dialog("BIOS preparation in progress","Wait for fwupd to finish. Keep AC connected and do not power off or suspend the computer.",None);return glib::Propagation::Stop;}}
        glib::Propagation::Proceed
    });
    for (i, b) in ui.nav.iter().enumerate() {
        let weak = Rc::downgrade(&ui);
        b.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.page.set(i);
                ui.render();
            }
        });
    }
    let weak = Rc::downgrade(&ui);
    refresh.connect_clicked(move |_| {
        if let Some(ui) = weak.upgrade() {
            ui.refresh();
        }
    });
    ui.render();
    ui.window.present();
    ui.refresh();
    if let Some(directory) = std::env::var_os("THINKHUB_FEEDBACK_TEST_DIR") {
        let ui = ui.clone();
        glib::timeout_add_local_once(Duration::from_millis(600), move || {
            feedback_smoke(ui, std::path::PathBuf::from(directory));
        });
    }
    if std::env::var_os("THINKHUB_CAPTURE_BIOS_REVIEW").is_some() {
        let weak = Rc::downgrade(&ui);
        let mut attempts = 0;
        glib::timeout_add_local(Duration::from_millis(500), move || {
            attempts += 1;
            let Some(ui) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if attempts > 60 {
                return glib::ControlFlow::Break;
            }
            if !ui.loaded.get() {
                return glib::ControlFlow::Continue;
            }
            ui.page.set(4);
            ui.render();
            ui.check_bios();
            glib::ControlFlow::Break
        });
    }
    if let Some(directory) = std::env::var_os("THINKPAD_DASHBOARD_CAPTURE_DIR") {
        ui.window.maximize();
        let weak = Rc::downgrade(&ui);
        let paintable = gtk::WidgetPaintable::new(Some(&ui.window));
        let directory = std::path::PathBuf::from(directory);
        let mut step = 0usize;
        let mut attempts = 0;
        let test_appearance = std::env::var_os("THINKHUB_TEST_CUSTOMIZE").is_some();
        // Read-only visual smoke check: navigate our own pages and render our own window.
        // This never clicks a control, changes hardware or captures another application.
        glib::timeout_add_local(Duration::from_millis(900), move || {
            let Some(ui) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            attempts += 1;
            if attempts > 50 {
                eprintln!("Visual smoke check timed out");
                return glib::ControlFlow::Break;
            }
            if !ui.loaded.get() {
                return glib::ControlFlow::Continue;
            }
            if step > 0 {
                let snapshot = gtk::Snapshot::new();
                paintable.snapshot(
                    &snapshot,
                    ui.window.width() as f64,
                    ui.window.height() as f64,
                );
                if let (Some(node), Some(renderer)) = (snapshot.to_node(), ui.window.renderer()) {
                    let result = std::fs::create_dir_all(&directory)
                        .map_err(|e| e.to_string())
                        .and_then(|_| {
                            renderer
                                .render_texture(&node, None)
                                .save_to_png(directory.join(if step > PAGES.len() {
                                    "customize-light-blue.png".into()
                                } else {
                                    format!("page-{}.png", step - 1)
                                }))
                                .map_err(|e| e.to_string())
                        });
                    if let Err(e) = result {
                        eprintln!("Capture failed: {e}");
                        return glib::ControlFlow::Break;
                    }
                } else {
                    ui.window.queue_draw();
                    return glib::ControlFlow::Continue;
                }
            }
            if step == PAGES.len() && test_appearance {
                ui.nav[6].emit_clicked();
                // Opt-in appearance integration check: use an isolated XDG_CONFIG_HOME.
                // Exercise the same GTK signals users trigger, then capture the live update.
                for widget in descendants(ui.content.upcast_ref()) {
                    if let Some(button) = widget.downcast_ref::<gtk::ToggleButton>() {
                        if button.label().as_deref() == Some("Light") {
                            button.set_active(true);
                        }
                    }
                    if let Some(dropdown) = widget.downcast_ref::<gtk::DropDown>() {
                        dropdown.set_selected(1);
                    }
                }
                step += 1;
                return glib::ControlFlow::Continue;
            }
            if step >= PAGES.len() {
                ui.page.set(0);
                ui.render();
                if test_appearance {
                    eprintln!(
                        "Appearance signal check completed: live light theme and blue accent saved"
                    );
                } else {
                    eprintln!(
                        "Read-only visual smoke check completed: {} pages",
                        PAGES.len()
                    );
                }
                return glib::ControlFlow::Break;
            }
            ui.nav[step].emit_clicked();
            step += 1;
            glib::ControlFlow::Continue
        });
    }
    let weak = Rc::downgrade(&ui);
    glib::timeout_add_local(Duration::from_secs(5), move || {
        if let Some(ui) = weak.upgrade() {
            ui.refresh();
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
    // The receiver owns Ui only while its window is open. No GTK objects cross threads.
    glib::timeout_add_local(Duration::from_millis(100), move || {
        if !ui.window.is_visible() {
            return glib::ControlFlow::Break;
        }
        while let Ok(event) = rx.try_recv() {
            ui.handle(event);
        }
        glib::ControlFlow::Continue
    });
}

fn label(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    l.set_wrap(true);
    if !class.is_empty() {
        l.add_css_class(class);
    }
    l
}
fn descendants(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut result = vec![];
    let mut child = widget.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        result.extend(descendants(&widget));
        result.push(widget);
    }
    result
}

// Explicit developer-only check of the real form/review signals. Never presses Send
// in another application. Opening an email draft requires a separate opt-in flag.
fn feedback_smoke(ui: Rc<Ui>, directory: std::path::PathBuf) {
    ui.window.maximize();
    std::fs::create_dir_all(&directory).expect("Create feedback test directory");
    let id = format!(
        "THINKHUB-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let subject = format!("Feedback delivery test {id}");
    let message = format!("This is a test of ThinkHub's new Feedback option.\nTest ID: {id}\nPlease confirm this message arrived. No hardware details or logs are attached.");
    ui.nav[7].emit_clicked();
    for widget in descendants(ui.content.upcast_ref()) {
        if let Some(entry) = widget.downcast_ref::<gtk::Entry>() {
            entry.set_text(&subject);
        }
        if let Some(view) = widget.downcast_ref::<gtk::TextView>() {
            view.buffer().set_text(&message);
        }
    }
    assert_eq!(ui.feedback.borrow().subject, subject);
    assert_eq!(ui.feedback.borrow().message, message);
    ui.nav[6].emit_clicked();
    ui.nav[7].emit_clicked();
    assert!(descendants(ui.content.upcast_ref()).iter().any(|w| w
        .downcast_ref::<gtk::Entry>()
        .is_some_and(|e| e.text() == subject)));
    let prepared = feedback::prepare(&ui.feedback.borrow(), &ui.data.borrow()).unwrap();
    std::fs::write(directory.join("test-email.txt"), prepared.copy_text()).unwrap();
    std::fs::write(directory.join("test-mailto.txt"), &prepared.uri).unwrap();
    for widget in descendants(ui.content.upcast_ref()) {
        if let Some(button) = widget.downcast_ref::<gtk::Button>() {
            if button.label().as_deref() == Some("Review feedback") {
                button.emit_clicked();
            }
        }
    }
    glib::timeout_add_local_once(Duration::from_millis(600), move || {
        let review = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Window>().ok())
            .find(|w| w.title().as_deref() == Some("Review feedback"))
            .expect("Review window opened");
        for (name, window) in [
            ("feedback-form.png", ui.window.upcast_ref::<gtk::Window>()),
            ("feedback-review.png", &review),
        ] {
            let paintable = gtk::WidgetPaintable::new(Some(window));
            let snapshot = gtk::Snapshot::new();
            paintable.snapshot(&snapshot, window.width() as f64, window.height() as f64);
            let node = snapshot.to_node().expect("Feedback window painted");
            window
                .renderer()
                .expect("Feedback renderer")
                .render_texture(&node, None)
                .save_to_png(directory.join(name))
                .expect("Save feedback capture");
        }
        eprintln!("Feedback form/review check passed: draft survives navigation; recipient and message preserved; no email sent. Test ID: {id}");
        if std::env::var_os("THINKHUB_FEEDBACK_TEST_OPEN_MAIL").is_some() {
            for widget in descendants(review.upcast_ref()) {
                if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                    if button.label().as_deref() == Some("Open email app") {
                        button.emit_clicked();
                    }
                }
            }
            eprintln!("Requested email draft handoff only; delivery still requires Send in the email app.");
        }
    });
}
fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "Unavailable".into(),
        _ => v.to_string(),
    }
}
fn field(v: &Value, key: &str) -> String {
    text(&v[key])
}
fn list<'a>(v: &'a Value, path: &str) -> &'a [Value] {
    v.pointer(path)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
fn number(v: &Value, suffix: &str) -> String {
    v.as_f64()
        .map(|n| format!("{n:.1}{suffix}"))
        .unwrap_or_else(|| "Unavailable".into())
}
fn bytes(v: &Value) -> String {
    v.as_f64()
        .map(|n| format!("{:.1} GiB", n / 1_073_741_824.0))
        .unwrap_or_else(|| "Unavailable".into())
}

fn command(args: &[String], authorize: bool) -> Result<String, String> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    // Authentication has no arbitrary timeout: the user may need time for polkit.
    let output = if authorize {
        std::process::Command::new(CLI)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| e.to_string())?
    } else {
        thinkpad_dashboard::process::run(Path::new(CLI), &refs, Duration::from_secs(45), 2_000_000)
            .map_err(|e| format!("{e:#}"))?
    };
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if error.is_empty() {
            format!("Backend exited with {}", output.status)
        } else {
            error
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn controllers(v: &Value) -> Vec<thinkpad_dashboard::controllers::Controller> {
    list(v, "/controllers")
        .iter()
        .filter_map(|c| {
            Some(thinkpad_dashboard::controllers::Controller {
                unit: c["unit"].as_str()?.into(),
                state: c["state"].as_str().unwrap_or("unknown").into(),
            })
        })
        .collect()
}
fn blockers(v: &Value, request: &Value) -> Vec<String> {
    match serde_json::from_value(request.clone()) {
        Ok(request) => thinkpad_dashboard::controllers::blockers(&request, &controllers(v)),
        Err(e) => vec![format!("Invalid request: {e}")],
    }
}
fn plan_allowed(v: &Value) -> bool {
    v.pointer("/plan/changes")
        .and_then(Value::as_array)
        .is_some_and(|changes| {
            changes.iter().all(|c| {
                ["attribute", "before", "after"]
                    .iter()
                    .all(|k| c[*k].is_string())
            })
        })
        && v.get("blockers")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
}

fn plan_summary(v: &Value) -> String {
    let mut lines = vec!["Review the proposed hardware change. Administrator authorization is requested only after you press Apply.".to_owned()];
    let changes = list(v, "/plan/changes");
    if changes.is_empty() {
        lines.push("\nThe requested setting already matches the current value.".into());
    }
    for c in changes {
        lines.push(format!(
            "\n{}\n    {} → {}",
            field(c, "attribute"),
            field(c, "before"),
            field(c, "after")
        ));
    }
    for note in list(v, "/plan/notes") {
        lines.push(format!("\n{}", text(note)));
    }
    for reason in list(v, "/blockers") {
        lines.push(format!("\nBlocked: {}", text(reason)));
    }
    lines.join("\n")
}

fn result_summary(title: &str, output: &str) -> String {
    let Ok(v) = serde_json::from_str::<Value>(output) else {
        return output.to_owned();
    };
    if title == "Firmware information" {
        let mut lines = vec!["Cached firmware information · metadata freshness unknown".to_owned()];
        for d in list(&v, "/devices") {
            lines.push(format!(
                "\n{} · installed {}",
                field(d, "name"),
                field(d, "version")
            ));
        }
        lines.push(format!(
            "\nComponents with cached updates: {}",
            v["cached_updates"]
                .as_array()
                .map(|a| a.len().to_string())
                .unwrap_or_else(|| "Unavailable".into())
        ));
        for u in list(&v, "/cached_updates") {
            lines.push(format!(
                "{}: {} → {}",
                field(u, "component"),
                field(u, "installed_version"),
                field(u, "available_versions")
            ));
        }
        for e in list(&v, "/errors") {
            lines.push(format!("\n{}", text(e)));
        }
        lines.push("\nThis check does not confirm that firmware is up to date.".into());
        return lines.join("\n");
    }
    if title == "BIOS information" {
        return list(&v, "/settings")
            .iter()
            .map(|s| format!("{}: {}", field(s, "name"), field(s, "value")))
            .collect::<Vec<_>>()
            .join("\n");
    }
    if let Some(active) = v["active"].as_str() {
        return format!("Active power profile: {active}");
    }
    if v.get("verified") == Some(&Value::Bool(true)) {
        return "The change was applied and verified by the hardware backend.".into();
    }
    if title == "Preferences saved" {
        return "Preferences were saved. Hardware settings were not changed.".into();
    }
    output.to_owned()
}

impl Ui {
    fn refresh(&self) {
        if self.scanning.replace(true) {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = command(&["status".into()], false)
                .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));
            let _ = tx.send(Event::Status(result));
        });
    }
    fn handle(self: &Rc<Self>, event: Event) {
        match event {
            Event::Status(Ok(v)) => {
                if v["schema_version"] != 1 || v["app_version"] != env!("CARGO_PKG_VERSION") {
                    self.healthy.set(false);
                    self.scanning.set(false);
                    self.status.set_text("Backend version is incompatible. Reinstall the matching backend before using controls.");
                    return;
                }
                let topology_changed = topology(&self.data.borrow()) != topology(&v);
                *self.data.borrow_mut() = v;
                self.healthy.set(true);
                self.scanning.set(false);
                if !self.loaded.replace(true) || topology_changed {
                    self.render();
                }
                let data = self.data.borrow();
                for (label, format) in self.bindings.borrow().iter() {
                    label.set_text(&format(&data));
                }
                if !self.busy.get() {
                    self.status.set_text("Live local data · refreshes every 5 seconds · changes require an explicit action");
                }
            }
            Event::Status(Err(e)) => {
                self.healthy.set(false);
                self.scanning.set(false);
                self.status.set_text(&format!(
                    "Live data unavailable; displayed values may be stale. {e}"
                ));
            }
            Event::Plan(request, result) => {
                self.busy.set(false);
                match result {
                    Ok(plan) => {
                        let allowed = plan_allowed(&plan);
                        let description = plan_summary(&plan);
                        self.dialog(
                            if allowed {
                                "Review hardware change"
                            } else {
                                "Change blocked"
                            },
                            &description,
                            allowed.then_some(vec!["apply".into(), request.to_string()]),
                        );
                    }
                    Err(e) => self.dialog("Cannot prepare this change", &e, None),
                }
            }
            Event::Finished(title, result) => {
                self.busy.set(false);
                match result {
                    Ok(s) => {
                        let message = if s.is_empty() {
                            "Completed successfully.".to_owned()
                        } else {
                            result_summary(&title, &s)
                        };
                        self.dialog(&title, &message, None);
                    }
                    Err(e) => self.dialog("Action did not complete", &e, None),
                }
                self.refresh();
            }
            Event::BiosPlan(result) => {
                self.busy.set(false);
                match result {
                    Ok(plan) => self.review_bios(plan),
                    Err(e) => self.dialog("BIOS update check unavailable", &e, None),
                }
            }
            Event::BiosFinished(result) => {
                self.bios_spinner.stop();
                self.bios_spinner.set_visible(false);
                self.bios_installing.set(false);
                self.busy.set(false);
                if let Some(app) = self.window.application() {
                    let cookie = self.inhibit_cookie.replace(0);
                    if cookie != 0 {
                        app.uninhibit(cookie);
                    }
                }
                match result {
                    Ok(message) => self.dialog("BIOS preparation completed", &message, None),
                    Err(e) => self.dialog("BIOS preparation did not complete", &e, None),
                }
                self.refresh();
            }
        }
    }
    fn render(self: &Rc<Self>) {
        // Move focus out of the page before removing editors or selectable labels.
        self.nav[self.page.get()].grab_focus();
        while let Some(w) = self.content.first_child() {
            self.content.remove(&w);
        }
        self.bindings.borrow_mut().clear();
        for (i, b) in self.nav.iter().enumerate() {
            if i == self.page.get() {
                b.add_css_class("selected");
            } else {
                b.remove_css_class("selected");
            }
        }
        self.content.append(&label(PAGES[self.page.get()], "title"));
        self.content.append(&label(
            match self.page.get() {
                0 => "Your ThinkPad, at a glance.",
                1 => "Charge, health and the controls your hardware exposes.",
                2 => "Power profiles and live cooling information.",
                3 => "Display brightness, keyboard lighting and TrackPoint controls.",
                4 => "Check and prepare supported BIOS updates through fwupd.",
                6 => "Make ThinkHub feel like yours.",
                7 => "Share a bug report, feature idea or feedback by email.",
                _ => "Save preferences and inspect detected capabilities.",
            },
            "subtitle",
        ));
        if self.page.get() == 6 {
            self.customize();
            return;
        }
        if self.page.get() == 7 {
            self.content.append(&feedback::page(
                &self.window,
                self.feedback.clone(),
                self.data.borrow().clone(),
            ));
            return;
        }
        if !self.loaded.get() {
            self.content
                .append(&label("Waiting for the installed backend…", "muted"));
            return;
        }
        let v = self.data.borrow().clone();
        match self.page.get() {
            0 => self.overview(&v),
            1 => self.batteries(&v),
            2 => self.power(&v),
            3 => self.devices(&v),
            4 => self.firmware(&v),
            _ => self.preferences(&v),
        }
    }
    fn card(&self, title: &str) -> gtk::Box {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 8);
        b.add_css_class("card");
        b.append(&label(title, "card-title"));
        self.content.append(&b);
        b
    }
    fn note(&self, parent: &gtk::Box, text: &str) {
        parent.append(&label(text, "muted"));
    }
    fn row(&self, parent: &gtk::Box, name: &str, value: &str) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        row.add_css_class("row");
        let key = label(name, "muted");
        key.set_width_request(155);
        let val = label(value, "");
        val.set_selectable(true);
        val.set_hexpand(true);
        row.append(&key);
        row.append(&val);
        parent.append(&row);
    }
    fn bound(&self, parent: &gtk::Box, name: &str, format: impl Fn(&Value) -> String + 'static) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        row.add_css_class("row");
        let key = label(name, "muted");
        key.set_width_request(155);
        let val = label(&format(&self.data.borrow()), "");
        val.set_selectable(true);
        val.set_hexpand(true);
        row.append(&key);
        row.append(&val);
        parent.append(&row);
        self.bindings.borrow_mut().push((val, Box::new(format)));
    }
    fn path(&self, parent: &gtk::Box, name: &str, pointer: &str) {
        let p = pointer.to_owned();
        self.bound(parent, name, move |v| {
            text(v.pointer(&p).unwrap_or(&Value::Null))
        });
    }
    fn request(self: &Rc<Self>, request: Value) {
        if !self.healthy.get() {
            self.dialog(
                "Live backend unavailable",
                "Refresh live data successfully before preparing a change.",
                None,
            );
            return;
        }
        if self.busy.replace(true) {
            return;
        }
        self.status.set_text("Preparing a fresh change preview…");
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = command(&["plan".into(), request.to_string()], false)
                .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));
            let _ = tx.send(Event::Plan(request, result));
        });
    }
    fn run(self: &Rc<Self>, title: &str, args: Vec<String>) {
        if !self.healthy.get() {
            self.dialog(
                "Live backend unavailable",
                "Refresh live data successfully before running an action.",
                None,
            );
            return;
        }
        if self.busy.replace(true) {
            return;
        }
        self.status.set_text(
            "Action in progress · respond to the system authorization dialog if one appears",
        );
        let tx = self.tx.clone();
        let title = title.to_owned();
        std::thread::spawn(move || {
            let result = command(&args, true);
            let _ = tx.send(Event::Finished(title, result));
        });
    }
    fn dialog(self: &Rc<Self>, title: &str, details: &str, action: Option<Vec<String>>) {
        let window = gtk::Window::builder()
            .title(title)
            .transient_for(&self.window)
            .modal(true)
            .default_width(680)
            .default_height(420)
            .build();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
        body.add_css_class("page");
        body.append(&label(title, "card-title"));
        let view = gtk::TextView::builder()
            .editable(false)
            .cursor_visible(false)
            .monospace(true)
            .wrap_mode(gtk::WrapMode::WordChar)
            .build();
        // Plain text only: system values never become markup or shell code.
        view.buffer().set_text(details);
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&view)
            .build();
        body.append(&scroll);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        buttons.set_halign(gtk::Align::End);
        let close = gtk::Button::with_label(if action.is_some() { "Cancel" } else { "Close" });
        let w = window.downgrade();
        close.connect_clicked(move |_| {
            if let Some(w) = w.upgrade() {
                w.close();
            }
        });
        buttons.append(&close);
        if let Some(args) = action {
            let apply = gtk::Button::with_label("Apply change");
            apply.add_css_class("suggested-action");
            let weak = Rc::downgrade(self);
            let w = window.downgrade();
            let once = Cell::new(false);
            apply.connect_clicked(move |_| {
                if once.replace(true) {
                    return;
                }
                if let Some(ui) = weak.upgrade() {
                    ui.run("Action completed", args.clone());
                }
                if let Some(w) = w.upgrade() {
                    w.close();
                }
            });
            buttons.append(&apply);
        }
        body.append(&buttons);
        window.set_child(Some(&body));
        window.present();
    }
    fn preview_button(
        self: &Rc<Self>,
        parent: &gtk::Box,
        v: &Value,
        sample: &Value,
        make: impl Fn() -> Value + 'static,
    ) {
        let reasons = blockers(v, sample);
        let button = gtk::Button::with_label("Review change…");
        button.set_halign(gtk::Align::Start);
        button.add_css_class("suggested-action");
        if !reasons.is_empty() {
            button.set_sensitive(false);
            self.note(parent, &reasons.join("\n"));
        }
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.request(make());
            }
        });
        parent.append(&button);
    }
    fn overview(&self, v: &Value) {
        let card = self.card(&field(&v["hardware"]["system"], "model"));
        self.path(&card, "System", "/overview/os_name");
        self.path(&card, "Processor", "/overview/cpu_model");
        self.bound(&card, "Memory available", |v| {
            format!(
                "{} of {}",
                bytes(&v["overview"]["memory_available_bytes"]),
                bytes(&v["overview"]["memory_total_bytes"])
            )
        });
        self.path(&card, "Kernel", "/overview/kernel");
        let card = self.card("Live status");
        self.bound(&card, "Battery charge", |v| {
            number(&v["hardware"]["battery_overview"]["percent"], "%")
        });
        self.bound(&card, "Power source", |v| {
            if list(v, "/hardware/power_sources")
                .iter()
                .any(|p| p["online"] == true)
            {
                "External power".into()
            } else {
                "No external power reported".into()
            }
        });
        self.path(&card, "Power profile", "/power/state/active");
        self.bound(&card, "CPU temperature", |v| {
            list(v, "/hardware/sensors")
                .iter()
                .find(|s| {
                    s["driver"] == "coretemp"
                        && s["label"]
                            .as_str()
                            .is_some_and(|l| l.starts_with("Package"))
                })
                .or_else(|| {
                    list(v, "/hardware/sensors")
                        .iter()
                        .find(|s| s["label"] == "CPU")
                })
                .map(|s| number(&s["value"], " °C"))
                .unwrap_or_else(|| "Unavailable".into())
        });
        self.bound(&card, "Fan speed", |v| {
            list(v, "/hardware/sensors")
                .iter()
                .filter(|s| s["unit"] == "RPM")
                .map(|s| number(&s["value"], " RPM"))
                .collect::<Vec<_>>()
                .join(" · ")
        });
        let card = self.card("Hardware");
        for gpu in list(v, "/overview/graphics") {
            self.row(
                &card,
                "Graphics",
                &format!("{} · {}", field(gpu, "vendor"), field(gpu, "driver")),
            );
        }
        for drive in list(v, "/overview/drives")
            .iter()
            .filter(|d| d["size_bytes"].as_u64().unwrap_or(0) > 0)
        {
            self.row(
                &card,
                &field(drive, "id"),
                &format!(
                    "{} · {}",
                    field(drive, "model"),
                    bytes(&drive["size_bytes"])
                ),
            );
        }
        self.note(&card,"Only capabilities detected on this machine are exposed. Support varies with model, firmware and kernel.");
    }
    fn batteries(self: &Rc<Self>, v: &Value) {
        if list(v, "/hardware/batteries").is_empty() {
            self.note(&self.card("Batteries"), "No battery was detected.");
        }
        for (i, b) in list(v, "/hardware/batteries").iter().enumerate() {
            let id = field(b, "id");
            let card = self.card(&format!("{id} · {}", field(b, "model")));
            if b["present"] != true {
                self.note(&card, "Battery is not present.");
                continue;
            }
            self.bound(&card, "Charge", move |v| {
                format!(
                    "{} · {}",
                    number(&v["hardware"]["batteries"][i]["percent"], "%"),
                    field(&v["hardware"]["batteries"][i], "status")
                )
            });
            self.bound(&card, "Battery health", move |v| {
                number(
                    &v["hardware"]["batteries"][i]["health_percent"],
                    "% of design capacity",
                )
            });
            self.bound(&card, "Capacity", move |v| {
                format!(
                    "{} full / {} design",
                    number(&v["hardware"]["batteries"][i]["energy_full_wh"], " Wh"),
                    number(&v["hardware"]["batteries"][i]["energy_design_wh"], " Wh")
                )
            });
            self.path(&card, "Cycles", &format!("/hardware/batteries/{i}/cycles"));
            self.bound(&card, "Current thresholds", move |v| {
                format!(
                    "{}–{}%",
                    field(&v["hardware"]["batteries"][i], "charge_start_percent"),
                    field(&v["hardware"]["batteries"][i], "charge_end_percent")
                )
            });
            if let (Some(start), Some(end)) = (
                b["charge_start_percent"].as_u64(),
                b["charge_end_percent"].as_u64(),
            ) {
                let controls = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                let a = gtk::SpinButton::with_range(0., 99., 1.);
                a.set_value(start as f64);
                let z = gtk::SpinButton::with_range(1., 100., 1.);
                z.set_value(end as f64);
                controls.append(&label("Start %", "muted"));
                controls.append(&a);
                controls.append(&label("Stop %", "muted"));
                controls.append(&z);
                card.append(&controls);
                let sample =
                    json!({"kind":"battery_thresholds","battery":id,"start":start,"end":end});
                let battery = id.clone();
                self.preview_button(&card,v,&sample,move ||json!({"kind":"battery_thresholds","battery":battery,"start":a.value_as_int(),"end":z.value_as_int()}));
            }
            self.path(
                &card,
                "Charge mode",
                &format!("/hardware/batteries/{i}/charge_modes/active"),
            );
            let modes: Vec<&str> = b["charge_modes"]["available"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|s| matches!(*s, "auto" | "inhibit-charge"))
                .collect();
            if !modes.is_empty() {
                let dropdown = gtk::DropDown::from_strings(&modes);
                dropdown.set_selected(
                    modes
                        .iter()
                        .position(|m| Some(*m) == b["charge_modes"]["active"].as_str())
                        .unwrap_or(0) as u32,
                );
                card.append(&dropdown);
                let owned: Vec<String> = modes.iter().map(|s| s.to_string()).collect();
                let sample = json!({"kind":"battery_charge_mode","battery":id,"mode":owned[0]});
                self.preview_button(&card,v,&sample,move ||json!({"kind":"battery_charge_mode","battery":id,"mode":owned[dropdown.selected() as usize]}));
            }
        }
    }
    fn power(self: &Rc<Self>, v: &Value) {
        let card = self.card("Power profile");
        self.path(&card, "Active profile", "/power/state/active");
        let profiles: Vec<String> = list(v, "/power/state/profiles")
            .iter()
            .filter_map(|p| p["id"].as_str().map(str::to_owned))
            .collect();
        if profiles.is_empty() {
            self.note(
                &card,
                &format!(
                    "Power profile service unavailable. {}",
                    field(&v["power"], "reason")
                ),
            );
        }
        for profile in profiles {
            let b = gtk::Button::with_label(&profile);
            b.set_halign(gtk::Align::Start);
            let weak = Rc::downgrade(self);
            b.connect_clicked(move |_|if let Some(ui)=weak.upgrade(){ui.dialog("Change power profile",&format!("Switch to {profile}? The power service will check for active holds and policy restrictions."),Some(vec!["power".into(),"set".into(),profile.clone()]));});
            card.append(&b);
        }
        self.bound(&card, "Active holds", |v| {
            serde_json::to_string(&v["power"]["state"]["holds"]).unwrap_or_default()
        });
        let card = self.card("Thermals");
        self.note(
            &card,
            "Cooling is monitored here. Manual fan curves are not available in this prototype.",
        );
        for (i, s) in list(v, "/hardware/sensors").iter().enumerate() {
            if s["driver"] == "thinkpad"
                && s["label"].as_str().is_some_and(|n| n.starts_with("temp"))
                && s["value"].as_f64() == Some(0.)
            {
                continue;
            }
            let name = format!("{} · {}", field(s, "driver"), field(s, "label"));
            let unit = field(s, "unit");
            self.bound(&card, &name, move |v| {
                number(&v["hardware"]["sensors"][i]["value"], &format!(" {unit}"))
            });
        }
        let card = self.card("Other power controllers");
        for c in list(v, "/controllers") {
            self.row(&card, &field(c, "unit"), &field(c, "state"));
        }
        self.note(
            &card,
            "Settings owned by another controller are blocked. This app does not stop services.",
        );
    }
    fn devices(self: &Rc<Self>, v: &Value) {
        let card = self.card("Connected displays");
        for d in list(v, "/devices/displays")
            .iter()
            .filter(|d| d["connected"] == true)
        {
            self.row(
                &card,
                &field(d, "connector"),
                &format!(
                    "Advertised modes: {}",
                    d["supported_resolutions"]
                        .as_array()
                        .map(|a| a.iter().map(text).collect::<Vec<_>>().join(", "))
                        .unwrap_or_else(|| "Unavailable".into())
                ),
            );
        }
        for d in list(v, "/desktop/outputs") {
            self.row(
                &card,
                &field(d, "name"),
                &format!(
                    "{} × {} · {} Hz · scale {}",
                    field(d, "width"),
                    field(d, "height"),
                    field(d, "refresh_hz"),
                    field(d, "scale")
                ),
            );
        }
        for (i, b) in list(v, "/hardware/brightness").iter().enumerate() {
            let Some(max) = b["maximum"].as_u64().filter(|m| *m > 0) else {
                continue;
            };
            let id = field(b, "id");
            let display = b["kind"] == "display";
            let card = self.card(if display {
                "Display brightness"
            } else if id.contains("thinklight") {
                "ThinkLight"
            } else {
                "Keyboard backlight"
            });
            self.note(&card, &id);
            self.bound(&card, "Current level", move |v| {
                if display {
                    number(
                        &json!(
                            v["hardware"]["brightness"][i]["current"]
                                .as_f64()
                                .unwrap_or(0.)
                                / max as f64
                                * 100.
                        ),
                        "%",
                    )
                } else {
                    format!(
                        "{} / {max}",
                        field(&v["hardware"]["brightness"][i], "current")
                    )
                }
            });
            let spin = gtk::SpinButton::with_range(0., if display { 100. } else { max as f64 }, 1.);
            let current = b["current"].as_f64().unwrap_or(0.);
            spin.set_value(if display {
                current / max as f64 * 100.
            } else {
                current
            });
            card.append(&spin);
            let sample = json!({"kind":"brightness","device":id,"value":0});
            self.preview_button(
                &card,
                v,
                &sample,
                move || json!({"kind":"brightness","device":id,"value":if display { (spin.value() / 100. * max as f64).round() as u64 } else {spin.value() as u64}}),
            );
        }
        for (i, t) in list(v, "/devices/trackpoints").iter().enumerate() {
            let card = self.card(&field(t, "name"));
            let controller = field(t, "controller");
            for setting in ["sensitivity", "speed"] {
                if let Some(current) = t[setting].as_u64() {
                    self.path(
                        &card,
                        &format!("Current {setting}"),
                        &format!("/devices/trackpoints/{i}/{setting}"),
                    );
                    let spin = gtk::SpinButton::with_range(1., 255., 1.);
                    spin.set_value(current as f64);
                    card.append(&spin);
                    let id = controller.clone();
                    let sample = json!({"kind":"track_point","controller":id,"setting":setting,"value":current});
                    self.preview_button(&card,v,&sample,move||json!({"kind":"track_point","controller":id,"setting":setting,"value":spin.value_as_int()}));
                }
            }
        }
        let card = self.card("Input devices");
        for input in list(v, "/devices/inputs")
            .iter()
            .filter(|i| i["kind"] != "other")
        {
            self.row(&card, &field(input, "kind"), &field(input, "name"));
        }
        self.note(&card,"Desktop acceleration, tap-to-click and display-mode changes are not implemented. No keyboard or pointer events are recorded.");
    }
    fn check_bios(&self) {
        if self.busy.replace(true) {
            return;
        }
        self.status
            .set_text("Checking BIOS releases and update readiness…");
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = firmware_update::inspect().map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::BiosPlan(result));
        });
    }
    fn review_bios(self: &Rc<Self>, plan: BiosPlan) {
        let window = gtk::Window::builder()
            .title("BIOS update warning")
            .transient_for(&self.window)
            .modal(true)
            .default_width(640)
            .default_height(680)
            .build();
        let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
        body.add_css_class("page");
        body.append(&label("BIOS update warning", "title"));
        body.append(&label(
            &format!("{} · installed {}", plan.component, plan.installed_version),
            "",
        ));
        if let Some(release) = &plan.release {
            body.append(&label(
                &format!("Available BIOS: {} · stable LVFS catalog", release.version),
                "accent",
            ));
        }
        let warning = gtk::Box::new(gtk::Orientation::Vertical, 10);
        warning.add_css_class("bios-warning");
        warning.append(&label(
            "An interrupted BIOS update can prevent this PC from starting.",
            "bios-warning-title",
        ));
        warning.append(&label("Save your work and connect AC power. Do not power off, disconnect power, suspend, or close the lid while the firmware updater is running. A restart is normally required to apply the BIOS update. If disk encryption uses the TPM, have your recovery key available.",""));
        body.append(&warning);
        body.append(&label("After you confirm, the system will request your administrator password. ThinkHub never sees or stores that password. Cancelling the warning or the authorization dialog stops the update before staging. ThinkHub will not restart your PC automatically.","muted"));
        for reason in &plan.blockers {
            body.append(&label(reason, "notice"));
        }
        let acknowledge = gtk::CheckButton::with_label(
            "I understand the risks and want to prepare this BIOS update.",
        );
        acknowledge.set_tooltip_text(Some(
            "The next step requires system administrator authorization.",
        ));
        acknowledge.set_sensitive(plan.ready());
        body.append(&acknowledge);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        buttons.set_halign(gtk::Align::End);
        let cancel = gtk::Button::with_label("Close");
        let weak = window.downgrade();
        cancel.connect_clicked(move |_| {
            if let Some(w) = weak.upgrade() {
                w.close();
            }
        });
        buttons.append(&cancel);
        let prepare = gtk::Button::with_label("Authenticate & prepare BIOS update…");
        prepare.add_css_class("suggested-action");
        prepare.set_sensitive(false);
        let weak = prepare.downgrade();
        acknowledge.connect_toggled(move |b| {
            if let Some(prepare) = weak.upgrade() {
                prepare.set_sensitive(b.is_active());
            }
        });
        let weak = Rc::downgrade(self);
        let dialog = window.downgrade();
        let once = Cell::new(false);
        prepare.connect_clicked(move |_| {
            if once.replace(true) {
                return;
            }
            if let Some(ui) = weak.upgrade() {
                ui.prepare_bios(plan.clone());
            }
            if let Some(w) = dialog.upgrade() {
                w.close();
            }
        });
        buttons.append(&prepare);
        body.append(&buttons);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&body)
            .build();
        window.set_child(Some(&scroll));
        window.present();
        // Explicit development capture opens only the read-only review; never acknowledges it.
        if let Some(path) = std::env::var_os("THINKHUB_CAPTURE_BIOS_REVIEW") {
            let paintable = gtk::WidgetPaintable::new(Some(&window));
            let weak = window.downgrade();
            let mut attempts = 0;
            glib::timeout_add_local(Duration::from_millis(700), move || {
                attempts += 1;
                let Some(window) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if attempts > 10 {
                    return glib::ControlFlow::Break;
                }
                let snapshot = gtk::Snapshot::new();
                paintable.snapshot(&snapshot, window.width() as f64, window.height() as f64);
                if let (Some(node), Some(renderer)) = (snapshot.to_node(), window.renderer()) {
                    match renderer.render_texture(&node, None).save_to_png(&path) {
                        Ok(()) => eprintln!("Read-only BIOS warning capture completed"),
                        Err(e) => eprintln!("BIOS warning capture failed: {e}"),
                    }
                    return glib::ControlFlow::Break;
                }
                window.queue_draw();
                glib::ControlFlow::Continue
            });
        }
    }
    fn prepare_bios(&self, plan: BiosPlan) {
        if !plan.ready() || self.busy.replace(true) {
            return;
        }
        self.bios_installing.set(true);
        self.bios_spinner.set_visible(true);
        self.bios_spinner.start();
        if let Some(app) = self.window.application() {
            self.inhibit_cookie.set(app.inhibit(
                Some(&self.window),
                gtk::ApplicationInhibitFlags::LOGOUT | gtk::ApplicationInhibitFlags::SUSPEND,
                Some("Preparing BIOS update with fwupd"),
            ));
        }
        self.status.set_text("Authorizing / preparing BIOS with fwupd · keep AC connected · do not power off or suspend");
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = firmware_update::install_reviewed(&mut firmware_update::System, &plan)
                .map_err(|e| format!("{e:#}"));
            let _ = tx.send(Event::BiosFinished(result));
        });
    }
    fn firmware(self: &Rc<Self>, v: &Value) {
        let card = self.card("BIOS updates");
        self.note(&card,"Supported UEFI BIOS updates are provided by fwupd and the stable LVFS catalog. Checking uses the local catalog; refresh it to look for newly published releases. Only an explicit, confirmed Prepare action can stage firmware.");
        let check = gtk::Button::with_label("Check BIOS updates…");
        check.set_halign(gtk::Align::Start);
        check.set_sensitive(v["firmware"]["client_installed"] == true);
        let weak = Rc::downgrade(self);
        check.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.check_bios();
            }
        });
        card.append(&check);
        let refresh = gtk::Button::with_label("Refresh update catalog");
        refresh.set_halign(gtk::Align::Start);
        refresh.set_sensitive(v["firmware"]["client_installed"] == true);
        let weak = Rc::downgrade(self);
        refresh.connect_clicked(move |_|if let Some(ui)=weak.upgrade(){if ui.busy.replace(true){return;}ui.status.set_text("Downloading firmware metadata from enabled remotes; no BIOS installation requested…");let tx=ui.tx.clone();std::thread::spawn(move ||{let result=firmware_update::refresh().map_err(|e|format!("{e:#}"));let _=tx.send(Event::Finished("Update catalog refreshed".into(),result));});});
        card.append(&refresh);
        self.note(&card,"Keep AC connected and laptop batteries at 30% or above. fwupd enforces additional model-specific requirements. ThinkHub never restarts the computer automatically.");
        let card = self.card("System firmware");
        self.path(&card, "BIOS version", "/hardware/system/bios_version");
        self.path(&card, "BIOS date", "/hardware/system/bios_date");
        self.path(
            &card,
            "BIOS settings pending restart",
            "/firmware/bios/pending_reboot",
        );
        self.note(&card,"Read cached fwupd device and update information. Metadata freshness is unknown. This does not refresh metadata, download, flash firmware or reboot.");
        let b = gtk::Button::with_label("Read firmware update information");
        b.set_sensitive(v["firmware"]["client_installed"] == true);
        let weak = Rc::downgrade(self);
        b.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.run(
                    "Firmware information",
                    vec!["firmware".into(), "scan".into()],
                );
            }
        });
        card.append(&b);
        let card = self.card("BIOS settings · read only");
        let read = gtk::Button::with_label("Read protected BIOS information…");
        read.set_sensitive(v["firmware"]["bios"]["interface_present"] == true);
        let weak = Rc::downgrade(self);
        read.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.run(
                    "BIOS information",
                    vec!["firmware".into(), "bios".into(), "--authorized".into()],
                );
            }
        });
        card.append(&read);
        for s in list(v, "/firmware/bios/settings") {
            self.row(
                &card,
                &field(s, "name"),
                if s["readable"] == true {
                    s["value"].as_str().unwrap_or("Unavailable")
                } else {
                    "Administrator read required"
                },
            );
        }
        self.note(
            &card,
            "BIOS editing is not available. Password settings are never queried.",
        );
    }
    fn set_appearance(&self, value: appearance::Appearance, feedback: &gtk::Label) {
        configure_theme(value.theme);
        self.css.load_from_data(&value.css());
        *self.appearance.borrow_mut() = value.clone();
        match appearance::path().and_then(|p| appearance::save(&p, &value)) {
            Ok(()) => {
                *self.appearance_error.borrow_mut() = None;
                feedback.set_text("Saved. Your appearance will be remembered next time.");
            }
            Err(e) => {
                feedback.set_text(&format!("Preview applied, but could not save: {e}"));
                *self.appearance_error.borrow_mut() = Some(e);
            }
        }
    }
    fn customize(self: &Rc<Self>) {
        let current = self.appearance.borrow().clone();
        let feedback = label(
            &self
                .appearance_error
                .borrow()
                .as_ref()
                .map(|e| format!("Using a preview; settings could not be loaded or saved: {e}"))
                .unwrap_or_else(|| "Changes apply immediately and are saved automatically.".into()),
            "muted",
        );
        let card = self.card("Theme");
        let choices = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let dark = gtk::ToggleButton::with_label("Dark");
        let light = gtk::ToggleButton::with_label("Light");
        light.set_group(Some(&dark));
        dark.set_active(current.theme == appearance::Theme::Dark);
        light.set_active(current.theme == appearance::Theme::Light);
        for (button, theme) in [
            (&dark, appearance::Theme::Dark),
            (&light, appearance::Theme::Light),
        ] {
            button.add_css_class("theme-choice");
            choices.append(button);
            let weak = Rc::downgrade(self);
            let feedback = feedback.clone();
            button.connect_toggled(move |button| {
                if !button.is_active() {
                    return;
                }
                if let Some(ui) = weak.upgrade() {
                    let mut value = ui.appearance.borrow().clone();
                    value.theme = theme;
                    ui.set_appearance(value, &feedback);
                }
            });
        }
        card.append(&choices);
        self.note(
            &card,
            "Choose the appearance for ThinkHub. Your desktop theme stays as it is.",
        );
        let card = self.card("Accent color");
        self.note(&card,"Use a preset, or pick any custom color. Buttons, highlights and focus indicators update together.");
        let names: Vec<&str> = appearance::PRESETS
            .iter()
            .map(|(name, _)| *name)
            .chain(["Custom color"])
            .collect();
        let presets = gtk::DropDown::from_strings(&names);
        presets.set_selected(
            appearance::PRESETS
                .iter()
                .position(|(_, hex)| hex.eq_ignore_ascii_case(&current.accent))
                .unwrap_or(8) as u32,
        );
        card.append(&presets);
        let picker = gtk::ColorButton::with_rgba(&gtk::gdk::RGBA::parse(&current.accent).unwrap());
        picker.set_use_alpha(false);
        picker.set_title("Choose ThinkHub's accent color");
        let custom = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let custom_label = label("Custom color", "muted");
        custom_label.set_wrap(false);
        custom.append(&custom_label);
        custom.append(&picker);
        let color = label(&current.accent, "accent");
        custom.append(&color);
        card.append(&custom);
        let weak = Rc::downgrade(self);
        let feedback_copy = feedback.clone();
        let picker_copy = picker.downgrade();
        let color_copy = color.clone();
        presets.connect_selected_notify(move |dropdown| {
            if let (Some(ui), Some((_, hex))) = (
                weak.upgrade(),
                appearance::PRESETS.get(dropdown.selected() as usize),
            ) {
                let mut value = ui.appearance.borrow().clone();
                value.accent = hex.to_string();
                if let Some(picker) = picker_copy.upgrade() {
                    picker.set_rgba(&gtk::gdk::RGBA::parse(*hex).unwrap());
                }
                color_copy.set_text(hex);
                ui.set_appearance(value, &feedback_copy);
            }
        });
        let weak = Rc::downgrade(self);
        let feedback_copy = feedback.clone();
        let presets_copy = presets.downgrade();
        picker.connect_color_set(move |picker| {
            if let Some(ui) = weak.upgrade() {
                let rgba = picker.rgba();
                let hex = format!(
                    "#{:02x}{:02x}{:02x}",
                    (rgba.red() * 255.).round() as u8,
                    (rgba.green() * 255.).round() as u8,
                    (rgba.blue() * 255.).round() as u8
                );
                let mut value = ui.appearance.borrow().clone();
                value.accent = hex.clone();
                if let Some(presets) = presets_copy.upgrade() {
                    presets.set_selected(8);
                }
                color.set_text(&hex);
                ui.set_appearance(value, &feedback_copy);
            }
        });
        let card = self.card("Preview");
        let preview = gtk::Button::with_label("Your accent color");
        preview.add_css_class("suggested-action");
        preview.set_halign(gtk::Align::Start);
        card.append(&preview);
        card.append(&label("Readable colors in light and dark themes", "accent"));
        self.note(&card,"Text tones adapt for contrast. Your chosen accent is preserved for buttons and highlights.");
        self.content.append(&feedback);
        let reset = gtk::Button::with_label("Restore default appearance");
        reset.set_halign(gtk::Align::Start);
        let weak = Rc::downgrade(self);
        reset.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.set_appearance(appearance::Appearance::default(), &feedback);
                ui.render();
            }
        });
        self.content.append(&reset);
    }
    fn preferences(self: &Rc<Self>, v: &Value) {
        let card = self.card("Saved preferences");
        self.note(&card,"Saving records a preference only. It does not apply hardware changes or install startup automation.");
        card.append(&label("Preferred power profile", "muted"));
        let profiles = ["No saved profile", "power-saver", "balanced", "performance"];
        let profile = gtk::DropDown::from_strings(&profiles);
        profile.set_selected(
            profiles
                .iter()
                .position(|p| Some(*p) == v["preferences"]["power_profile"].as_str())
                .unwrap_or(0) as u32,
        );
        card.append(&profile);
        let mut ids = std::collections::BTreeSet::new();
        for battery in list(v, "/hardware/batteries") {
            if let Some(id) = battery["id"].as_str() {
                ids.insert(id.to_owned());
            }
        }
        if let Some(saved) = v["preferences"]["battery_thresholds"].as_object() {
            ids.extend(saved.keys().cloned());
        }
        let mut editors = vec![];
        for id in ids {
            let enabled = gtk::CheckButton::with_label(&format!("Save charge thresholds for {id}"));
            let saved = &v["preferences"]["battery_thresholds"][&id];
            enabled.set_active(saved.is_object());
            card.append(&enabled);
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let start = gtk::SpinButton::with_range(0., 99., 1.);
            start.set_value(saved["start"].as_f64().unwrap_or(75.));
            let end = gtk::SpinButton::with_range(1., 100., 1.);
            end.set_value(saved["end"].as_f64().unwrap_or(80.));
            row.append(&label("Start %", "muted"));
            row.append(&start);
            row.append(&label("Stop %", "muted"));
            row.append(&end);
            row.set_sensitive(enabled.is_active());
            let weak = row.downgrade();
            enabled.connect_toggled(move |b| {
                if let Some(row) = weak.upgrade() {
                    row.set_sensitive(b.is_active());
                }
            });
            card.append(&row);
            editors.push((id, enabled, start, end));
        }
        let b = gtk::Button::with_label("Save preferences");
        b.set_halign(gtk::Align::Start);
        let weak = Rc::downgrade(self);
        b.connect_clicked(move |_| {
            if let Some(ui) = weak.upgrade() {
                let selected=profile.selected() as usize;
                let power=if selected==0 {Value::Null} else {json!(profiles[selected])};
                let mut thresholds=serde_json::Map::new();
                for (id,enabled,start,end) in &editors {if enabled.is_active(){thresholds.insert(id.clone(),json!({"start":start.value_as_int(),"end":end.value_as_int()}));}}
                let value=json!({"schema_version":1,"power_profile":power,"battery_thresholds":thresholds}).to_string();
                ui.run(
                    "Preferences saved",
                    vec!["settings".into(), "save".into(), value],
                );
            }
        });
        card.append(&b);
        if !v["preferences_error"].is_null() {
            self.note(&card, &field(v, "preferences_error"));
        }
        let card = self.card("Detected capabilities");
        for c in list(v, "/hardware/capabilities") {
            self.row(
                &card,
                &field(c, "id"),
                &format!("{} · {}", field(c, "status"), field(c, "reason")),
            );
        }
        let card = self.card("Diagnostics");
        for s in list(v, "/hardware/diagnostics")
            .iter()
            .chain(list(v, "/desktop/diagnostics"))
        {
            self.note(&card, &text(s));
        }
        self.note(&card,"This is an early public preview. Feature support varies by model, kernel and firmware. Use Feedback to report your experience.");
    }
}

fn topology(v: &Value) -> Value {
    // Rebuild editors only when hardware identities or control ownership changes.
    json!({"batteries":list(v,"/hardware/batteries").iter().map(|b|json!([b["id"],b["present"],b["model"],b["charge_modes"]])).collect::<Vec<_>>(),"brightness":list(v,"/hardware/brightness").iter().map(|b|json!([b["id"],b["maximum"]])).collect::<Vec<_>>(),"trackpoints":list(v,"/devices/trackpoints").iter().map(|b|json!([b["controller"],b["name"]])).collect::<Vec<_>>(),"sensors":list(v,"/hardware/sensors").iter().map(|s|json!([s["id"],s["label"],s["unit"]])).collect::<Vec<_>>(),"controllers":v["controllers"],"displays":v["devices"]["displays"],"inputs":v["devices"]["inputs"],"profiles":v["power"]["state"]["profiles"]})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_or_malformed_plan_never_enables_apply() {
        for v in [
            json!({}),
            json!({"plan":{},"blockers":null}),
            json!({"plan":{},"blockers":["Owned by Vantage"]}),
            json!({"plan":null,"blockers":[]}),
        ] {
            assert!(!plan_allowed(&v));
        }
        assert!(plan_allowed(&json!({"plan":{"changes":[]},"blockers":[]})));
    }
    #[test]
    fn unknown_controller_state_blocks_battery_changes() {
        let request = json!({"kind":"battery_thresholds","battery":"BAT0","start":40,"end":80});
        assert!(!blockers(&json!({}), &request).is_empty());
        assert!(blockers(
            &json!({}),
            &json!({"kind":"brightness","device":"panel","value":10})
        )
        .is_empty());
    }
    #[test]
    fn firmware_results_use_actual_cached_schema_and_distinguish_failure() {
        let output=result_summary("Firmware information",&json!({"devices":[],"cached_updates":[{"component":"BIOS","installed_version":"1","available_versions":["2"]}],"errors":[]}).to_string());
        assert!(output.contains("cached updates: 1"));
        assert!(output.contains("BIOS: 1 →"));
        let missing = result_summary(
            "Firmware information",
            &json!({"cached_updates":null,"errors":["service unavailable"]}).to_string(),
        );
        assert!(missing.contains("cached updates: Unavailable"));
        assert!(missing.contains("service unavailable"));
    }
    #[test]
    fn polling_does_not_reset_drafts_but_new_hardware_does() {
        let mut v = json!({"hardware":{"batteries":[{"id":"BAT0","present":true,"model":"battery","percent":50}]}});
        let before = topology(&v);
        v["hardware"]["batteries"][0]["percent"] = json!(51);
        assert_eq!(topology(&v), before);
        v["hardware"]["batteries"][0]["id"] = json!("BAT1");
        assert_ne!(topology(&v), before);
    }
}
