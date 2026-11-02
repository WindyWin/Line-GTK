use super::*;

#[test]
fn candidates_include_members_beyond_the_first_thirty_and_filter_the_entire_group() {
    let members: Vec<_> = (0..120)
        .map(|i| Member {
            mid: format!("u-person-{i}"),
            name: format!("Person {i:03}"),
            all: false,
            avatar_path: None,
        })
        .collect();
    let options = candidates(&members, Some("u-person-0"), "");
    assert_eq!(options.len(), 120);
    assert!(options[0].all);
    assert!(!options.iter().any(|m| m.mid == "u-person-0"));
    assert_eq!(options.last().unwrap().mid, "u-person-119");
    let options = candidates(&members, None, "person 119");
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].mid, "u-person-119");
}

fn settle() {
    let context = glib::MainContext::default();
    let until = std::time::Instant::now() + std::time::Duration::from_millis(250);
    while std::time::Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn key(ui: &PickerUi, key: gdk::Key) -> bool {
    ui.keys
        .emit_by_name::<bool>("key-pressed", &[&key, &0u32, &gdk::ModifierType::empty()])
}

fn complete_request(state: &AppState, ok: bool, members: serde_json::Value) {
    let (id, account, chat, generation) = state
        .pending
        .borrow()
        .iter()
        .find_map(|(id, pending)| match pending {
            Pending::MentionMembers {
                account,
                chat_mid,
                generation,
            } => Some((*id, account.clone(), chat_mid.clone(), *generation)),
            _ => None,
        })
        .expect("a member lookup is pending");
    state.pending.borrow_mut().remove(&id);
    response(
        state,
        &account,
        &chat,
        generation,
        ok,
        &serde_json::json!({"members": members}),
    );
    settle();
}

fn assert_selected_visible(ui: &PickerUi, index: i32) {
    let row = ui.list.selected_row().unwrap();
    assert_eq!(row.index(), index);
    let bounds = row.compute_bounds(&ui.list).unwrap();
    let adjustment = ui.scroll.vadjustment();
    assert!(bounds.height() > 0.0);
    assert!(bounds.y() as f64 >= adjustment.value() - 1.0);
    assert!(
        (bounds.y() + bounds.height()) as f64 <= adjustment.value() + adjustment.page_size() + 1.0,
        "row {index} must be completely visible"
    );
}

fn screenshot(widget: &impl IsA<gtk::Widget>, path: &std::path::Path) {
    let widget = widget.as_ref();
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, widget.width() as f64, widget.height() as f64);
    let renderer = gtk::gsk::CairoRenderer::new();
    renderer.realize(None::<&gdk::Surface>).unwrap();
    let texture = renderer.render_texture(snapshot.to_node().unwrap(), None);
    texture.save_to_png(path).unwrap();
    renderer.unrealize();
}

