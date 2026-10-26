use super::*;
use std::io::{Cursor, Read};
use std::time::Duration;

struct Preview {
    title: String,
    description: String,
    image: Option<Vec<u8>>,
}

pub(super) fn append(bubble: &gtk::Box, text: &str) {
    for url in extract_urls(text).into_iter().take(4) {
        let Ok(parsed) = reqwest::Url::parse(&url) else {
            continue;
        };
        if !matches!(parsed.scheme(), "http" | "https") {
            continue;
        }
        let domain = parsed.host_str().unwrap_or(&url).to_string();
        let card = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(3)
            .css_classes(["line-link-preview"])
            .build();
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Contain)
            .width_request(320)
            .height_request(180)
            .visible(false)
            .css_classes(["line-link-preview-image"])
            .build();
        card.append(&picture);
        let domain_label = gtk::Label::builder()
            .label(&domain)
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(["line-link-preview-domain"])
            .build();
        card.append(&domain_label);
        let title = gtk::Label::builder()
            .label(if youtube_id(&url).is_some() {
                crate::i18n::t("youtube_video")
            } else {
                domain.clone()
            })
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .max_width_chars(42)
            .css_classes(["line-link-preview-title"])
            .build();
        card.append(&title);
        let description = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .wrap_mode(gtk::pango::WrapMode::WordChar)
            .lines(3)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(42)
            .css_classes(["line-link-preview-description"])
            .build();
        card.append(&description);

        let actions = gtk::Box::builder()
            .spacing(8)
            .css_classes(["line-link-preview-actions"])
            .build();
        let load = gtk::Button::builder()
            .label(crate::i18n::t("link_preview_load"))
            .css_classes(["flat"])
            .build();
        let open = gtk::LinkButton::builder()
            .label(crate::i18n::t("link_preview_open"))
            .uri(&url)
            .css_classes(["flat"])
            .build();
        actions.append(&load);
        actions.append(&open);
        card.append(&actions);
        bubble.append(&card);

        let title_c = title.clone();
        let description_c = description.clone();
        let picture_c = picture.clone();
        load.connect_clicked(move |button| {
            button.set_sensitive(false);
            button.set_label(&crate::i18n::t("link_preview_loading"));
            let (tx, rx) = async_channel::bounded(1);
            let url = url.clone();
            let domain = domain.clone();
            std::thread::spawn(move || {
                let _ = tx.send_blocking(fetch(&url, &domain));
            });
            let button = button.clone();
            let title = title_c.clone();
            let description = description_c.clone();
            let picture = picture_c.clone();
            glib::spawn_future_local(async move {
                match rx.recv().await {
                    Ok(Ok(preview)) => {
                        title.set_text(&preview.title);
                        description.set_text(&preview.description);
                        if let Some(bytes) = preview.image
                            && let Ok(pixbuf) = Pixbuf::from_read(Cursor::new(bytes))
                        {
                            picture.set_paintable(Some(&gdk::Texture::for_pixbuf(&pixbuf)));
                            picture.set_visible(true);
                        }
                        button.set_visible(false);
                    }
                    _ => {
                        description.set_text(&crate::i18n::t("link_preview_unavailable"));
                        button.set_label(&crate::i18n::t("link_preview_retry"));
                        button.set_sensitive(true);
                    }
                }
            });
        });
    }
}

fn fetch(url: &str, domain: &str) -> Result<Preview, ()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .map_err(|_| ())?;
    let response = client
        .get(url)
        .send()
        .map_err(|_| ())?
        .error_for_status()
        .map_err(|_| ())?;
    let html_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.contains("text/html") || value.contains("application/xhtml+xml")
        });
    if !html_type
        || response
            .content_length()
            .is_some_and(|size| size > 256 * 1024)
    {
        return Err(());
    }
    let page_url = response.url().clone();
    let mut bytes = Vec::new();
    response
        .take(256 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() > 256 * 1024 {
        return Err(());
    }
    let html = String::from_utf8_lossy(&bytes);
    let title = meta(&html, "og:title")
        .or_else(|| meta(&html, "twitter:title"))
        .or_else(|| page_title(&html))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| domain.to_string());
    let description = meta(&html, "og:description")
        .or_else(|| meta(&html, "description"))
        .or_else(|| meta(&html, "twitter:description"))
        .unwrap_or_default();
    let image = meta(&html, "og:image")
        .or_else(|| meta(&html, "twitter:image"))
        .and_then(|url| page_url.join(&url).ok())
        .filter(|url| {
            matches!(url.scheme(), "http" | "https")
                && (page_url.scheme() != "https" || url.scheme() == "https")
        })
        .and_then(|url| fetch_image(&client, &url));
    Ok(Preview {
        title: truncate(&title, 140),
        description: truncate(&description, 280),
        image,
    })
}

