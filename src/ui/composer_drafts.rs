use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Scope {
    account: Option<String>,
    chat: Option<String>,
    generation: u64,
}

#[derive(Clone, Default)]
struct Draft {
    text: String,
    cursor: i32,
    mentions: Vec<crate::protocol::Mention>,
    reply: Option<replies::ReplyTarget>,
    paths: Vec<PathBuf>,
}

#[derive(Default)]
pub(super) struct Drafts {
    scope: Scope,
    current: Draft,
    saved: HashMap<String, (u64, Draft)>,
    next_generation: u64,
}

impl Drafts {
    fn bind(&mut self, account: Option<String>, chat: Option<String>) -> bool {
        if self.scope.account == account && self.scope.chat == chat {
            return false;
        }
        if self.scope.account != account {
            self.saved.clear();
            self.current = Draft::default();
        } else if let Some(previous) = self.scope.chat.take() {
            self.saved.insert(
                previous,
                (self.scope.generation, std::mem::take(&mut self.current)),
            );
        }
        let saved = chat.as_ref().and_then(|chat| self.saved.remove(chat));
        self.scope.account = account;
        self.scope.chat = chat;
        if let Some((generation, draft)) = saved {
            self.scope.generation = generation;
            self.current = draft;
        } else {
            self.current = Draft::default();
            self.invalidate();
        }
        true
    }

    fn invalidate(&mut self) {
        self.next_generation = self.next_generation.wrapping_add(1);
        self.scope.generation = self.next_generation;
    }

    fn cancel_attachments(&mut self) {
        self.current.paths.clear();
        self.invalidate();
    }

    fn reset(&mut self) {
        self.saved.clear();
        self.current = Draft::default();
        self.invalidate();
    }

    fn stage(&mut self, scope: &Scope, path: PathBuf) -> bool {
        let Some(chat) = scope.chat.as_ref() else {
            return false;
        };
        if self.scope == *scope {
            self.current.paths.push(path);
            return true;
        }
        // Clipboard reads can finish after navigating away. Keep the result
        // with the originating chat, unless it was cancelled or logged out.
        if self.scope.account == scope.account
            && let Some((generation, draft)) = self.saved.get_mut(chat)
            && *generation == scope.generation
        {
            draft.paths.push(path);
            return true;
        }
        false
    }
}

fn confirm(paths: &mut Vec<PathBuf>, mut send: impl FnMut(&PathBuf) -> bool) {
    paths.retain(|path| !send(path));
}

pub(super) fn sync(state: &AppState) {
    let account = state.self_mid.borrow().clone();
    let chat = state.current_chat.borrow().clone();
    {
        let mut drafts = state.composer_drafts.borrow_mut();
        if drafts.scope.account == account && drafts.scope.chat == chat {
            return;
        }
        if drafts.scope.account == account && drafts.scope.chat.is_some() {
            drafts.current.text = state.composer.text().to_string();
            drafts.current.cursor = state.composer.position();
            drafts.current.mentions = mentions::snapshot(state);
            drafts.current.reply = state.reply_target.borrow().clone();
        }
        drafts.bind(account.clone(), chat.clone());
    }
    let draft = state.composer_drafts.borrow().current.clone();
    mentions::restore_composer(state, &draft.text, draft.cursor, &draft.mentions);
    *state.reply_target.borrow_mut() = draft.reply.filter(|target| {
        chat.as_deref() == Some(target.chat_mid.as_str())
            && !state.unsent_ids.borrow().contains(&(
                account.clone().unwrap_or_default(),
                target.quote.message_id.clone(),
            ))
    });
    replies::refresh(state);
    refresh(state);
}

pub(super) fn scope(state: &AppState) -> Scope {
    sync(state);
    state.composer_drafts.borrow().scope.clone()
}

pub(super) fn stage(state: &AppState, scope: &Scope, path: PathBuf) {
    sync(state);
    let staged = state.composer_drafts.borrow_mut().stage(scope, path);
    if staged && state.composer_drafts.borrow().scope == *scope {
        refresh(state);
    }
}

