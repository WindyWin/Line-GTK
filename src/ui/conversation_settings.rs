use super::*;
use crate::protocol::{ConversationDetails as Details, ConversationPerson as Person};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Request {
    Details,
    Candidates,
    Invite,
}

#[derive(Clone)]
struct Ui {
    window: gtk::Window,
    account: String,
    mid: String,
    generation: u64,
    status: gtk::Label,
    refresh: gtk::Button,
    invite: gtk::Button,
    submit: gtk::Button,
    cancel: gtk::Button,
    spinner: gtk::Spinner,
    members: gtk::ListBox,
    invitees: gtk::ListBox,
    member_group: libadwaita::PreferencesGroup,
    invitee_group: libadwaita::PreferencesGroup,
    picker: gtk::Box,
    candidates: gtk::ListBox,
    selected: Rc<RefCell<HashSet<String>>>,
    busy: Rc<Cell<bool>>,
    loaded: Rc<Cell<bool>>,
    search: gtk::SearchEntry,
    search_friends: gtk::SearchEntry,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

pub(super) fn close() {
    let ui = UI.with(|slot| slot.borrow_mut().take());
    if let Some(ui) = ui {
        ui.window.destroy();
    }
}

fn active(state: &AppState, ui: &Ui) -> bool {
    *state.session_ready.borrow()
        && state.self_mid.borrow().as_deref() == Some(&ui.account)
        && UI.with(|slot| {
            slot.borrow()
                .as_ref()
                .is_some_and(|current| current.generation == ui.generation)
        })
}

fn set_busy(ui: &Ui, busy: bool) {
    ui.busy.set(busy);
    ui.spinner.set_spinning(busy);
    ui.spinner.set_visible(busy);
    ui.refresh.set_sensitive(!busy);
    ui.invite.set_sensitive(!busy && ui.loaded.get());
    ui.candidates.set_sensitive(!busy);
    ui.cancel.set_sensitive(!busy);
    let count = ui.selected.borrow().len();
    ui.submit.set_sensitive(!busy && (1..=100).contains(&count));
}

fn send(state: &AppState, ui: &Ui, request: Request) {
    if !active(state, ui) || ui.busy.get() {
        return;
    }
    let method = match request {
        Request::Details => "get_conversation_settings",
        Request::Candidates => "get_group_invite_candidates",
        Request::Invite => "invite_chat_members",
    };
    let mut mids: Vec<_> = ui.selected.borrow().iter().cloned().collect();
    mids.sort();
    if request == Request::Invite && !(1..=100).contains(&mids.len()) {
        return;
    }
    if request != Request::Invite {
        ui.selected.borrow_mut().clear();
        clear_list(&ui.candidates);
        ui.picker.set_visible(request == Request::Candidates);
    }
    ui.status
        .set_text(&crate::i18n::t(if request == Request::Invite {
            "group_inviting"
        } else {
            "group_settings_loading"
        }));
    ui.status.set_visible(true);
    set_busy(ui, true);
    match state.sidecar.request(
        method,
        Some(serde_json::json!({"chatMid": ui.mid, "mids": mids})),
    ) {
        Ok(id) => {
            state.pending.borrow_mut().insert(
                id,
                Pending::ConversationSettings {
                    account: ui.account.clone(),
                    chat_mid: ui.mid.clone(),
                    generation: ui.generation,
                    request,
                },
            );
        }
        Err(_) => failed(ui, request),
    }
}

fn failed(ui: &Ui, request: Request) {
    set_busy(ui, false);
    ui.status
        .set_text(&crate::i18n::t(if request == Request::Invite {
            "group_invite_failed"
        } else {
            "group_settings_failed"
        }));
    ui.status.set_visible(true);
}

fn list() -> gtk::ListBox {
    gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build()
}

fn person_row(person: &Person, account: &str) -> libadwaita::ActionRow {
    libadwaita::ActionRow::builder()
        .title(&person.name)
        .use_markup(false)
        .title_lines(2)
        .subtitle(if person.mid == account {
            crate::i18n::t("group_member_you")
        } else {
            String::new()
        })
        .build()
}

fn filter_list(list: &gtk::ListBox, term: &str) {
    let term = term.trim().to_lowercase();
    let mut child = list.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(row) = widget.downcast::<libadwaita::ActionRow>() {
            row.set_visible(row.title().to_lowercase().contains(&term));
        }
    }
}