fn fetch_image(client: &reqwest::blocking::Client, url: &reqwest::Url) -> Option<Vec<u8>> {
    let response = client
        .get(url.clone())
        .send()
        .ok()?
        .error_for_status()
        .ok()?;
    let image_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("image/"));
    if !image_type
        || response
            .content_length()
            .is_some_and(|size| size > 2 * 1024 * 1024)
    {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 2 * 1024 * 1024 {
        return None;
    }
    let reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || width as u64 * height as u64 > 12_000_000
    {
        return None;
    }
    let thumbnail = image::load_from_memory(&bytes).ok()?.thumbnail(320, 180);
    let mut png = Cursor::new(Vec::new());
    thumbnail.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}

fn meta(html: &str, key: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(offset) = lower[from..].find("<meta") {
        let start = from + offset;
        let end = tag_end(html, start)?;
        let tag = &html[start..=end];
        if attribute(tag, "property")
            .or_else(|| attribute(tag, "name"))
            .is_some_and(|name| name.eq_ignore_ascii_case(key))
        {
            return attribute(tag, "content").map(|value| entities(&value));
        }
        from = end + 1;
    }
    None
}

fn tag_end(html: &str, start: usize) -> Option<usize> {
    let mut quote = None;
    for (offset, byte) in html.as_bytes()[start..].iter().copied().enumerate() {
        match (quote, byte) {
            (None, b'\'' | b'"') => quote = Some(byte),
            (Some(current), byte) if current == byte => quote = None,
            (None, b'>') => return Some(start + offset),
            _ => {}
        }
    }
    None
}

fn page_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let content = lower[start..].find('>')? + start + 1;
    let end = lower[content..].find("</title>")? + content;
    Some(entities(html[content..end].trim()))
}

fn attribute(tag: &str, wanted: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let mut i = 1;
    while i < bytes.len() {
        while i < bytes.len() && !bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
            i += 1;
        }
        if start == i {
            break;
        }
        let name = &tag[start..i];
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let quote = bytes
            .get(i)
            .copied()
            .filter(|byte| *byte == b'"' || *byte == b'\'');
        if quote.is_some() {
            i += 1;
        }
        let value_start = i;
        while i < bytes.len()
            && match quote {
                Some(q) => bytes[i] != q,
                None => !bytes[i].is_ascii_whitespace() && bytes[i] != b'>',
            }
        {
            i += 1;
        }
        let value = &tag[value_start..i];
        if quote.is_some() && i < bytes.len() {
            i += 1;
        }
        if name.eq_ignore_ascii_case(wanted) {
            return Some(value.to_string());
        }
    }
    None
}

fn entities(value: &str) -> String {
    let mut result = value.to_string();
    for (encoded, decoded) in [
        ("&amp;", "&"),
        ("&quot;", "\""),
        ("&#39;", "'"),
        ("&apos;", "'"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&nbsp;", " "),
    ] {
        result = result.replace(encoded, decoded);
    }
    result.trim().to_string()
}

fn truncate(value: &str, max: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max {
        value.to_string()
    } else {
        format!("{}…", value.chars().take(max).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_open_graph_metadata_with_attribute_order_and_quotes() {
        let html = r#"<meta content='A &amp; B' property='og:title'>
            <meta name="description" content="Short description"><title>Fallback</title>"#;
        assert_eq!(meta(html, "og:title").as_deref(), Some("A & B"));
        assert_eq!(
            meta(html, "description").as_deref(),
            Some("Short description")
        );
        assert_eq!(page_title(html).as_deref(), Some("Fallback"));
        assert_eq!(
            meta(r#"<meta property="og:title" content="A > B">"#, "og:title").as_deref(),
            Some("A > B")
        );
    }
}
