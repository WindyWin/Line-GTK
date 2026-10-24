use super::*;
use crate::protocol::Mention;

#[derive(Clone, serde::Deserialize)]
struct Member {
    mid: String,
    name: String,
    #[serde(rename = "avatarPath", default)]
    avatar_path: Option<String>,
}

pub(super) fn avatar_ready(state: &AppState, mid: &str, path: &str) {
    let updated = {
        let mut data = state.mentions.borrow_mut();
        let mut updated = false;
        for member in data.members.iter_mut().flatten() {
            if member.mid == mid {
                member.avatar_path = Some(path.to_string());
                updated = true;
            }
        }
        updated
    };
    if updated
        && state
            .mentions
            .borrow()
            .popover
            .as_ref()
            .is_some_and(|p| p.is_visible())
    {
        refresh(state);
    }
}

#[derive(Default)]
pub(super) struct MentionState {
    text: String,
    spans: Vec<Mention>,
    members: Option<Vec<Member>>,
    candidates: Vec<Member>,
    loading: bool,
    failed: bool,
    generation: u64,
    replacing: bool,
    dismissed: bool,
    popover: Option<gtk::Popover>,
    list: Option<gtk::ListBox>,
}

#[derive(Debug, Clone)]
pub(super) struct MentionDraft {
    pub account: Option<String>,
    pub chat_mid: String,
    pub text: String,
    pub mentions: Vec<Mention>,
}

// Retain identity only when the edit leaves the entire selected label intact.
fn edited_spans(old: &str, new: &str, spans: &[Mention]) -> Vec<Mention> {
    let old: Vec<char> = old.chars().collect();
    let new: Vec<char> = new.chars().collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_end = old.len() - suffix;
    let new_end = new.len() - suffix;
    spans
        .iter()
        .filter_map(|span| {
            let mut span = span.clone();
            if span.end <= prefix {
                return Some(span);
            }
            if span.start >= old_end {
                span.start = span.start - old_end + new_end;
                span.end = span.end - old_end + new_end;
                return Some(span);
            }
            None
        })
        .collect()
}

fn query(text: &str, cursor: usize, spans: &[Mention]) -> Option<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let before = chars.get(..cursor)?;
    let start = before.iter().rposition(|ch| *ch == '@')?;
    if start > 0 && !before[start - 1].is_whitespace() {
        return None;
    }
    if spans
        .iter()
        .any(|span| start >= span.start && start < span.end)
    {
        return None;
    }
    let term: String = before[start + 1..].iter().collect();
    if term.contains(['\n', '\r', '@']) || term.chars().count() > 80 {
        return None;
    }
    Some((start, term.to_lowercase()))
}

pub(super) fn reset(state: &AppState) {
    let mut data = state.mentions.borrow_mut();
    data.generation = data.generation.wrapping_add(1);
    data.text = state.composer.text().to_string();
    data.spans.clear();
    state.composer.set_attributes(&gtk::pango::AttrList::new());
    data.members = None;
    data.candidates.clear();
    data.loading = false;
    data.failed = false;
    data.dismissed = true;
    if let Some(popover) = &data.popover {
        popover.popdown();
    }
}

pub(super) fn restore(state: &AppState, spans: &[Mention]) {
    let mut data = state.mentions.borrow_mut();
    data.text = state.composer.text().to_string();
    data.spans = spans.to_vec();
    state
        .composer
        .set_attributes(&attributes(&state.composer, &data.text, &data.spans));
}

pub(super) fn restore_failed(state: &AppState, draft: &MentionDraft) {
    if draft.account != *state.self_mid.borrow()
        || state.current_chat.borrow().as_deref() != Some(&draft.chat_mid)
        || !state.composer.text().is_empty()
    {
        return;
    }
    state.composer.set_text(&draft.text);
    restore(state, &draft.mentions);
}

pub(super) fn outgoing(state: &AppState) -> (String, Vec<Mention>) {
    let raw = state.composer.text();
    let text = raw.trim().to_string();
    let leading = raw.chars().count() - raw.trim_start().chars().count();
    let end = leading + text.chars().count();
    let spans = state
        .mentions
        .borrow()
        .spans
        .iter()
        .filter_map(|span| {
            if span.start < leading || span.end > end {
                return None;
            }
            Some(Mention {
                start: span.start - leading,
                end: span.end - leading,
                ..span.clone()
            })
        })
        .collect();
    (text, spans)
}

