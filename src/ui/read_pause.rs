use super::*;

pub(super) fn chat_paused(state: &AppState, mid: &str) -> bool {
    let account = state.self_mid.borrow();
    account.as_deref().is_some_and(|account| {
        state
            .config
            .borrow()
            .chat_read_receipts_paused(account, mid)
    })
}

pub(super) fn is_paused(state: &AppState, mid: &str) -> bool {
    state.config.borrow().pause_read_receipts || chat_paused(state, mid)
}

pub(super) fn can_mark_read(state: &AppState, mid: &str) -> bool {
    let account = state.self_mid.borrow();
    account
        .as_deref()
        .is_some_and(|account| state.config.borrow().can_mark_chat_read(account, mid))
}

// 復帰や再描画ではなく、本文・入力欄での操作をきっかけに既読を再開する。
pub(super) fn read_on_interaction(state: &AppState) {
    let chat_mid = state.current_chat.borrow().clone();
    let account = state.self_mid.borrow().clone();
    let generation = *state.message_render_generation.borrow();
    let s = state.clone();
    glib::idle_add_local_once(move || {
        // スクロール操作の適用後に判定し、トーク切り替え後の遅延処理は捨てる。
        if *s.current_chat.borrow() != chat_mid
            || *s.self_mid.borrow() != account
            || *s.message_render_generation.borrow() != generation
            || !*s.session_ready.borrow()
            || !s.window.is_active()
            || s.msg_stack.visible_child_name().as_deref() != Some("list")
            || !*s.stick_bottom.borrow()
        {
            return;
        }
        let Some(mid) = chat_mid else { return };
        let adjustment = s.message_scroll.vadjustment();
        let bottom = (adjustment.upper() - adjustment.page_size()).max(0.0);
        if adjustment.value() + 120.0 < bottom
            || !can_mark_read(&s, &mid)
            || !s
                .chats
                .borrow()
                .iter()
                .any(|c| c.mid == mid && c.unread > 0)
            || !s
                .last_incoming_id
                .borrow()
                .as_ref()
                .is_some_and(|id| s.seen_msg_ids.borrow().contains(id))
        {
            return;
        }
        mark_chat_read(&s, &mid);
        clear_unread_badge(&s, &mid);
        dismiss_new_marker(&s);
    });
}

pub(super) fn eligible(chat: &ChatInfo) -> bool {
    chat.mid.starts_with('u') && chat.kind != "bot"
}

pub(super) fn append_chat_menu(menu: &gio::Menu, mid: &str, paused: bool) {
    let item = gio::MenuItem::new(
        Some(&crate::i18n::t(if paused {
            "chat_read_pause_disable"
        } else {
            "chat_read_pause_enable"
        })),
        None,
    );
    item.set_action_and_target_value(Some("win.toggle-chat-read-pause"), Some(&mid.to_variant()));
    menu.append_item(&item);
}

pub(super) fn refresh_chat_row(state: &AppState, chat: &ChatInfo, row: &gtk::ListBoxRow) {
    let paused = chat_paused(state, &chat.mid);
    let label = crate::i18n::t("chat_read_pause_active");
    fn update(widget: &gtk::Widget, paused: bool, label: &str) {
        if widget.has_css_class("line-chat-read-paused") {
            widget.set_visible(paused);
            widget.set_tooltip_text(Some(label));
            widget.update_property(&[gtk::accessible::Property::Label(label)]);
            return;
        }
        let mut child = widget.first_child();
        while let Some(next) = child {
            child = next.next_sibling();
            update(&next, paused, label);
        }
    }
    if let Some(child) = row.child() {
        update(&child, paused, &label);
    }
    row.set_tooltip_text(Some(&if paused {
        format!("{}\n{label}", chat.name)
    } else {
        chat.name.clone()
    }));
}

