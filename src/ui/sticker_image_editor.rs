use super::*;
use crate::sticker_image::{ImageOptions, ImageSize};

fn send_prepared(state: &AppState, image: &crate::sticker_image::PreparedImage) -> bool {
    let result = (|| -> anyhow::Result<PathBuf> {
        let directory = state.data_dir.join("cache/media");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("sticker-image-{}.png", now_ms()));
        crate::sticker_image::save(image, &path)?;
        Ok(path)
    })();
    match result {
        Ok(path) => try_send_local_media_path(state, path),
        Err(error) => {
            toast(
                state,
                &crate::i18n::tf("media_send_failed", &[("error", &error.to_string())]),
            );
            false
        }
    }
}

pub(super) fn send_last(state: &AppState, path: &std::path::Path) {
    let options = crate::sticker_image::options_for(&state.data_dir, path);
    match crate::sticker_image::prepare(path, options.size.pixels(), options.remove_background) {
        Ok(image) => {
            send_prepared(state, &image);
        }
        Err(error) => toast(
            state,
            &crate::i18n::tf("media_send_failed", &[("error", &error.to_string())]),
        ),
    }
}

pub(super) fn wire_menu(state: &AppState, button: &gtk::Button, path: PathBuf) {
    let actions = gio::SimpleActionGroup::new();
    let edit = gio::SimpleAction::new("edit", None);
    let s = state.clone();
    let scope = composer_drafts::scope(state);
    edit.connect_activate(move |_, _| {
        if composer_drafts::scope(&s) == scope {
            s.sticker_popover.popdown();
            open(&s, path.clone());
        }
    });
    actions.add_action(&edit);
    button.insert_action_group("sticker-image", Some(&actions));
    let menu = gio::Menu::new();
    menu.append(
        Some(&crate::i18n::t("sticker_image_edit")),
        Some("sticker-image.edit"),
    );
    let popup = Rc::new(move |button: &gtk::Button, x: f64, y: f64| {
        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(button);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.connect_closed(|popover| {
            let popover = popover.clone();
            glib::idle_add_local_once(move || popover.unparent());
        });
        popover.popup();
        popover.grab_focus();
    });
    button.connect_unrealize(|button| {
        let mut child = button.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.is::<gtk::Popover>() {
                widget.unparent();
            }
        }
    });
    let click = gtk::GestureClick::new();
    click.set_button(3);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak = button.downgrade();
    let popup_c = popup.clone();
    click.connect_pressed(move |gesture, _, x, y| {
        if let Some(button) = weak.upgrade() {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            popup_c(&button, x, y);
        }
    });
    button.add_controller(click);
    let keys = gtk::EventControllerKey::new();
    let weak = button.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if key == gdk::Key::Menu
            || (key == gdk::Key::F10 && modifiers.contains(gdk::ModifierType::SHIFT_MASK))
        {
            if let Some(button) = weak.upgrade() {
                popup(&button, 10.0, 10.0);
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    button.add_controller(keys);
}

pub(super) fn open(state: &AppState, path: PathBuf) {
    let scope = composer_drafts::scope(state);
    let options = crate::sticker_image::options_for(&state.data_dir, &path);
    let dialog = libadwaita::Dialog::builder()
        .title(crate::i18n::t("sticker_send_as_image"))
        .content_width(400)
        .content_height(560)
        .build();
    let toolbar = libadwaita::ToolbarView::new();
    toolbar.add_top_bar(&libadwaita::HeaderBar::new());
    let body = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_start(18)
        .margin_end(18)
        .margin_top(12)
        .margin_bottom(18)
        .build();
    let preview = gtk::Picture::builder()
        .content_fit(gtk::ContentFit::Contain)
        .can_shrink(true)
        .vexpand(true)
        .build();
    preview.set_size_request(240, 240);
    preview.update_property(&[gtk::accessible::Property::Label(&crate::i18n::t(
        "sticker_image_preview",
    ))]);
    // A checkerboard makes remaining transparency visible before sending.
    let checkerboard = gtk::DrawingArea::builder()
        .vexpand(true)
        .hexpand(true)
        .build();
    checkerboard.set_draw_func(|_, cr, width, height| {
        for y in (0..height).step_by(12) {
            for x in (0..width).step_by(12) {
                let shade = if (x / 12 + y / 12) % 2 == 0 {
                    1.0
                } else {
                    0.85
                };
                cr.set_source_rgb(shade, shade, shade);
                cr.rectangle(f64::from(x), f64::from(y), 12.0, 12.0);
                let _ = cr.fill();
            }
        }
    });
    let overlay = gtk::Overlay::builder().child(&checkerboard).build();
    overlay.add_overlay(&preview);
    overlay.set_measure_overlay(&preview, true);
    let frame = gtk::Frame::builder().child(&overlay).build();
    body.append(&frame);
    let size_label = gtk::Label::builder()
        .label(crate::i18n::t("sticker_image_size"))
        .xalign(0.0)
        .build();
    let sizes: Vec<String> = [
        "sticker_image_small",
        "sticker_image_medium",
        "sticker_image_large",
        "sticker_image_original",
    ]
    .iter()
    .map(|key| crate::i18n::t(key))
    .collect();
    let size = gtk::DropDown::from_strings(&sizes.iter().map(String::as_str).collect::<Vec<_>>());
    size.set_selected(options.size.selection());
    size.update_property(&[gtk::accessible::Property::Label(&crate::i18n::t(
        "sticker_image_size",
    ))]);
    size_label.set_mnemonic_widget(Some(&size));
    body.append(&size_label);
    body.append(&size);
    let remove = gtk::CheckButton::with_label(&crate::i18n::t("sticker_image_remove_bg"));
    remove.set_active(options.remove_background);
    body.append(&remove);
    let hint = gtk::Label::builder()
        .label(crate::i18n::t("sticker_image_edit_hint"))
        .xalign(0.0)
        .wrap(true)
        .css_classes(["caption", "dim-label"])
        .build();
    body.append(&hint);
    let status = gtk::Label::builder().xalign(0.0).wrap(true).build();
    body.append(&status);
    let send = gtk::Button::builder()
        .label(crate::i18n::t("send"))
        .css_classes(["suggested-action"])
        .halign(gtk::Align::End)
        .build();
    let actions = gtk::Box::builder()
        .margin_start(18)
        .margin_end(18)
        .margin_top(8)
        .margin_bottom(18)
        .halign(gtk::Align::End)
        .build();
    actions.append(&send);
    toolbar.add_bottom_bar(&actions);
    let prepared = Rc::new(RefCell::new(None));
    let update: Rc<dyn Fn()> = {
        let path = path.clone();
        let size = size.downgrade();
        let remove = remove.downgrade();
        let preview = preview.downgrade();
        let prepared = prepared.clone();
        let status = status.downgrade();
        let send = send.downgrade();
        Rc::new(move || {
            let (Some(size), Some(remove), Some(preview), Some(status), Some(send)) = (
                size.upgrade(),
                remove.upgrade(),
                preview.upgrade(),
                status.upgrade(),
                send.upgrade(),
            ) else {
                return;
            };
            let pixels = ImageSize::from_selection(size.selected()).pixels();
            match crate::sticker_image::prepare(&path, pixels, remove.is_active()) {
                Ok(image) => {
                    preview.set_paintable(Some(&gdk::Texture::for_pixbuf(&image.pixbuf)));
                    status.set_text(&if remove.is_active() && !image.background_removed {
                        crate::i18n::t("sticker_image_bg_unchanged")
                    } else {
                        String::new()
                    });
                    *prepared.borrow_mut() = Some(image);
                    send.set_sensitive(true);
                }
                Err(error) => {
                    *prepared.borrow_mut() = None;
                    preview.set_paintable(None::<&gdk::Texture>);
                    status.set_text(&crate::i18n::tf(
                        "media_send_failed",
                        &[("error", &error.to_string())],
                    ));
                    send.set_sensitive(false);
                }
            }
        })
    };
    let update_c = update.clone();
    size.connect_selected_notify(move |_| update_c());
    let update_c = update.clone();
    remove.connect_toggled(move |_| update_c());
    update();

    let s = state.clone();
    let weak_dialog = dialog.downgrade();
    send.connect_clicked(move |_| {
        // An editor opened in one conversation must never send to another.
        if composer_drafts::scope(&s) != scope {
            if let Some(dialog) = weak_dialog.upgrade() {
                dialog.close();
            }
            return;
        }
        let Some(image) = prepared.borrow().clone() else {
            return;
        };
        if send_prepared(&s, &image) {
            let options = ImageOptions {
                size: ImageSize::from_selection(size.selected()),
                remove_background: remove.is_active(),
            };
            if let Err(error) = crate::sticker_image::remember_options(&s.data_dir, &path, options)
            {
                tracing::warn!(%error, "could not save sticker image options");
            }
            if let Some(dialog) = weak_dialog.upgrade() {
                dialog.close();
            }
        }
    });
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&body)
        .build();
    toolbar.set_content(Some(&scroll));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(&state.window));
}