fn request_members(state: &AppState) {
    let Some(chat_mid) = state.current_chat.borrow().clone() else {
        return;
    };
    let Some(account) = state.self_mid.borrow().clone() else {
        return;
    };
    let generation = {
        let mut data = state.mentions.borrow_mut();
        if data.loading || data.members.is_some() || data.failed {
            return;
        }
        data.loading = true;
        data.generation
    };
    match state.sidecar.request(
        "get_chat_members",
        Some(serde_json::json!({"chatMid": chat_mid})),
    ) {
        Ok(id) => {
            state.pending.borrow_mut().insert(
                id,
                Pending::MentionMembers {
                    account,
                    chat_mid,
                    generation,
                },
            );
        }
        Err(_) => {
            let mut data = state.mentions.borrow_mut();
            data.loading = false;
            data.failed = true;
        }
    }
}

pub(super) fn response(
    state: &AppState,
    account: &str,
    chat_mid: &str,
    generation: u64,
    ok: bool,
    result: &serde_json::Value,
) {
    if state.self_mid.borrow().as_deref() != Some(account)
        || state.current_chat.borrow().as_deref() != Some(chat_mid)
        || state.mentions.borrow().generation != generation
    {
        return;
    }
    {
        let mut data = state.mentions.borrow_mut();
        data.loading = false;
        data.failed = !ok;
        if ok {
            data.members = serde_json::from_value(result["members"].clone()).ok();
            data.failed = data.members.is_none();
        }
    }
    refresh(state);
}

fn refresh(state: &AppState) {
    let cursor = state.composer.position().max(0) as usize;
    let text = state.composer.text();
    let active = {
        let data = state.mentions.borrow();
        if data.replacing {
            return;
        }
        !data.dismissed
            && state
                .current_chat
                .borrow()
                .as_deref()
                .is_some_and(|mid| mid.starts_with('c'))
            && query(&text, cursor, &data.spans).is_some()
    };
    if !active {
        if let Some(popover) = &state.mentions.borrow().popover {
            popover.popdown();
        }
        return;
    }
    request_members(state);
    let (popover, list, candidates, status) = {
        let mut data = state.mentions.borrow_mut();
        let Some((_, term)) = query(&text, cursor, &data.spans) else {
            return;
        };
        let Some(popover) = data.popover.clone() else {
            return;
        };
        let Some(list) = data.list.clone() else {
            return;
        };
        data.candidates = data
            .members
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|m| {
                state.self_mid.borrow().as_deref() != Some(&m.mid)
                    && m.name.to_lowercase().contains(&term)
            })
            .take(30)
            .cloned()
            .collect();
        let status = if data.loading {
            "mention_loading"
        } else if data.failed {
            "mention_load_failed"
        } else {
            "mention_no_members"
        };
        (popover, list, data.candidates.clone(), status)
    };
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if candidates.is_empty() {
        let label = gtk::Label::builder()
            .label(crate::i18n::t(status))
            .wrap(true)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        let row = gtk::ListBoxRow::builder()
            .selectable(false)
            .activatable(status == "mention_load_failed")
            .child(&label)
            .build();
        list.append(&row);
    } else {
        for member in candidates {
            let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row_box.set_margin_top(6);
            row_box.set_margin_bottom(6);
            row_box.set_margin_start(12);
            row_box.set_margin_end(12);
            let avatar_frame = gtk::Box::builder()
                .halign(gtk::Align::Center)
                .valign(gtk::Align::Center)
                .css_classes(["line-avatar-frame", "line-avatar-sm"])
                .build();
            let avatar = gtk::Picture::builder()
                .content_fit(gtk::ContentFit::Cover)
                .width_request(40)
                .height_request(40)
                .css_classes(["line-avatar", "line-avatar-sm"])
                .build();
            if let Some(path) = member.avatar_path.as_deref()
                && std::path::Path::new(path).exists()
            {
                attach_texture_async(avatar.clone(), path.to_string(), 80);
            }
            avatar_frame.append(&avatar);
            row_box.append(&avatar_frame);
            let row_content = gtk::Box::new(gtk::Orientation::Vertical, 2);
            row_content.set_hexpand(true);
            row_content.set_valign(gtk::Align::Center);
            row_box.append(&row_content);
            let label = gtk::Label::builder()
                .label(&member.name)
                .xalign(0.0)
                .hexpand(true)
                .max_width_chars(32)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            // Distinguish members with identical display names.
            label.set_tooltip_text(Some(&member.mid));
            row_content.append(&label);
            let same_name = state
                .mentions
                .borrow()
                .members
                .as_deref()
                .unwrap_or_default()
                .iter()
                .filter(|m| m.name == member.name)
                .count()
                > 1;
            if same_name {
                row_content.append(
                    &gtk::Label::builder()
                        .label(&member.mid)
                        .xalign(0.0)
                        .css_classes(["dim-label", "caption"])
                        .build(),
                );
            }
            let row = gtk::ListBoxRow::builder()
                .focusable(false)
                .child(&row_box)
                .build();
            list.append(&row);
        }
        list.select_row(list.row_at_index(0).as_ref());
    }
    if state.composer.is_mapped() {
        popover.popup();
    }
}