fn shortcut(group: &libadwaita::PreferencesGroup, key: &str, action: impl Fn() + 'static) {
    let row = libadwaita::ActionRow::builder()
        .title(crate::i18n::t(key))
        .use_markup(false)
        .activatable(true)
        .build();
    let icon = gtk::Image::from_icon_name("go-next-symbolic");
    icon.set_accessible_role(gtk::AccessibleRole::Presentation);
    row.add_suffix(&icon);
    row.connect_activated(move |_| action());
    group.add(&row);
}

pub(super) fn wire(state: &AppState) {
    let action = gio::SimpleAction::new("conversation-settings", Some(glib::VariantTy::STRING));
    let s = state.clone();
    action.connect_activate(move |_, value| {
        if let Some(mid) = value.and_then(|value| value.str()) {
            open(&s, mid);
        }
    });
    state.window.add_action(&action);
    let action = gio::SimpleAction::new("current-conversation-settings", None);
    let s = state.clone();
    action.connect_activate(move |_, _| {
        let mid = s.current_chat.borrow().clone();
        if let Some(mid) = mid {
            open(&s, &mid);
        } else {
            toast(&s, &crate::i18n::t("select_chat"));
        }
    });
    state.window.add_action(&action);
}

fn open(state: &AppState, mid: &str) {
    if !*state.session_ready.borrow() {
        toast(state, &crate::i18n::t("still_restoring"));
        return;
    }
    let Some(account) = state.self_mid.borrow().clone() else {
        return;
    };
    let Some(chat) = state.chats.borrow().iter().find(|c| c.mid == mid).cloned() else {
        return;
    };
    close();
    let generation = GENERATION.with(|counter| {
        counter.set(counter.get().wrapping_add(1));
        counter.get()
    });
    let window = gtk::Window::builder()
        .transient_for(&state.window)
        .destroy_with_parent(true)
        .modal(true)
        .title(crate::i18n::t("conversation_settings"))
        .default_width(480)
        .default_height(680)
        .build();
    window.set_titlebar(Some(&libadwaita::HeaderBar::new()));
    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(24)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(20)
        .margin_end(20)
        .build();
    let title = gtk::Label::builder()
        .label(&chat.name)
        .wrap(true)
        .xalign(0.0)
        .css_classes(["title-1"])
        .build();
    body.append(&title);
    let group_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .visible(mid.starts_with('c'))
        .build();
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let invite = gtk::Button::with_label(&crate::i18n::t("group_invite_friends"));
    invite.add_css_class("suggested-action");
    let refresh = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text(crate::i18n::t("group_settings_refresh"))
        .build();
    toolbar.append(&invite);
    toolbar.append(&refresh);
    let spinner = gtk::Spinner::new();
    toolbar.append(&spinner);
    group_box.append(&toolbar);
    let status = gtk::Label::builder().wrap(true).xalign(0.0).build();
    group_box.append(&status);
    let picker = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .visible(false)
        .build();
    let search_friends = gtk::SearchEntry::builder()
        .placeholder_text(crate::i18n::t("friends_search"))
        .build();
    let candidates = list();
    let submit = gtk::Button::with_label(&crate::i18n::t("group_send_invitations"));
    submit.add_css_class("suggested-action");
    submit.set_hexpand(true);
    let cancel = gtk::Button::with_label(&crate::i18n::t("contact_block_cancel"));
    let picker_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    picker_actions.append(&cancel);
    picker_actions.append(&submit);
    picker.append(&search_friends);
    picker.append(&candidates);
    picker.append(&picker_actions);
    group_box.append(&picker);
    let search = gtk::SearchEntry::builder()
        .placeholder_text(crate::i18n::t("group_search_people"))
        .build();
    group_box.append(&search);
    let members = list();
    let member_group = libadwaita::PreferencesGroup::builder()
        .title(crate::i18n::t("mention_members"))
        .build();
    member_group.add(&members);
    group_box.append(&member_group);
    let invitees = list();
    let invitee_group = libadwaita::PreferencesGroup::builder()
        .title(crate::i18n::t("group_pending_invitations"))
        .build();
    invitee_group.add(&invitees);
    group_box.append(&invitee_group);
    body.append(&group_box);
    let general = libadwaita::PreferencesGroup::new();
    let pinned = state.config.borrow().is_chat_pinned(&account, mid);
    for (key, action) in [
        (
            if pinned { "unpin_chat" } else { "pin_chat" },
            "win.toggle-chat-pin",
        ),
        ("hide_chat", "win.hide-chat"),
    ] {
        let s = state.clone();
        let mid = mid.to_string();
        let account = account.clone();
        shortcut(&general, key, move || {
            if s.self_mid.borrow().as_deref() == Some(&account) && *s.session_ready.borrow() {
                close();
                let _ = gtk::prelude::WidgetExt::activate_action(
                    &s.window,
                    action,
                    Some(&mid.to_variant()),
                );
            }
        });
    }
    if mid.starts_with('u') || mid.starts_with('c') {
        let s = state.clone();
        let mid = mid.to_string();
        let account = account.clone();
        shortcut(
            &general,
            if chat.muted {
                "unmute_chat"
            } else {
                "mute_chat"
            },
            move || {
                if s.self_mid.borrow().as_deref() == Some(&account) && *s.session_ready.borrow() {
                    close();
                    if let Err(error) = s.sidecar.mute_chat(&mid, !chat.muted) {
                        toast(
                            &s,
                            &crate::i18n::tf("mute_failed", &[("error", &error.to_string())]),
                        );
                    }
                }
            },
        );
    }
    if state.current_chat.borrow().as_deref() == Some(mid) {
        let s = state.clone();
        shortcut(&general, "shared_media", move || {
            close();
            shared_media::open(&s);
        });
        if mid.starts_with('c') {
            let s = state.clone();
            shortcut(&general, "albums", move || {
                close();
                s.album_btn.emit_clicked();
            });
        }
    }
    body.append(&general);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&body)
        .build();
    window.set_child(Some(&scroll));
    let ui = Ui {
        window: window.clone(),
        account,
        mid: mid.into(),
        generation,
        status,
        refresh,
        invite,
        submit,
        cancel,
        spinner,
        members,
        invitees,
        member_group,
        invitee_group,
        picker,
        candidates,
        selected: Rc::new(RefCell::new(HashSet::new())),
        busy: Rc::new(Cell::new(false)),
        loaded: Rc::new(Cell::new(false)),
        search: search.clone(),
        search_friends: search_friends.clone(),
    };
    let members = ui.members.clone();
    let invitees = ui.invitees.clone();
    search.connect_search_changed(move |entry| {
        filter_list(&members, &entry.text());
        filter_list(&invitees, &entry.text());
    });
    let list = ui.candidates.clone();
    search_friends.connect_search_changed(move |entry| filter_list(&list, &entry.text()));
    ui.cancel.connect_clicked(move |_| {
        let ui = UI.with(|slot| slot.borrow().clone());
        if let Some(ui) = ui.filter(|ui| ui.generation == generation && !ui.busy.get()) {
            ui.picker.set_visible(false);
            ui.selected.borrow_mut().clear();
            ui.status.set_visible(false);
            ui.invite.grab_focus();
            set_busy(&ui, false);
        }
    });
    for (button, request) in [
        (&ui.refresh, Request::Details),
        (&ui.invite, Request::Candidates),
        (&ui.submit, Request::Invite),
    ] {
        let s = state.clone();
        button.connect_clicked(move |_| {
            let ui = UI.with(|slot| slot.borrow().clone());
            if let Some(ui) = ui.filter(|ui| ui.generation == generation) {
                send(&s, &ui, request);
            }
        });
    }
    window.connect_close_request(move |_| {
        close();
        glib::Propagation::Stop
    });
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(|_, key, _, _| {
        if key == gdk::Key::Escape {
            close();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);
    UI.with(|slot| *slot.borrow_mut() = Some(ui.clone()));
    if mid.starts_with('c') {
        send(state, &ui, Request::Details);
    }
    window.present();
}

pub(super) fn response(
    state: &AppState,
    account: &str,
    mid: &str,
    generation: u64,
    request: Request,
    ok: bool,
    result: &serde_json::Value,
) {
    let Some(ui) = UI.with(|slot| slot.borrow().clone()) else {
        return;
    };
    if ui.account != account || ui.mid != mid || ui.generation != generation || !active(state, &ui)
    {
        return;
    }
    if !ok || result.get("chatMid").and_then(|v| v.as_str()) != Some(mid) {
        failed(&ui, request);
        return;
    }
    set_busy(&ui, false);
    match request {
        Request::Details => {
            let Ok(details) = serde_json::from_value::<Details>(result.clone()) else {
                failed(&ui, request);
                return;
            };
            if details.chat_mid != mid {
                return;
            }
            clear_list(&ui.members);
            clear_list(&ui.invitees);
            for person in &details.members {
                let row = person_row(person, account);
                row.set_activatable(true);
                let s = state.clone();
                let mid = person.mid.clone();
                row.connect_activated(move |_| {
                    close();
                    user_profile::open(&s, &mid);
                });
                ui.members.append(&row);
            }
            for person in &details.invitees {
                ui.invitees.append(&person_row(person, account));
            }
            filter_list(&ui.members, &ui.search.text());
            filter_list(&ui.invitees, &ui.search.text());
            ui.member_group.set_title(&crate::i18n::tf(
                "group_members_count",
                &[("n", &details.members.len().to_string())],
            ));
            ui.invitee_group.set_title(&crate::i18n::tf(
                "group_invitees_count",
                &[("n", &details.invitees.len().to_string())],
            ));
            let description = details
                .invitees
                .is_empty()
                .then(|| crate::i18n::t("group_no_invitations"));
            ui.invitee_group.set_description(description.as_deref());
            ui.window.set_title(Some(&format!(
                "{} — {}",
                details.name,
                crate::i18n::t("conversation_settings")
            )));
            ui.status.set_text("");
            ui.status.set_visible(false);
            ui.loaded.set(true);
            set_busy(&ui, false);
        }
        Request::Candidates => {
            let Some(people) = result
                .get("candidates")
                .cloned()
                .and_then(|v| serde_json::from_value::<Vec<Person>>(v).ok())
            else {
                failed(&ui, request);
                return;
            };
            ui.status.set_text(&crate::i18n::t(if people.is_empty() {
                "group_no_candidates"
            } else {
                "group_choose_friends"
            }));
            for person in people {
                let row = person_row(&person, account);
                let check = gtk::CheckButton::builder()
                    .valign(gtk::Align::Center)
                    .build();
                check.update_property(&[gtk::accessible::Property::Label(&person.name)]);
                let selected = ui.selected.clone();
                let submit = ui.submit.clone();
                check.connect_toggled(move |check| {
                    let mut selected = selected.borrow_mut();
                    if check.is_active() {
                        selected.insert(person.mid.clone());
                    } else {
                        selected.remove(&person.mid);
                    }
                    submit.set_sensitive(!selected.is_empty() && selected.len() <= 100);
                });
                row.add_suffix(&check);
                row.set_activatable_widget(Some(&check));
                ui.candidates.append(&row);
            }
            filter_list(&ui.candidates, &ui.search_friends.text());
            ui.search_friends.grab_focus();
        }
        Request::Invite => {
            toast(state, &crate::i18n::t("group_invite_done"));
            send(state, &ui, Request::Details);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui() -> Ui {
        UI.with(|slot| slot.borrow().clone().unwrap())
    }

    fn settle() {
        let until = std::time::Instant::now() + std::time::Duration::from_millis(180);
        while std::time::Instant::now() < until {
            while glib::MainContext::default().pending() {
                glib::MainContext::default().iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn screenshot(window: &gtk::Window, name: &str) {
        let Some(directory) = std::env::var_os("LINE_GTK_SETTINGS_SCREENSHOTS") else {
            return;
        };
        settle();
        let paintable = gtk::WidgetPaintable::new(Some(window));
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, window.width() as f64, window.height() as f64);
        let node = snapshot.to_node().expect("rendered settings");
        let texture = window.renderer().unwrap().render_texture(&node, None);
        std::fs::create_dir_all(&directory).unwrap();
        texture
            .save_to_png(PathBuf::from(directory).join(format!("{name}.png")))
            .unwrap();
    }

    #[test]
    #[ignore = "requires a GTK display and isolated XDG directories; run with --ignored --test-threads=1"]
    fn group_settings_picker_refresh_failure_and_stale_responses() {
        use std::os::unix::fs::PermissionsExt;
        libadwaita::init().unwrap();
        let root =
            std::env::temp_dir().join(format!("linegtk-group-settings-{}", std::process::id()));
        std::fs::create_dir_all(root.join("protocol")).unwrap();
        let sidecar = root.join("protocol/line-gtk-protocol");
        // Consume requests without connecting to LINE. Responses below are deterministic fixtures.
        std::fs::write(
            &sidecar,
            "#!/bin/sh\nwhile IFS= read -r request; do :; done\n",
        )
        .unwrap();
        std::fs::set_permissions(&sidecar, std::fs::Permissions::from_mode(0o700)).unwrap();
        let assets = root.join("assets");
        if !assets.exists() {
            std::os::unix::fs::symlink(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
                &assets,
            )
            .unwrap();
        }
        let config = AppConfig {
            language: "en".into(),
            tray_enabled: false,
            autostart: false,
            animations: false,
            ..AppConfig::default()
        };
        config.save(&root.join("data"));
        let app = Application::builder()
            .application_id("dev.linegtk.GroupSettingsTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let state = super::super::build_ui(&app, root.clone(), root.join("data"), false).unwrap();
        *state.self_mid.borrow_mut() = Some("u-self".into());
        *state.session_ready.borrow_mut() = true;
        let chat: ChatInfo = serde_json::from_value(serde_json::json!({
            "mid": "c-group", "name": "Weekend plans", "kind": "group",
        }))
        .unwrap();
        apply_chats(&state, vec![chat], false);
        open(&state, "c-group");
        let first_generation = ui().generation;
        let details = serde_json::json!({
            "chatMid": "c-group", "name": "Weekend plans",
            "members": [{"mid":"u-self","name":"Alex"}, {"mid":"u-member","name":"Jamie <&>"}],
            "invitees": [{"mid":"u-pending","name":"Taylor"}],
        });
        response(
            &state,
            "u-self",
            "c-group",
            first_generation,
            Request::Details,
            true,
            &details,
        );
        assert_eq!(ui().member_group.title(), "Members (2)");
        assert_eq!(ui().invitee_group.title(), "Invited (1)");
        ui().search.set_text("Jamie");
        settle();
        assert!(!ui().members.row_at_index(0).unwrap().is_visible());
        assert!(ui().members.row_at_index(1).unwrap().is_visible());
        ui().search.set_text("");
        crate::config::apply_theme("light");
        screenshot(&ui().window, "members-light");
        ui().invite.emit_clicked();
        assert!(ui().busy.get());
        assert!(!ui().submit.is_sensitive());
        let candidates = serde_json::json!({
            "chatMid": "c-group", "candidates": [{"mid":"u-friend","name":"Morgan"}],
        });
        response(
            &state,
            "u-self",
            "c-group",
            first_generation,
            Request::Candidates,
            true,
            &candidates,
        );
        let row = ui()
            .candidates
            .row_at_index(0)
            .unwrap()
            .downcast::<libadwaita::ActionRow>()
            .unwrap();
        row.activatable_widget()
            .unwrap()
            .downcast::<gtk::CheckButton>()
            .unwrap()
            .set_active(true);
        assert!(ui().submit.is_sensitive());
        screenshot(&ui().window, "invite-light");
        ui().submit.emit_clicked();
        assert!(ui().busy.get());
        assert!(!ui().candidates.is_sensitive());
        response(
            &state,
            "u-self",
            "c-group",
            first_generation,
            Request::Invite,
            false,
            &serde_json::Value::Null,
        );
        assert!(!ui().busy.get());
        assert!(ui().selected.borrow().contains("u-friend"));
        assert!(ui().submit.is_sensitive());
        ui().submit.emit_clicked();
        response(
            &state,
            "u-self",
            "c-group",
            first_generation,
            Request::Invite,
            true,
            &serde_json::json!({"chatMid":"c-group","invited":["u-friend"]}),
        );
        assert!(ui().busy.get()); // Successful invitation reloads the roster.
        assert!(!ui().picker.is_visible());
        assert!(ui().selected.borrow().is_empty());
        response(
            &state,
            "u-self",
            "c-group",
            first_generation,
            Request::Details,
            true,
            &details,
        );
        crate::config::apply_theme("dark");
        screenshot(&ui().window, "members-dark");
        ui().window.set_default_size(380, 520);
        screenshot(&ui().window, "members-narrow");
        open(&state, "c-group");
        assert_ne!(ui().generation, first_generation);
        response(
            &state,
            "u-self",
            "c-group",
            first_generation,
            Request::Details,
            true,
            &details,
        );
        assert!(!ui().loaded.get());
        *state.self_mid.borrow_mut() = Some("u-other".into());
        response(
            &state,
            "u-self",
            "c-group",
            ui().generation,
            Request::Details,
            true,
            &details,
        );
        assert!(!ui().loaded.get());
        close();
        state.window.destroy();
    }
}