pub(super) fn wire(state: &AppState) {
    for widget in [
        state.message_scroll.clone().upcast::<gtk::Widget>(),
        state.composer.clone().upcast::<gtk::Widget>(),
    ] {
        let click = gtk::GestureClick::new();
        click.set_button(1);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let s = state.clone();
        // 入力欄がジェスチャーを取得する前に、操作の開始を受け取る。
        click.connect_pressed(move |_, _, _, _| read_on_interaction(&s));
        widget.add_controller(click);
    }
    let key = gtk::EventControllerKey::new();
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    let s = state.clone();
    key.connect_key_released(move |_, _, _, _| read_on_interaction(&s));
    state.conversation.add_controller(key);

    // ScrolledWindow内の標準処理より前に観測し、スクロール自体は妨げない。
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    let s = state.clone();
    wheel.connect_scroll(move |_, _, _| {
        read_on_interaction(&s);
        glib::Propagation::Proceed
    });
    state.conversation.add_controller(wheel);

    let action = gio::SimpleAction::new("toggle-chat-read-pause", Some(glib::VariantTy::STRING));
    let s = state.clone();
    action.connect_activate(move |_, value| {
        if !*s.session_ready.borrow() {
            return;
        }
        let Some(mid) = value.and_then(|value| value.str()) else {
            return;
        };
        if !s
            .chats
            .borrow()
            .iter()
            .any(|chat| chat.mid == mid && eligible(chat))
        {
            return;
        }
        let Some(account) = s.self_mid.borrow().clone() else {
            return;
        };
        let paused = !chat_paused(&s, mid);
        let result =
            s.config
                .borrow_mut()
                .set_chat_read_receipts_paused(&s.data_dir, &account, mid, paused);
        if let Err(error) = result {
            tracing::error!(%error, "友だち別の既読停止を保存できませんでした");
            toast(&s, &crate::i18n::t("read_receipts_pause_save_failed"));
            return;
        }
        // 解除だけでは履歴を読み直したり、まとめて既読を送ったりしない。
        chat_filter::refresh(&s);
        toast(
            &s,
            &crate::i18n::t(if paused {
                "chat_read_pause_enabled"
            } else {
                "chat_read_pause_disabled"
            }),
        );
    });
    state.window.add_action(&action);
    state
        .read_pause_switch
        .set_active(state.config.borrow().pause_read_receipts);
    let s = state.clone();
    state
        .read_pause_switch
        .connect_state_set(move |switch, paused| {
            let previous = s.config.borrow().pause_read_receipts;
            if paused != previous {
                let result = s
                    .config
                    .borrow_mut()
                    .set_read_receipts_paused(&s.data_dir, paused);
                if let Err(error) = result {
                    tracing::error!(%error, "既読スイッチを保存できませんでした");
                    // シグナル内で戻すと、GTKが後から読み上げ用の状態を上書きする。
                    let s = s.clone();
                    gtk::glib::idle_add_local_once(move || {
                        let saved = s.config.borrow().pause_read_receipts;
                        s.read_pause_switch.set_active(saved);
                        s.read_pause_switch.set_state(saved);
                        apply_language(&s);
                        toast(&s, &crate::i18n::t("read_receipts_pause_save_failed"));
                    });
                    return gtk::glib::Propagation::Stop;
                }
            }
            // GTKの既定処理で保存前の状態へ上書きされないよう、確定した値を反映する。
            let saved = s.config.borrow().pause_read_receipts;
            switch.set_state(saved);
            switch.set_active(saved);
            // オフに戻しただけでは、開いているトークの既読を送らない。
            apply_language(&s);
            gtk::glib::Propagation::Stop
        });
    apply_language(state);
}

pub(super) fn apply_language(state: &AppState) {
    let paused = state.read_pause_switch.is_active();
    state
        .read_pause_label
        .set_text(&crate::i18n::t("read_receipts_pause"));
    let status = crate::i18n::t(if paused {
        "read_receipts_pause_on"
    } else {
        "read_receipts_pause_off"
    });
    state.read_pause_switch.set_tooltip_text(Some(&status));
    state
        .read_pause_switch
        .update_property(&[gtk::accessible::Property::Label(&crate::i18n::t(
            "read_receipts_pause",
        ))]);
    state
        .read_pause_box
        .set_tooltip_text(Some(&crate::i18n::t("read_receipts_pause_subtitle")));
    if paused {
        state.read_pause_box.add_css_class("line-read-paused");
    } else {
        state.read_pause_box.remove_css_class("line-read-paused");
    }
}