fn choose(state: &AppState, index: usize) {
    let text = state.composer.text().to_string();
    let cursor = state.composer.position().max(0) as usize;
    let (member, start, mut spans) = {
        let data = state.mentions.borrow();
        let Some(member) = data.candidates.get(index).cloned() else {
            return;
        };
        let Some((start, _)) = query(&text, cursor, &data.spans) else {
            return;
        };
        (member, start, data.spans.clone())
    };
    let chars: Vec<char> = text.chars().collect();
    let label = format!("@{}", member.name);
    let inserted = format!("{} ", label);
    let updated = format!(
        "{}{}{}",
        chars[..start].iter().collect::<String>(),
        inserted,
        chars[cursor..].iter().collect::<String>()
    );
    spans = edited_spans(&text, &updated, &spans);
    spans.push(Mention {
        start,
        end: start + label.chars().count(),
        mid: member.mid,
        all: false,
    });
    spans.sort_by_key(|span| span.start);
    state.mentions.borrow_mut().replacing = true;
    state.composer.set_text(&updated);
    state
        .composer
        .set_position((start + inserted.chars().count()) as i32);
    {
        let mut data = state.mentions.borrow_mut();
        data.text = updated;
        data.spans = spans;
        state
            .composer
            .set_attributes(&attributes(&state.composer, &data.text, &data.spans));
        data.replacing = false;
        data.dismissed = true;
        if let Some(popover) = &data.popover {
            popover.popdown();
        }
    }
    state.composer.grab_focus();
}

