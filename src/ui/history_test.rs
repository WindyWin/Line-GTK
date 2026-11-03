use super::*;

fn settle() {
    let context = glib::MainContext::default();
    let until = Instant::now() + Duration::from_millis(250);
    while Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn find(widget: &gtk::Widget, matches: &impl Fn(&gtk::Widget) -> bool) -> Option<gtk::Widget> {
    if matches(widget) {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find(&current, matches) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn status_row(state: &AppState) -> gtk::Widget {
    find(state.message_list.widget().upcast_ref(), &|widget| {
        widget.has_css_class("line-history-sentinel")
    })
    .expect("the history status must be visible at the top")
}

fn assert_status(state: &AppState, key: &str, loading: bool, failed: bool) {
    let row = status_row(state);
    let label = find(&row, &|widget| widget.is::<gtk::Label>())
        .unwrap()
        .downcast::<gtk::Label>()
        .unwrap();
    assert_eq!(label.text(), crate::i18n::t(key));
    let spinner = find(&row, &|widget| widget.is::<gtk::Spinner>())
        .unwrap()
        .downcast::<gtk::Spinner>()
        .unwrap();
    assert_eq!(spinner.is_spinning(), loading);
    assert_eq!(spinner.is_visible(), loading);
    let retry = find(&row, &|widget| widget.is::<gtk::Button>()).unwrap();
    assert_eq!(retry.is_visible(), failed);
    assert!(
        row.height() >= 52,
        "{key}: row height {}, mapped {}",
        row.height(),
        row.is_mapped()
    );
}

fn complete(state: &AppState, ok: bool, result: serde_json::Value) {
    let id = state.history.borrow().older_request.unwrap();
    let pending = state.pending.borrow_mut().remove(&id).unwrap();
    response(state, id, pending, ok, &result);
    settle();
}

fn message(id: u32) -> MessageInfo {
    serde_json::from_value(message_json(id)).unwrap()
}

fn message_json(id: u32) -> serde_json::Value {
    serde_json::json!({
        "id": id.to_string(), "from": "u-friend", "to": "c-fixture",
        "text": format!("Tin nhắn {id}"), "createdTime": 1700000000000_i64 + i64::from(id) * 1000,
        "mine": false, "contentType": "TEXT",
    })
}

fn screenshot(state: &AppState, path: &std::path::Path) {
    let paintable = gtk::WidgetPaintable::new(Some(&state.window));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(
        &snapshot,
        state.window.width() as f64,
        state.window.height() as f64,
    );
    let renderer = gtk::gsk::CairoRenderer::new();
    renderer.realize(None::<&gdk::Surface>).unwrap();
    let texture = renderer.render_texture(snapshot.to_node().unwrap(), None);
    texture.save_to_png(path).unwrap();
    renderer.unrealize();
}

#[test]
#[ignore = "requires a desktop display and isolated XDG directories; uses a fixture sidecar"]
fn fixture_history_loading_retry_end_and_chat_switch() {
    use std::os::unix::fs::PermissionsExt;
    libadwaita::init().unwrap();
    let root = std::env::temp_dir().join(format!("linegtk-history-ui-{}", std::process::id()));
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
        language: "vi".into(),
        animations: true,
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
        .application_id("dev.linegtk.HistoryFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let state = build_ui(&app, root.clone(), data, false).unwrap();
    *state.self_mid.borrow_mut() = Some("u-fixture".into());
    *state.current_chat.borrow_mut() = Some("c-fixture".into());
    state.stack.set_visible_child_name("shell");
    state.chat_title.set_text("Lịch sử tin nhắn");
    set_msg_state(&state, "list", None);
    for id in 41..=60 {
        append_message(&state, &message(id), false);
    }
    reset_for_latest(&state, true);
    arm(&state);
    state.window.present();
    state.message_scroll.vadjustment().set_value(0.0);
    settle();
    state.message_scroll.vadjustment().set_value(0.0);
    settle();
    screenshot(&state, &root.join("waiting-light.png"));
    assert_status(&state, "history_waiting", false, false);

    *state.session_ready.borrow_mut() = true;
    on_edge(&state, Edge::Top);
    settle();
    assert_status(&state, "history_older_loading", true, false);
    let request_id = state.history.borrow().older_request;
    on_edge(&state, Edge::Top);
    assert_eq!(state.history.borrow().older_request, request_id);
    for (name, scheme) in [
        ("loading-light.png", libadwaita::ColorScheme::ForceLight),
        ("loading-dark.png", libadwaita::ColorScheme::ForceDark),
    ] {
        libadwaita::StyleManager::default().set_color_scheme(scheme);
        settle();
        screenshot(&state, &root.join(name));
    }

    for failure in 1..=MAX_FAILURES {
        complete(&state, false, serde_json::json!({}));
        assert_status(&state, "history_failed", false, true);
        assert_eq!(state.history.borrow().older_failures, failure);
        state.history.borrow_mut().older_ready_at = None;
        on_edge(&state, Edge::Top);
        if failure < MAX_FAILURES {
            assert!(state.history.borrow().older_request.is_some());
        } else {
            assert!(state.history.borrow().older_request.is_none());
        }
    }
    screenshot(&state, &root.join("failed-dark.png"));
    let retry = find(&status_row(&state), &|widget| widget.is::<gtk::Button>())
        .unwrap()
        .downcast::<gtk::Button>()
        .unwrap();
    retry.emit_clicked();
    settle();
    assert_status(&state, "history_older_loading", true, false);
    assert_eq!(state.history.borrow().older_failures, 0);

    // Exhausting a nonempty final page must keep both the messages and status.
    let anchor_row = state.message_rows.borrow().get("41").unwrap().clone();
    let before = anchor_row
        .compute_bounds(&state.message_scroll)
        .unwrap()
        .y();
    complete(
        &state,
        true,
        serde_json::json!({
            "messages": (31..41).map(message_json).collect::<Vec<_>>(), "hasMore": false,
        }),
    );
    let after = anchor_row
        .compute_bounds(&state.message_scroll)
        .unwrap()
        .y();
    assert!(
        (before - after).abs() < 2.0,
        "prepending history must preserve the reading position"
    );
    state.message_scroll.vadjustment().set_value(0.0);
    settle();
    assert_status(&state, "history_start", false, false);
    assert_eq!(state.message_list.message_count(), 30);
    assert!(state.seen_msg_ids.borrow().contains("31"));
    on_edge(&state, Edge::Top);
    assert!(state.history.borrow().older_request.is_none());
    gtk::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceLight);
    settle();
    screenshot(&state, &root.join("exhausted-light.png"));

    // An old response must not change the loading row in the next chat.
    state.history.borrow_mut().has_more_older = true;
    state.history.borrow_mut().older_ready_at = None;
    on_edge(&state, Edge::Top);
    let stale_id = state.history.borrow().older_request.unwrap();
    let stale_pending = state.pending.borrow_mut().remove(&stale_id).unwrap();
    clear_messages(&state);
    *state.history_request_generation.borrow_mut() += 1;
    *state.current_chat.borrow_mut() = Some("c-next".into());
    append_message(&state, &message(90), false);
    reset_for_latest(&state, true);
    arm(&state);
    on_edge(&state, Edge::Top);
    response(
        &state,
        stale_id,
        stale_pending,
        true,
        &serde_json::json!({"messages": [], "hasMore": false}),
    );
    settle();
    assert_status(&state, "history_older_loading", true, false);
    assert!(state.history.borrow().has_more_older);
    complete(
        &state,
        true,
        serde_json::json!({"messages": [], "hasMore": false}),
    );
    assert_status(&state, "history_start", false, false);
    state.window.destroy();
    let _ = state.sidecar.request("fixture_exit", None);
    println!("History screenshots: {}", root.display());
}
