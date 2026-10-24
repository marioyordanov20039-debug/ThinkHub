//! Feedback drafts stay in memory and are handed to the user's email application.
use gtk::{gio, glib, prelude::*};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

pub const RECIPIENT: &str = "info.thinkhub@protonmail.com";
pub const CATEGORIES: [&str; 3] = ["Bug report", "Feature idea", "General feedback"];
const MESSAGE_LIMIT: usize = 1_000;

#[derive(Clone, Debug)]
pub struct Draft {
    pub category: u32,
    pub subject: String,
    pub message: String,
    pub include_system: bool,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            category: 2,
            subject: String::new(),
            message: String::new(),
            include_system: false,
        }
    }
}

#[derive(Debug)]
pub struct Prepared {
    pub subject: String,
    pub body: String,
    pub uri: String,
}
impl Prepared {
    pub fn copy_text(&self) -> String {
        format!(
            "To: {RECIPIENT}\nSubject: {}\n\n{}",
            self.subject, self.body
        )
    }
}

pub fn prepare(draft: &Draft, snapshot: &Value) -> Result<Prepared, String> {
    let category = CATEGORIES
        .get(draft.category as usize)
        .ok_or("Choose a feedback type.")?;
    let subject = draft.subject.trim();
    let message = draft.message.trim();
    if subject.is_empty()
        || subject.chars().count() > 120
        || draft.subject.chars().any(char::is_control)
    {
        return Err("Enter a subject of 1–120 characters, without line breaks.".into());
    }
    if message.is_empty() || message.chars().count() > MESSAGE_LIMIT {
        return Err(format!("Enter a message of 1–{MESSAGE_LIMIT} characters. For a longer report, continue writing in your email app."));
    }
    if message
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err(
            "The message contains unsupported control characters. Remove them and try again."
                .into(),
        );
    }
    let subject = format!("[ThinkHub feedback] {category}: {subject}");
    let mut body = format!(
        "{}\n\n---\nThinkHub {}\nFeedback type: {category}",
        message.replace("\r\n", "\n").replace('\r', "\n"),
        env!("CARGO_PKG_VERSION")
    );
    if draft.include_system {
        for (name, pointer) in [
            ("ThinkPad model", "/hardware/system/model"),
            ("Linux distribution", "/overview/os_name"),
            ("Kernel", "/overview/kernel"),
        ] {
            let value = snapshot
                .pointer(pointer)
                .and_then(Value::as_str)
                .map(|s| {
                    s.chars()
                        .filter(|c| !c.is_control())
                        .take(160)
                        .collect::<String>()
                })
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "Unavailable".into());
            body.push_str(&format!("\n{name}: {value}"));
        }
    }
    // RFC 6068: encode each field independently; user text cannot add recipients/headers.
    let encoded_subject = glib::uri_escape_string(&subject, None::<&str>, false);
    let encoded_body = glib::uri_escape_string(body.replace('\n', "\r\n"), None::<&str>, false);
    let uri = format!("mailto:{RECIPIENT}?subject={encoded_subject}&body={encoded_body}");
    Ok(Prepared { subject, body, uri })
}

fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class(class);
    label
}

pub fn page(
    parent: &gtk::ApplicationWindow,
    draft: Rc<RefCell<Draft>>,
    snapshot: Value,
) -> gtk::Box {
    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    form.add_css_class("card");
    form.append(&label("Help improve ThinkHub", "card-title"));
    form.append(&label(&format!("Feedback goes to {RECIPIENT}. Review your message here, then send it from your email app."), "muted"));
    form.append(&label("Feedback type", "muted"));
    let category = gtk::DropDown::from_strings(&CATEGORIES);
    category.set_selected(draft.borrow().category);
    form.append(&category);
    let saved = draft.clone();
    category.connect_selected_notify(move |widget| saved.borrow_mut().category = widget.selected());
    form.append(&label("Subject", "muted"));
    let subject = gtk::Entry::builder()
        .placeholder_text("What would you like to tell us?")
        .max_length(120)
        .build();
    subject.set_text(&draft.borrow().subject);
    form.append(&subject);
    let saved = draft.clone();
    subject.connect_changed(move |widget| saved.borrow_mut().subject = widget.text().to_string());
    form.append(&label("Message", "muted"));
    let message = gtk::TextView::builder()
        .wrap_mode(gtk::WrapMode::WordChar)
        .top_margin(10)
        .bottom_margin(10)
        .left_margin(10)
        .right_margin(10)
        .build();
    message.buffer().set_text(&draft.borrow().message);
    let scroll = gtk::ScrolledWindow::builder()
        .min_content_height(170)
        .child(&message)
        .build();
    form.append(&scroll);
    let count = label(
        &format!(
            "{} / {MESSAGE_LIMIT} characters",
            draft.borrow().message.chars().count()
        ),
        "muted",
    );
    form.append(&count);
    let saved = draft.clone();
    message.buffer().connect_changed(move |buffer| {
        let value = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string();
        count.set_text(&format!(
            "{} / {MESSAGE_LIMIT} characters",
            value.trim().chars().count()
        ));
        saved.borrow_mut().message = value;
    });
    let system =
        gtk::CheckButton::with_label("Include ThinkPad model, Linux distribution and kernel");
    if let Some(text) = system.last_child().and_downcast::<gtk::Label>() {
        text.set_wrap(true);
        text.set_xalign(0.0);
    }
    system.set_active(draft.borrow().include_system);
    form.append(&system);
    let saved = draft.clone();
    system.connect_toggled(move |widget| saved.borrow_mut().include_system = widget.is_active());
    form.append(&label("Your app version is included. System details are optional. No logs, serial numbers or settings files are attached automatically. Avoid passwords and recovery keys. Your email app supplies your sender address.", "muted"));
    form.append(&label("The draft stays in memory while ThinkHub is open. Nothing is sent until you press Send in your email app.", "muted"));
    let status = label("", "notice");
    let review = gtk::Button::with_label("Review feedback");
    review.set_halign(gtk::Align::Start);
    review.add_css_class("suggested-action");
    let parent = parent.downgrade();
    let result = status.clone();
    review.connect_clicked(move |_| match prepare(&draft.borrow(), &snapshot) {
        Ok(prepared) => {
            if let Some(parent) = parent.upgrade() {
                result.set_text("");
                review_window(&parent, prepared);
            }
        }
        Err(error) => result.set_text(&error),
    });
    form.append(&review);
    form.append(&status);
    form
}

