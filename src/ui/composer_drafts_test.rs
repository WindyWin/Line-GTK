use super::*;

fn draft() -> Drafts {
    let mut draft = Drafts::default();
    draft.bind(Some("account-a".into()), Some("chat-a".into()));
    draft
}

#[test]
fn paste_waits_for_confirmation_and_retains_failed_sends() {
    let mut draft = draft();
    let scope = draft.scope.clone();
    assert!(draft.stage(&scope, "one.png".into()));
    assert!(draft.stage(&scope, "two.png".into()));
    assert_eq!(draft.current.paths.len(), 2);
    let mut sent = Vec::new();
    confirm(&mut draft.current.paths, |path| {
        sent.push(path.clone());
        path == &PathBuf::from("one.png")
    });
    assert_eq!(sent, vec![PathBuf::from("one.png"), "two.png".into()]);
    assert_eq!(draft.current.paths, vec![PathBuf::from("two.png")]);
    draft.bind(scope.account.clone(), Some("chat-b".into()));
    assert!(draft.current.paths.is_empty());
    draft.bind(scope.account.clone(), scope.chat.clone());
    assert_eq!(draft.current.paths, vec![PathBuf::from("two.png")]);
    confirm(&mut draft.current.paths, |_| true);
    draft.bind(scope.account.clone(), Some("chat-b".into()));
    draft.bind(scope.account, scope.chat);
    assert!(draft.current.paths.is_empty());
}

#[test]
fn chat_drafts_preserve_text_and_route_late_pastes_to_their_origin() {
    let mut draft = draft();
    let first = draft.scope.clone();
    draft.current.text = " First 👋\nline ".into();
    draft.current.cursor = 3;
    draft.stage(&first, "one.png".into());
    draft.bind(first.account.clone(), Some("chat-b".into()));
    let second = draft.scope.clone();
    assert!(draft.current.text.is_empty());
    assert!(draft.current.paths.is_empty());
    draft.current.text = "Second".into();
    assert!(draft.stage(&first, "late.png".into()));
    assert!(draft.current.paths.is_empty());
    draft.bind(first.account.clone(), first.chat.clone());
    assert_eq!(draft.current.text, " First 👋\nline ");
    assert_eq!(draft.current.cursor, 3);
    assert_eq!(
        draft.current.paths,
        vec![PathBuf::from("one.png"), "late.png".into()]
    );
    draft.bind(second.account, second.chat);
    assert_eq!(draft.current.text, "Second");
    assert!(draft.current.paths.is_empty());
}

#[test]
fn deselecting_a_chat_keeps_its_draft() {
    let mut draft = draft();
    let scope = draft.scope.clone();
    draft.current.text = "Keep me".into();
    draft.stage(&scope, "one.png".into());
    draft.bind(scope.account.clone(), None);
    assert!(draft.current.text.is_empty());
    assert!(draft.current.paths.is_empty());
    let unselected = draft.scope.clone();
    assert!(!draft.stage(&unselected, "nowhere.png".into()));
    assert!(draft.stage(&scope, "late.png".into()));
    draft.bind(scope.account, scope.chat);
    assert_eq!(draft.current.text, "Keep me");
    assert_eq!(draft.current.paths.len(), 2);
}

#[test]
fn cancel_only_discards_current_attachments_and_their_in_flight_pastes() {
    let mut draft = draft();
    let first = draft.scope.clone();
    draft.stage(&first, "one.png".into());
    draft.bind(first.account.clone(), Some("chat-b".into()));
    let second = draft.scope.clone();
    draft.current.text = "Keep text".into();
    draft.stage(&second, "two.png".into());
    draft.cancel_attachments();
    assert_eq!(draft.current.text, "Keep text");
    assert!(draft.current.paths.is_empty());
    assert!(!draft.stage(&second, "cancelled.png".into()));
    assert!(draft.stage(&first, "late.png".into()));
    draft.bind(first.account.clone(), first.chat.clone());
    assert_eq!(draft.current.paths.len(), 2);
    draft.bind(second.account.clone(), second.chat.clone());
    assert!(!draft.stage(&second, "still-cancelled.png".into()));
    assert!(draft.current.paths.is_empty());
}

#[test]
fn account_changes_and_logout_discard_all_drafts_and_stale_reads() {
    for logout in [false, true] {
        let mut draft = draft();
        let first = draft.scope.clone();
        draft.current.text = "Private A".into();
        draft.stage(&first, "one.png".into());
        draft.bind(first.account.clone(), Some("chat-b".into()));
        let second = draft.scope.clone();
        draft.current.text = "Private B".into();
        draft.stage(&second, "two.png".into());
        if logout {
            draft.reset();
        } else {
            draft.bind(Some("account-b".into()), first.chat.clone());
        }
        assert!(draft.current.text.is_empty());
        assert!(draft.current.paths.is_empty());
        assert!(draft.saved.is_empty());
        assert!(!draft.stage(&first, "late.png".into()));
        assert!(!draft.stage(&second, "late-b.png".into()));
        draft.bind(first.account.clone(), first.chat.clone());
        assert!(!draft.stage(&first, "late-after-return.png".into()));
        assert!(draft.current.text.is_empty());
        assert!(draft.current.paths.is_empty());
        draft.bind(second.account.clone(), second.chat.clone());
        assert!(!draft.stage(&second, "late-after-return-b.png".into()));
        assert!(draft.current.text.is_empty());
        assert!(draft.current.paths.is_empty());
    }
}