pub(super) fn wire(state: &AppState) {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .activate_on_single_click(true)
        .build();
    let scroll = gtk::ScrolledWindow::builder()
        .min_content_width(260)
        .max_content_height(260)
        .propagate_natural_height(true)
        .propagate_natural_width(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    let popover = gtk::Popover::builder()
        .autohide(false)
        .has_arrow(false)
        .position(gtk::PositionType::Top)
        .child(&scroll)
        .build();
    popover.set_focusable(false);
    list.set_focusable(false);
    scroll.set_focusable(false);
    popover.set_parent(&state.composer);
    popover.update_property(&[gtk::accessible::Property::Label(&crate::i18n::t(
        "mention_members",
    ))]);
    state.mentions.borrow_mut().popover = Some(popover.clone());
    state.mentions.borrow_mut().list = Some(list.clone());
    let weak = popover.downgrade();
    state.composer.connect_destroy(move |_| {
        if let Some(popover) = weak.upgrade() {
            popover.unparent();
        }
    });
    let s = state.clone();
    list.connect_row_activated(move |_, row| {
        if s.mentions.borrow().failed {
            s.mentions.borrow_mut().failed = false;
            refresh(&s);
        } else {
            choose(&s, row.index() as usize);
        }
    });
    let s = state.clone();
    state.composer.connect_changed(move |entry| {
        {
            let mut data = s.mentions.borrow_mut();
            if data.replacing {
                return;
            }
            let text = entry.text().to_string();
            data.spans = edited_spans(&data.text, &text, &data.spans);
            data.text = text;
            entry.set_attributes(&attributes(entry, &data.text, &data.spans));
            data.dismissed = false;
        }
        refresh(&s);
    });
    let s = state.clone();
    state
        .composer
        .connect_cursor_position_notify(move |_| refresh(&s));
    let s = state.clone();
    state.composer.connect_has_focus_notify(move |entry| {
        if !entry.has_focus()
            && let Some(popover) = &s.mentions.borrow().popover
        {
            popover.popdown();
        }
    });
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let s = state.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if modifiers.intersects(
            gdk::ModifierType::CONTROL_MASK
                | gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::SUPER_MASK
                | gdk::ModifierType::SHIFT_MASK,
        ) {
            return glib::Propagation::Proceed;
        }
        let (popover, list, count) = {
            let data = s.mentions.borrow();
            (
                data.popover.clone(),
                data.list.clone(),
                data.candidates.len(),
            )
        };
        if !popover.is_some_and(|p| p.is_visible()) {
            return glib::Propagation::Proceed;
        }
        if key == gdk::Key::Escape {
            let mut data = s.mentions.borrow_mut();
            data.dismissed = true;
            if let Some(popover) = &data.popover {
                popover.popdown();
            }
            return glib::Propagation::Stop;
        }
        let Some(list) = list else {
            return glib::Propagation::Proceed;
        };
        if count > 0 {
            let selected = list.selected_row().map(|r| r.index()).unwrap_or(0);
            if key == gdk::Key::Down || key == gdk::Key::Up {
                let next = if key == gdk::Key::Down {
                    (selected + 1) % count as i32
                } else {
                    (selected - 1).rem_euclid(count as i32)
                };
                list.select_row(list.row_at_index(next).as_ref());
                return glib::Propagation::Stop;
            }
            if key == gdk::Key::Return || key == gdk::Key::KP_Enter || key == gdk::Key::Tab {
                choose(&s, selected as usize);
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    state.composer.add_controller(keys);
}

// Use the theme's text accent so mentions remain legible in light and dark mode.
#[allow(deprecated)]
pub(super) fn color(widget: &impl IsA<gtk::Widget>) -> gdk::RGBA {
    widget
        .style_context()
        .lookup_color("accent_color")
        .unwrap_or_else(|| widget.color())
}

pub(super) fn attributes(
    widget: &impl IsA<gtk::Widget>,
    text: &str,
    spans: &[Mention],
) -> gtk::pango::AttrList {
    let attrs = gtk::pango::AttrList::new();
    let accent = color(widget);
    let channel = |value: f32| (value.clamp(0.0, 1.0) * u16::MAX as f32).round() as u16;
    let mut boundaries: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
    boundaries.push(text.len());
    for span in spans.iter().take(100) {
        if span.end <= span.start {
            continue;
        }
        let (Some(&start), Some(&end)) = (boundaries.get(span.start), boundaries.get(span.end))
        else {
            continue;
        };
        let mut foreground = gtk::pango::AttrColor::new_foreground(
            channel(accent.red()),
            channel(accent.green()),
            channel(accent.blue()),
        );
        foreground.set_start_index(start as u32);
        foreground.set_end_index(end as u32);
        attrs.insert(foreground);
        let mut weight = gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Bold);
        weight.set_start_index(start as u32);
        weight.set_end_index(end as u32);
        attrs.insert(weight);
        let mut underline = gtk::pango::AttrInt::new_underline(gtk::pango::Underline::Single);
        underline.set_start_index(start as u32);
        underline.set_end_index(end as u32);
        attrs.insert(underline);
    }
    attrs
}

pub(super) fn mention_at(spans: &[Mention], offset: usize) -> Option<&str> {
    spans
        .iter()
        .take(100)
        .find(|m| !m.all && m.start <= offset && offset < m.end && is_user_mid(&m.mid))
        .map(|m| m.mid.as_str())
}

pub(super) fn is_user_mid(mid: &str) -> bool {
    mid.len() > 1 && mid.starts_with('u')
}

fn mention_at_byte<'a>(text: &str, spans: &'a [Mention], byte: usize) -> Option<&'a str> {
    if byte > text.len() || !text.is_char_boundary(byte) {
        return None;
    }
    mention_at(spans, text[..byte].chars().count())
}

fn label_mention(
    label: &gtk::Label,
    text: &str,
    spans: &[Mention],
    x: f64,
    y: f64,
) -> Option<String> {
    let (ox, oy) = label.layout_offsets();
    let scale = gtk::pango::SCALE as f64;
    let (inside, index, _) = label.layout().xy_to_index(
        ((x - ox as f64) * scale) as i32,
        ((y - oy as f64) * scale) as i32,
    );
    if !inside {
        return None;
    }
    mention_at_byte(text, spans, usize::try_from(index).ok()?).map(str::to_string)
}