fn review_window(parent: &gtk::ApplicationWindow, prepared: Prepared) {
    let window = gtk::Window::builder()
        .title("Review feedback")
        .transient_for(parent)
        .modal(true)
        .default_width(680)
        .default_height(580)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.add_css_class("page");
    body.append(&label("Review your email", "card-title"));
    let view = gtk::TextView::builder()
        .editable(false)
        .cursor_visible(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    view.buffer().set_text(&prepared.copy_text());
    body.append(
        &gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&view)
            .build(),
    );
    body.append(&label("Open your email app, check the message and press Send there. Opening the draft does not send it, and ThinkHub cannot confirm delivery.", "muted"));
    let status = label("No message has been sent.", "notice");
    body.append(&status);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let close = gtk::Button::with_label("Back");
    let weak = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(w) = weak.upgrade() {
            w.close();
        }
    });
    buttons.append(&close);
    let copy = gtk::Button::with_label("Copy email");
    let text = prepared.copy_text();
    let feedback = status.clone();
    copy.connect_clicked(move |button| {
        button.display().clipboard().set_text(&text);
        feedback.set_text("Email copied. Paste it into your email app and send it to info.thinkhub@protonmail.com.");
    });
    buttons.append(&copy);
    let open = gtk::Button::with_label("Open email app");
    open.add_css_class("suggested-action");
    let feedback = status.clone();
    open.connect_clicked(move |button| {
        button.set_sensitive(false);
        feedback.set_text("Opening your email app…");
        let context = button.display().app_launch_context();
        let feedback = feedback.clone();
        let button = button.downgrade();
        gio::AppInfo::launch_default_for_uri_async(&prepared.uri, Some(&context), None::<&gio::Cancellable>, move |result| {
            if let Some(button) = button.upgrade() { button.set_sensitive(true); }
            feedback.set_text(&match result {
                Ok(()) => "Check your email app for the draft, then press Send there. ThinkHub cannot confirm delivery.".to_owned(),
                Err(error) => format!("Could not open your email app: {error}. Use Copy email, or configure your desktop’s default email app. Your draft is still here."),
            });
        });
    });
    buttons.append(&open);
    body.append(&buttons);
    window.set_child(Some(&body));
    window.present();
}

#[cfg(test)]
mod tests {
    use super::*;
    fn draft() -> Draft {
        Draft {
            subject: "A test & idea?".into(),
            message: "Hello café 🐧\nSecond line &bcc=other@example.com".into(),
            ..Draft::default()
        }
    }
    #[test]
    fn recipient_is_fixed_and_unicode_and_query_characters_round_trip() {
        let mail = prepare(&draft(), &Value::Null).unwrap();
        assert!(mail
            .uri
            .starts_with(&format!("mailto:{RECIPIENT}?subject=")));
        assert_eq!(mail.uri.matches('&').count(), 1);
        let (subject, body) = mail
            .uri
            .split_once("?subject=")
            .unwrap()
            .1
            .split_once("&body=")
            .unwrap();
        assert_eq!(
            glib::uri_unescape_string(subject, None::<&str>).unwrap(),
            mail.subject
        );
        assert_eq!(
            glib::uri_unescape_string(body, None::<&str>).unwrap(),
            mail.body.replace('\n', "\r\n")
        );
        assert!(mail.uri.contains("%0D%0A"));
    }
    #[test]
    fn empty_oversized_and_header_injection_drafts_are_rejected() {
        assert!(prepare(&Draft::default(), &Value::Null).is_err());
        for subject in [
            "title\r\nBcc: other@example.com".to_owned(),
            "x".repeat(121),
            "a\0b".into(),
        ] {
            let mut d = draft();
            d.subject = subject;
            assert!(prepare(&d, &Value::Null).is_err());
        }
        for message in [" ".to_owned(), "x".repeat(1001), "a\0b".into()] {
            let mut d = draft();
            d.message = message;
            assert!(prepare(&d, &Value::Null).is_err());
        }
        let mut d = draft();
        d.category = 999;
        assert!(prepare(&d, &Value::Null).is_err());
    }
    #[test]
    fn optional_system_context_is_allowlisted_and_off_by_default() {
        let snapshot = serde_json::json!({"hardware":{"system":{"model":"ThinkPad T480","serial":"PRIVATE-SERIAL"}},"overview":{"os_name":"Test Linux","kernel":"test\nheader","username":"PRIVATE-USER"},"preferences":{"secret":"PRIVATE-SETTINGS"}});
        let mut d = draft();
        let mail = prepare(&d, &snapshot).unwrap();
        assert!(!mail.body.contains("T480"));
        d.include_system = true;
        let mail = prepare(&d, &snapshot).unwrap();
        assert!(mail.body.contains("ThinkPad model: ThinkPad T480"));
        assert!(mail.body.contains("Kernel: testheader"));
        assert!(!mail.body.contains("PRIVATE-"));
        assert!(mail.copy_text().starts_with(&format!("To: {RECIPIENT}\n")));
    }
}
