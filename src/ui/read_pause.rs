use super::{AppState, toast};
use gtk::prelude::*;

pub(super) fn wire(state: &AppState) {
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