/// Opens the profile of a mentioned user when the label is clicked (not dragged).
pub(super) fn wire_label_click(
    state: &AppState,
    label: &gtk::Label,
    text: &str,
    spans: &[Mention],
) {
    if !spans.iter().any(|m| !m.all && is_user_mid(&m.mid)) {
        return;
    }
    let (text, spans) = (text.to_string(), spans.to_vec());
    let click = gtk::GestureClick::builder()
        .button(1)
        .propagation_phase(gtk::PropagationPhase::Capture)
        .build();
    let press = Rc::new(std::cell::Cell::new((0.0, 0.0)));
    {
        let press = press.clone();
        click.connect_pressed(move |_, _, x, y| press.set((x, y)));
    }
    {
        let (state, label, text, spans) =
            (state.clone(), label.clone(), text.clone(), spans.clone());
        click.connect_released(move |gesture, n, x, y| {
            let (px, py) = press.get();
            if n != 1
                || (x - px).abs() > 4.0
                || (y - py).abs() > 4.0
                || label.selection_bounds().is_some()
            {
                return;
            }
            if let Some(mid) = label_mention(&label, &text, &spans, x, y) {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                user_profile::open(&state, &mid);
            }
        });
    }
    label.add_controller(click);
    let motion = gtk::EventControllerMotion::new();
    {
        let label = label.clone();
        motion.connect_motion(move |_, x, y| {
            let over = label_mention(&label, &text, &spans, x, y).is_some();
            label.set_cursor_from_name(over.then_some("pointer"));
        });
    }
    label.add_controller(motion);
}

pub(super) fn mentions_account(message: &MessageInfo, account: Option<&str>) -> bool {
    !message.mine
        && !message.decryption_failed
        && account.is_some_and(|mid| {
            message
                .mentions
                .iter()
                .any(|mention| mention.all || mention.mid == mid)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mention(start: usize, end: usize) -> Mention {
        Mention {
            start,
            end,
            mid: "u-person".into(),
            all: false,
        }
    }
    #[test]
    fn edits_preserve_identity_only_outside_mention() {
        let spans = vec![mention(2, 6)];
        assert_eq!(
            edited_spans("😀 @Amy hi", "前😀 @Amy hi", &spans),
            vec![mention(3, 7)]
        );
        assert_eq!(edited_spans("😀 @Amy hi", "😀 @Amy hello", &spans), spans);
        assert!(edited_spans("😀 @Amy hi", "😀 @Ann hi", &spans).is_empty());
        assert!(edited_spans("😀 @Amy hi", "😀 hi", &spans).is_empty());
        assert_eq!(
            edited_spans("😀 @Amy hi", "😀 X@Amy hi", &spans),
            vec![mention(3, 7)]
        );
    }
    #[test]
    fn trigger_ignores_email_and_existing_mentions() {
        assert_eq!(query("Hi @Am", 6, &[]), Some((3, "am".into())));
        assert!(query("mail@example", 12, &[]).is_none());
        assert!(query("@Amy", 4, &[mention(0, 4)]).is_none());
    }
    #[test]
    fn mention_hit_test_uses_char_offsets_and_skips_all() {
        let spans = vec![
            mention(2, 6),
            Mention {
                start: 8,
                end: 12,
                mid: String::new(),
                all: true,
            },
            Mention {
                start: 13,
                end: 16,
                mid: "c-group".into(),
                all: false,
            },
        ];
        assert_eq!(mention_at(&spans, 2), Some("u-person"));
        assert_eq!(mention_at(&spans, 5), Some("u-person"));
        assert_eq!(mention_at(&spans, 6), None);
        assert_eq!(mention_at(&spans, 9), None);
        assert_eq!(mention_at(&spans, 14), None);
        assert_eq!(mention_at_byte("😀 @Amy hi", &spans, 5), Some("u-person"));
        assert_eq!(mention_at_byte("😀 @Amy hi", &spans, 1), None);
    }
    #[test]
    fn notification_requires_own_identity_in_metadata() {
        let mut msg: MessageInfo = serde_json::from_value(
            serde_json::json!({"text":"@Me", "mentions":[{"start":0,"end":3,"mid":"u-me"}]}),
        )
        .unwrap();
        assert!(mentions_account(&msg, Some("u-me")));
        assert!(!mentions_account(&msg, Some("u-other")));
        assert!(!mentions_account(&msg, None));
        msg.mine = true;
        assert!(!mentions_account(&msg, Some("u-me")));
    }
}