#[test]
#[ignore = "requires a desktop display; uses a fixture sidecar and no LINE account"]
fn fixture_conversation_drafts_restore_text_photos_mentions_and_replies() {
    use std::os::unix::fs::PermissionsExt;
    libadwaita::init().unwrap();
    let root = std::env::temp_dir().join(format!("linegtk-drafts-ui-{}", std::process::id()));
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
        .application_id("dev.linegtk.DraftsFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let state = build_ui(&app, root.clone(), data, false).unwrap();
    *state.self_mid.borrow_mut() = Some("u-fixture".into());
    *state.session_ready.borrow_mut() = true;
    state.stack.set_visible_child_name("shell");
    let first: ChatInfo = serde_json::from_value(serde_json::json!({
        "mid": "c-first", "name": "First conversation", "kind": "group", "unread": 0,
    }))
    .unwrap();
    let second = ChatInfo {
        mid: "c-second".into(),
        name: "Second conversation".into(),
        ..first.clone()
    };
    state
        .chats
        .borrow_mut()
        .extend([first.clone(), second.clone()]);
    open_chat(&state, &first);
    let text = "  Hi 👋\n@Friend  ";
    let spans = vec![crate::protocol::Mention {
        start: 7,
        end: 14,
        mid: "u-friend".into(),
        all: false,
    }];
    mentions::restore_composer(&state, text, 6, &spans);
    *state.reply_target.borrow_mut() = Some(replies::ReplyTarget {
        chat_mid: first.mid.clone(),
        quote: serde_json::from_value(serde_json::json!({
            "messageId": "123", "text": "Original message", "from": "u-friend",
        }))
        .unwrap(),
    });
    replies::refresh(&state);
    let photo = root.join("draft.png");
    image::RgbaImage::from_pixel(112, 80, image::Rgba([20, 180, 80, 255]))
        .save(&photo)
        .unwrap();
    let first_scope = scope(&state);
    stage(&state, &first_scope, photo.clone());
    assert!(state.attachment_preview.is_visible());

    open_chat(&state, &second);
    assert!(state.composer.text().is_empty());
    assert!(state.reply_target.borrow().is_none());
    assert!(!state.reply_preview.reveals_child());
    assert!(!state.attachment_preview.is_visible());
    assert!(mentions::snapshot(&state).is_empty());
    state.composer.set_text("Second draft");
    state.composer.set_position(4);
    stage(&state, &first_scope, photo.clone());
    assert!(!state.attachment_preview.is_visible());

    open_chat(&state, &first);
    assert_eq!(state.composer.text(), text);
    assert_eq!(state.composer.position(), 6);
    assert_eq!(mentions::snapshot(&state), spans);
    assert!(state.reply_preview.reveals_child());
    assert_eq!(
        state
            .reply_target
            .borrow()
            .as_ref()
            .unwrap()
            .quote
            .message_id,
        "123"
    );
    assert!(state.attachment_preview.is_visible());
    assert_eq!(state.composer_drafts.borrow().current.paths.len(), 2);
    // Rebuilding the thread and reconnecting must preserve the active draft.
    clear_messages(&state);
    on_logged_in(
        &state,
        &serde_json::json!({"mid": "u-fixture", "displayName": "Fixture"}),
    );
    assert_eq!(mentions::snapshot(&state), spans);
    assert_eq!(state.composer.text(), text);
    save_screenshot(&state, &root.join("first-conversation.png"));

    // Sending clears only this conversation's draft.
    state.send_btn.emit_clicked();
    assert!(state.composer.text().is_empty());
    assert!(state.reply_target.borrow().is_none());
    assert!(!state.attachment_preview.is_visible());
    let pending = state.pending.borrow();
    let sends: Vec<_> = pending
        .values()
        .filter_map(|pending| match pending {
            Pending::Send { chat_mid, .. } => Some(chat_mid.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(sends, vec![first.mid.as_str(); 3]);
    drop(pending);
    open_chat(&state, &second);
    assert_eq!(state.composer.text(), "Second draft");
    assert_eq!(state.composer.position(), 4);
    save_screenshot(&state, &root.join("second-conversation.png"));
    open_chat(&state, &first);
    assert!(state.composer.text().is_empty());
    assert!(!state.attachment_preview.is_visible());

    // An unsent quote must not be resurrected from a background draft.
    *state.reply_target.borrow_mut() = Some(replies::ReplyTarget {
        chat_mid: first.mid.clone(),
        quote: serde_json::from_value(serde_json::json!({"messageId": "123"})).unwrap(),
    });
    open_chat(&state, &second);
    state
        .unsent_ids
        .borrow_mut()
        .insert(("u-fixture".into(), "123".into()));
    open_chat(&state, &first);
    assert!(state.reply_target.borrow().is_none());
    open_chat(&state, &second);
    reset(&state);
    open_chat(&state, &first);
    assert!(state.composer.text().is_empty());
    assert!(!state.attachment_preview.is_visible());
    open_chat(&state, &second);
    assert!(state.composer.text().is_empty());
    assert!(
        !state
            .composer_drafts
            .borrow_mut()
            .stage(&first_scope, photo)
    );
    state.window.destroy();
    let _ = state.sidecar.request("fixture_exit", None);
    println!("Draft fixture screenshots: {}", root.display());
}

fn save_screenshot(state: &AppState, path: &std::path::Path) {
    let context = glib::MainContext::default();
    let until = std::time::Instant::now() + std::time::Duration::from_millis(300);
    while std::time::Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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