#[test]
#[ignore = "requires a desktop display and isolated XDG directories; uses a fixture sidecar"]
fn fixture_picker_scroll_loading_avatars_retry_and_stale_responses() {
    use std::os::unix::fs::PermissionsExt;
    libadwaita::init().unwrap();
    let root = std::env::temp_dir().join(format!("linegtk-mentions-ui-{}", std::process::id()));
    let protocol = root.join("protocol");
    let data = root.join("data");
    std::fs::create_dir_all(&protocol).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    std::os::unix::fs::symlink(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"),
        root.join("assets"),
    )
    .unwrap();
    AppConfig {
        language: "en".into(),
        animations: false,
        tray_enabled: false,
        close_to_tray: false,
        ..AppConfig::default()
    }
    .save(&data);
    let binary = protocol.join("line-gtk-protocol");
    std::fs::write(
        &binary,
        "#!/usr/bin/env python3\nimport json, sys\nfor line in sys.stdin:\n    if json.loads(line)['method'] == 'fixture_exit': break\n",
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let app = Application::builder()
        .application_id("dev.linegtk.MentionsFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let state = build_ui(&app, root.clone(), data, false).unwrap();
    *state.self_mid.borrow_mut() = Some("u-fixture".into());
    *state.session_ready.borrow_mut() = true;
    state.stack.set_visible_child_name("shell");
    let first: ChatInfo = serde_json::from_value(serde_json::json!({
        "mid": "c-first", "name": "Mention picker fixture", "kind": "group", "unread": 0,
    }))
    .unwrap();
    let second = ChatInfo {
        mid: "c-second".into(),
        ..first.clone()
    };
    state
        .chats
        .borrow_mut()
        .extend([first.clone(), second.clone()]);
    open_chat(&state, &first);
    settle();
    state.composer.grab_focus();
    state.composer.set_text("@");
    state.composer.set_position(-1);
    settle();
    let ui = state.mentions.borrow().picker.clone().unwrap();
    assert!(ui.popover.is_visible());
    assert!(ui.spinner.is_spinning());
    assert_eq!(ui.status_label.text(), crate::i18n::t("mention_loading"));
    assert_eq!(state.mentions.borrow().candidates.len(), 1);
    screenshot(&ui.popover, &root.join("loading.png"));

    // Typing during a delayed lookup keeps the loading state, without sending
    // a second request or moving the composer caret with arrow keys.
    state.composer.set_text("@person");
    state.composer.set_position(-1);
    settle();
    assert!(state.mentions.borrow().candidates.is_empty());
    assert_eq!(ui.status_label.text(), crate::i18n::t("mention_loading"));
    for pressed in [gdk::Key::Down, gdk::Key::Up, gdk::Key::Return] {
        assert!(key(&ui, pressed));
        assert_eq!(state.composer.text(), "@person");
        assert_eq!(state.composer.position(), 7);
    }
    assert_eq!(
        state
            .pending
            .borrow()
            .values()
            .filter(|p| matches!(p, Pending::MentionMembers { .. }))
            .count(),
        1
    );
    let members = serde_json::Value::Array(
        (0..80)
            .map(|i| {
                serde_json::json!({
                    "mid": format!("u-person-{i}"), "name": format!("Person {i:02}"),
                })
            })
            .collect(),
    );
    complete_request(&state, true, members.clone());
    assert_eq!(state.mentions.borrow().candidates.len(), 80);
    assert!(!ui.status.is_visible());
    assert!(!ui.spinner.is_spinning());
    assert_selected_visible(&ui, 0);

    for index in 1..=40 {
        assert!(key(&ui, gdk::Key::Down));
        assert_selected_visible(&ui, index);
    }
    assert!(ui.scroll.vadjustment().value() > 0.0);
    let row = ui.list.selected_row().unwrap();
    let offset = ui.scroll.vadjustment().value();
    let avatar = root.join("avatar.png");
    image::RgbaImage::from_pixel(40, 40, image::Rgba([20, 180, 80, 255]))
        .save(&avatar)
        .unwrap();
    avatar_ready(&state, "u-person-40", avatar.to_str().unwrap());
    refresh(&state);
    settle();
    assert_eq!(ui.list.selected_row(), Some(row));
    assert_eq!(ui.scroll.vadjustment().value(), offset);
    assert_selected_visible(&ui, 40);
    screenshot(&ui.popover, &root.join("keyboard-scroll.png"));
    assert!(key(&ui, gdk::Key::Up));
    assert_selected_visible(&ui, 39);
    for _ in 0..39 {
        key(&ui, gdk::Key::Up);
    }
    assert_selected_visible(&ui, 0);
    key(&ui, gdk::Key::Up);
    assert_selected_visible(&ui, 79);
    key(&ui, gdk::Key::Down);
    assert_selected_visible(&ui, 0);
    key(&ui, gdk::Key::Up);
    state.composer.set_text("@person 00");
    state.composer.set_position(-1);
    settle();
    assert_eq!(state.mentions.borrow().candidates.len(), 1);
    assert_selected_visible(&ui, 0);
    state.composer.set_text("@person");
    state.composer.set_position(-1);
    settle();
    assert_selected_visible(&ui, 0);
    key(&ui, gdk::Key::Up);
    assert_selected_visible(&ui, 79);
    key(&ui, gdk::Key::Return);
    assert_eq!(state.composer.text(), "@Person 79 ");
    assert_eq!(snapshot(&state)[0].mid, "u-person-79");
    assert!(!ui.popover.is_visible());
    assert!(
        !state
            .pending
            .borrow()
            .values()
            .any(|p| matches!(p, Pending::Send { .. }))
    );

    state.composer.set_text("@nobody");
    state.composer.set_position(-1);
    settle();
    assert!(ui.status.is_visible());
    assert_eq!(ui.status_label.text(), crate::i18n::t("mention_no_members"));
    key(&ui, gdk::Key::Return);
    assert_eq!(state.composer.text(), "@nobody");
    key(&ui, gdk::Key::Escape);
    refresh(&state);
    assert!(!ui.popover.is_visible());

    reset_lookup(&state);
    state.composer.set_text("@person");
    state.composer.set_position(-1);
    settle();
    complete_request(&state, false, serde_json::Value::Null);
    assert!(ui.retry.is_visible());
    assert!(!ui.spinner.is_spinning());
    assert_eq!(
        ui.status_label.text(),
        crate::i18n::t("mention_load_failed")
    );
    screenshot(&ui.popover, &root.join("retry.png"));
    assert!(key(&ui, gdk::Key::F5));
    assert!(!ui.retry.is_visible());
    assert!(ui.spinner.is_spinning());
    complete_request(&state, false, serde_json::Value::Null);
    ui.retry.emit_clicked();
    assert!(ui.spinner.is_spinning());
    complete_request(&state, true, members.clone());
    assert!(!ui.status.is_visible());

    // A late response must not reopen the popup after focus leaves the input.
    state.search_entry.grab_focus();
    settle();
    let generation = state.mentions.borrow().generation;
    response(
        &state,
        "u-fixture",
        &first.mid,
        generation,
        true,
        &serde_json::json!({"members": members}),
    );
    assert!(!ui.popover.is_visible());
    open_chat(&state, &second);
    state.composer.grab_focus();
    state.composer.set_text("@");
    state.composer.set_position(-1);
    settle();
    response(
        &state,
        "u-fixture",
        &first.mid,
        generation,
        true,
        &serde_json::json!({"members": members}),
    );
    assert!(state.mentions.borrow().members.is_none());
    assert!(ui.spinner.is_spinning());
    complete_request(&state, true, serde_json::json!([]));
    assert_eq!(state.mentions.borrow().candidates.len(), 1);
    assert!(!ui.status.is_visible());
    state.window.destroy();
    let _ = state.sidecar.request("fixture_exit", None);
    println!("Mention picker screenshots: {}", root.display());
}