pub(super) fn cancel(state: &AppState) {
    sync(state);
    state.composer_drafts.borrow_mut().cancel_attachments();
    refresh(state);
}

pub(super) fn reset(state: &AppState) {
    state.composer_drafts.borrow_mut().reset();
    mentions::restore_composer(state, "", 0, &[]);
    replies::cancel(state);
    refresh(state);
}

/// Returns false when attachments must wait for the session to recover.
pub(super) fn send(state: &AppState) -> bool {
    sync(state);
    if state.composer_drafts.borrow().current.paths.is_empty() {
        return true;
    }
    if !*state.session_ready.borrow() {
        toast(state, &crate::i18n::t("still_restoring"));
        return false;
    }
    // Sending can rebuild the message view and sync the draft scope. Do not
    // hold a RefCell borrow across that work.
    let scope = state.composer_drafts.borrow().scope.clone();
    let mut paths = std::mem::take(&mut state.composer_drafts.borrow_mut().current.paths);
    confirm(&mut paths, |path| {
        try_send_local_media_path(state, path.clone())
    });
    if state.composer_drafts.borrow().scope == scope {
        state
            .composer_drafts
            .borrow_mut()
            .current
            .paths
            .splice(0..0, paths);
    }
    let sent = state.composer_drafts.borrow().current.paths.is_empty();
    refresh(state);
    sent
}

pub(super) fn refresh(state: &AppState) {
    let preview = &state.attachment_preview;
    while let Some(child) = preview.first_child() {
        preview.remove(&child);
    }
    let paths = state.composer_drafts.borrow().current.paths.clone();
    let scope = state.composer_drafts.borrow().scope.clone();
    preview.set_visible(!paths.is_empty());
    if paths.is_empty() {
        return;
    }
    let hint = gtk::Label::builder()
        .label(crate::i18n::t("attachment_draft_hint"))
        .xalign(0.0)
        .wrap(true)
        .css_classes(["caption", "dim-label"])
        .build();
    preview.append(&hint);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for (index, path) in paths.iter().enumerate() {
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();
        if matches!(guess_media_o_type(path), "image" | "gif")
            && let Ok(pixbuf) = Pixbuf::from_file_at_scale(path, 112, 80, true)
        {
            let picture = gtk::Picture::for_paintable(&gdk::Texture::for_pixbuf(&pixbuf));
            picture.set_size_request(112, 80);
            picture.set_alternative_text(Some(&crate::i18n::t("preview_photo")));
            card.append(&picture);
        } else {
            let icon = gtk::Image::from_icon_name("mail-attachment-symbolic");
            icon.set_size_request(112, 80);
            card.append(&icon);
        }
        let details = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let label = gtk::Label::builder()
            .label(name.as_ref())
            .tooltip_text(name.as_ref())
            .ellipsize(gtk::pango::EllipsizeMode::Middle)
            .max_width_chars(12)
            .hexpand(true)
            .css_classes(["caption"])
            .build();
        let remove_label = crate::i18n::t("attachment_remove");
        let remove = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text(&remove_label)
            .css_classes(["flat", "circular"])
            .build();
        remove.update_property(&[gtk::accessible::Property::Label(&remove_label)]);
        let s = state.clone();
        let scope = scope.clone();
        remove.connect_clicked(move |_| {
            {
                let mut drafts = s.composer_drafts.borrow_mut();
                if drafts.scope != scope || index >= drafts.current.paths.len() {
                    return;
                }
                drafts.current.paths.remove(index);
            }
            refresh(&s);
            s.composer.grab_focus();
        });
        details.append(&label);
        details.append(&remove);
        card.append(&details);
        row.append(&card);
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .child(&row)
        .build();
    preview.append(&scroll);
}

#[cfg(test)]
#[path = "composer_drafts_test.rs"]
mod tests;
