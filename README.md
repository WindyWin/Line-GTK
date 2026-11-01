# Line-GTK

Unofficial native LINE client for Linux, with a GTK4/Libadwaita interface and a Deno protocol sidecar powered by [linejs](https://github.com/evex-dev/linejs).

**README:** English · [Tiếng Việt](README.vi.md) · [日本語](README.ja.md)

> The `personal/features` branch is for personal feature development and is separate from `main`, which is used for upstream pull requests. It integrates Japanese UI and font support, message decryption, group information, and email login changes.

![Line-GTK main window](assets/761546283_1013837091419784_4898538377611644175_n.jpg)

## Features

Everything checked below is implemented in the current Line-GTK feature set. The sections after this list describe behavior and limits in detail.

**Sign-in and account**

- [x] QR login as an additional device; email and password login with phone verification when needed
- [x] Optional automatic sign-in at startup, with credentials stored in the OS keyring
- [x] Session restore, launch at login, optional start hidden in the tray, and logout
- [x] Letter Sealing decryption, encryption-key import from the phone, group-key recovery, and key re-request from a failed message
- [x] Message cache encryption when a keyring key is available, plus per-chat limits and cleanup warnings

**Chats and messages**

- [x] 1:1, group, and multi-person chats, official accounts, friends list, add friend by LINE ID, and user profiles
- [x] Text, photos, videos, voice messages, files, stickers, Flex cards, and link preview cards
- [x] Quoted replies, jump to the original message, jump to the last read message, and older-history paging by scrolling
- [x] Copy a whole message or select part of it; unsend your own messages and apply unsends from other devices
- [x] Six standard LINE reactions and LINE emoji reactions, with a who-reacted list
- [x] Group mentions with an @ picker, @all, mention notifications, and mentioned-member profiles
- [x] Unicode emoji input and inline display of received LINE emoji
- [x] Native LINE text polls: view, vote, and create with multiple choice, anonymous votes, member-added options, and a closing date
- [x] In-chat message search over saved history, 50 results per page, with jump to the message
- [x] Chat filters, name/preview search, pinning, hiding, blocking, muting, and per-account persistence
- [x] Read-receipt pause globally or per contact, mark read up to now, and unread sync from other devices
- [x] Drafts, attachment previews, paste and drag-and-drop, and messages queued while reconnecting

**Stickers and emoji**

- [x] Sticker picker with account-owned packs, creators' packs from LINE STORE, and personal PNG/APNG packs
- [x] Sticker import with per-file validation, progress, cancellation, retry, rename, remove, and restore of older packs
- [x] Size and background editing, GIF conversion, optimization (resolution, frame rate, palette), and animated hover previews
- [x] Received LINE emoji as inline images; owned LINE emoji available for reactions

**Groups, media, and files**

- [x] Named group notices for invitations, joins, leaves, removals, renames, and picture changes
- [x] Group member list with search, pending invitations, profiles, and inviting up to 100 friends
- [x] Group sender names in bubbles, quotes, and notification titles
- [x] Conversation albums and original photo saving; shared media browser for photos, videos, links, and files
- [x] Image viewer with zoom and drawing, video player, PDF/text/Markdown previews, and open/show-in-folder actions
- [x] Download folders, ask-every-time saving, file-name collision handling, and download progress
- [x] Call-history cards for incoming, outgoing, missed, and unanswered calls

**Desktop, recovery, and appearance**

- [x] Desktop notifications with sender/group titles, sticker images, mention titles, sound, volume, and per-chat mute
- [x] System tray with recent chats, unread badge, mute timers, close-to-tray, and autostart
- [x] Resume after sleep and network recovery, reconnect retries, stalled-receiver detection, and cached chats at startup
- [x] Discord Rich Presence, themes, font and UI scaling, animations, and Vietnamese (default), Thai, English, and Japanese UI
- [x] Cache retention, per-chat message limits, granular cache clearing, diagnostics, and protocol-engine restart
- [x] Experimental 1:1 voice calls behind an opt-in setting; they require QR login and may be unstable

### Not yet available

These are native LINE features this client does not provide. The experimental voice-call option above is not a substitute for stable calling.

**Calls and account**

- [ ] Stable voice calls, video calls, group calls, and screen sharing
- [ ] Call backgrounds, effects, and other calling features from the official client
- [ ] Account switcher and multiple signed-in accounts
- [ ] Linked-device management, account backup/restore, and migration
- [ ] Editing your own profile (name, status message, avatar, cover) and privacy/security settings
- [ ] Friend requests (accept/reject), favorites, delete/hide friends, and QR-code/ID discovery beyond adding a LINE ID
- [ ] Chat backup and restore

**Groups and chat**

- [ ] Creating groups, leaving or deleting groups, removing members, renaming, changing group pictures, and invitation-link controls
- [ ] Group Notes/posts and group album management
- [ ] Forwarding and sharing messages, multi-select actions, and pinning individual messages
- [ ] Full-text search across all chats (search is per conversation and limited to saved history)
- [ ] Per-chat backgrounds/themes, message translation, notification quick reply, and message effects
- [ ] Keep, Keep Memo, and saving messages to Keep
- [ ] Official-account rich menus, buttons, and quick replies

**Media and message types**

- [ ] Creating, renaming, and deleting albums; adding or removing album photos; album video
- [ ] Sending multiple photos at once, video messages, and other attachment types
- [ ] Location and live-location sharing, contact cards, and other rich message types
- [ ] In-app image editing beyond drawing

**Stickers and emoji**

- [ ] Buying and managing stickers and LINE emoji sets inside the app
- [ ] Sending purchased LINE emoji as message content and animated LINE emoji playback
- [ ] Publishing personal sticker packs to LINE (personal packs here are local photo collections)
- [ ] Sticker effects, sounds, and popup animations

**Polls and other LINE services**

- [ ] Date and image polls, and editing, closing, or deleting polls
- [ ] OpenChat discovery, joining, and management
- [ ] Timeline/VOOM, LINE Today, LINE Pay, Shopping, Games, and premium subscription features
- [ ] AI/prototype features in the official client: chat summary/search/translation, OCR translation, natural-language sticker search, AI agent, and call backgrounds

### Read receipts and unread state

Turn on **Do not mark as read** at the top of the window to stop read receipts from this app. The setting survives restarts, and unread badges remain. Turning it off does not mark everything read at once: receipts resume through normal activity, such as opening a chat. In an already open chat at the latest messages, clicking the conversation or composer, typing, or scrolling may send a receipt. Reading older history, the global pause, or a per-contact pause prevents this. Receipts already sent or sent by other devices are unaffected.

To pause one contact, right-click their chat and choose **Do not mark this friend as read**. Their avatar gains an eye-off mark. Opening the chat or receiving messages leaves them unread. Use the same menu to resume; resuming alone sends nothing. Per-contact settings are stored per account. Resuming a contact has no effect while the global pause is on or automatic read receipts are off.

Use **Mark read up to here** at the top of a chat or in its context menu to mark only messages received so far. Pause settings stay on, so later messages remain unread. Only a successfully sent range updates the unread count; new messages arriving during the request or a failed request cannot incorrectly clear it. Read notifications from another device update the chat, open conversation, and tray even while local receipts are paused. This synchronization never sends a receipt and checks the server when needed to avoid clearing newer unread messages.

### Messages and chat controls

Click the arrow beside a message or choose **Reply** from its context menu to quote it. Cancel with × or Esc; switching chats clears the quote. Use the smile button or **React** menu to pick one of six standard reactions, a recent LINE emoji, or an emoji from an owned set. Click a reaction under a bubble to toggle yours; right-click or long-press it to see who reacted. Reactions from other devices are stored per account. The six standard reactions use LINE icons in `assets/reactions`. LINE may reject sending an emoji reaction, depending on conditions such as ownership; the official desktop client only displays received LINE emoji reactions.

Open a conversation and click the search icon or press **Ctrl+F** to search saved message text and file names. Results appear newest first, with 50 per page; select one to jump to it in the conversation. Search covers history saved on this computer, so load older messages first if they are missing. Press **Esc** to close search.

Right-click a contact chat and confirm **Block** to block that person on LINE. Unblock from the same menu or **Settings → Features → Blocked users**. History remains, the list shows a blocked mark, and the one-to-one composer and send requests are disabled. Their group messages still appear. The server block list refreshes at startup, on resume, and after changes from another device. A failed request retains the last confirmed state. Blocking does not change read-pause settings or send a read receipt. Group behavior follows [LINE's help](https://help2.line.me/line/smartphone?contentId=200002013&lang=ja).

Right-click a friend chat to pin or unpin it. Pinned chats appear first; each group remains sorted by recency. Search, category, and hidden-chat filters still apply. Pins are stored per account on this computer, and pinning sends no read receipt. Right-click a chat to hide it from this computer's chat list, search results, and recent tray chats. Restore it under **Settings → Features → Hidden chats**. History is retained, and hiding or restoring sends no receipt. Hiding the open chat clears the selection. Hidden state persists through new messages and restarts per account, but does not sync to other devices; notification settings are independent.

Use **Unsend** on one of your messages and confirm to request removal on LINE. The app replaces its content only after LINE confirms success; a failure leaves the original. LINE decides eligibility and time limits and shows its standard notice to recipients. Unsend events from other devices are stored per account, so reloading history or restarting does not restore removed content. Already downloaded photos and files are not deleted.

The sticker picker supports account-owned LINE packs, creators’ packs downloaded from LINE STORE, and personal PNG collections. Choose **From LINE STORE**, browse creators’ packs in your browser or paste a pack link/ID, then choose **Preview pack → Download**. Downloads show progress, can be cancelled, and retain completed images; **Retry missing images** fetches unavailable items again. Owned packs use native LINE stickers; downloaded artwork is labelled **Sends as photos**. Owned collections load their grids when selected, including packs beyond the former 12-pack limit.

Choose **Import PNG files** to review up to 100 PNG or APNG files and add selected files to a named new or existing personal pack. Each file must be at most 10 MiB and 2048 × 2048 pixels. The app saves private copies, so moving or deleting source files does not break the pack. Identical content is deduplicated; different files with the same name remain separate. Corrupt or unsupported files produce per-file errors. Animated PNGs are limited to 240 frames, 30 seconds, and 32 million decoded canvas pixels. Personal images default to **Original size**, with background removal off.

Click an image to send it using its saved settings, or choose **Edit** (also available by right-click or Shift+F10) to preview size and background changes. Downloaded store images keep the previous **Large** / **Auto-remove background** defaults. Settings are independent for each account, pack, and image. Flat-background removal preserves enclosed artwork; LINE controls the final photo presentation. Rename packs, add PNGs to personal packs, or remove library images/packs without changing source files or sent messages. The picker renders 60 items per page and shows cached hover previews.

Animated creators’ packs download their APNG artwork, with static fallback when animation is unavailable. Store previews, imported-file previews, and picker hover previews can play cached animation; the global animation setting applies. Imported animations are converted to GIF on send, using the existing animated IMAGE transport. Choose **Optimize image** on an item to compare file sizes, optionally resize its longest side (96–1024 px, preserving aspect ratio without upscaling), and adjust GIF frame rate (5–30 fps) and palette (32–256 colors). New animations default to **128 px, 10 fps, and 64 colors** for smaller downloads. Existing saved settings are kept; **Small GIF** applies and previews the lightweight preset. Static images keep their original dimensions by default and use lossless PNG compression; GIF conversion can increase file size and uses binary transparency. Preview before saving or sending. Settings persist per image, originals stay intact, and generated files are cached separately. Reimport a previously downloaded static pack’s link to retrieve its animated artwork.

Saved image packs and originals persist outside the clearable sticker cache. Thumbnails, review previews, and send derivatives remain cache data. **Restore previous image packs** explicitly adopts the older shared library into the current account and migrates saved image settings. See [the implementation plan and validation notes](docs/custom-sticker-library-plan.md).

The composer emoji button inserts Unicode emoji as text at the cursor, using the system emoji font (`noto-fonts-emoji` on Arch). Received LINE emoji display as still images inline, with up to four background downloads into the media cache. If an image is unavailable, its alternative text remains; quotes and chat previews also show alternative text. Sending owned LINE emoji and animated LINE emoji playback are not supported.

### Groups, notifications, and recovery

Group invitation, join, leave, and removal events appear as named notices, including events involving multiple members. Names load in the background. Events received by this computer are stored per account and interleaved with messages by time after reopening or restarting. Limits are 500 events per chat and 5,000 per account; clearing the message cache removes them. Notices cannot be replied to and their IDs are never used for read receipts. Historical group events are unavailable from the message history API, so events missed while the app was closed or disconnected appear only if LINE redelivers them. Group and multi-person bubbles show sender names, including members who are not friends, and update names and quoted sender names after background lookup.

Open **⋮ → Conversation settings**, or right-click a chat and choose **Conversation settings**. Group chats show searchable members and pending invitations; click a member to view their profile. **Invite friends** lets you select up to 100 friends and send LINE invitations. The picker excludes current members, people already invited, and official accounts, and rechecks eligibility before sending. Refresh to see membership changes from other devices. Opening settings from the chat list does not open the conversation or mark it read. Pin, hide, notification, and shared-content shortcuts are also available. Invitation links, QR invitations, member removal, and group renaming are not implemented.

Group notifications show **Sender (Group)** for text, photos, and stickers. Name lookup waits up to 1.2 seconds, then uses a distinct temporary member label if needed and silently updates the same notification later. Sticker notifications show a still image when the notification server supports body images. Image lookup also waits up to 1.2 seconds; a text notification may appear first and be silently updated. A dismissed notification does not reopen. Other servers use an image hint and sticker icon, with text as fallback. Notification images are transparent PNGs up to 200 × 100 pixels and are removed with the sticker cache. Clicking a notification opens the chat; mute settings still apply.

On resume from sleep, the app reconnects its receive stream after two seconds using the saved session, refreshes the chat list and latest 80 messages of the open chat, and merges them without duplicates. Drafts, quotes, scroll position, hidden and pinned chats, and read-pause settings remain. Refresh itself sends no receipt. Pending outgoing bubbles merge after sending finishes. If disconnected, the current view remains and retries occur after 3, 10, and 30 seconds. A **Retry** action appears if needed, and recovery also runs when the network returns. The app uses logind resume notifications, with periodic timing as fallback. Chat previews update when messages load; background loading processes two chats at a time and falls back to **Tap to open** when the latest message is unavailable.

### Media and albums

Click a video thumbnail's play button for a viewer with playback, position, elapsed and total time, and volume controls. Replay after completion, save the video itself, or stop playback by closing the viewer. Playback requires a GTK media backend and suitable codecs. If it fails, use **Retry**, **Open in another app**, or download. If LINE no longer provides the original, the app shows an error instead of saving its thumbnail as a video. Album videos are outside the current feature set.

Use **Album** at the top of a chat to open its albums, or **All chats** for the full list. With no chat selected, the full list appears by default. Open a photo for a larger view and save its original from the photo or image viewer. Existing download location and dialog settings apply; normal saving does not overwrite a same-named file. Originals load 12 at a time, with up to three concurrent downloads, and are reused from the media cache. JPEG, PNG, GIF, and WebP are supported up to 50 MB per original. Album videos are marked unsupported; creating albums or adding and deleting photos is unavailable. Use **Retry** after a failed download or refresh stale album information.

## Requirements

Email login authenticates this client as an additional PC device and may require a phone verification code. Normally the app restores a session using PC credentials. Enable **Save login details and sign in automatically at startup** to save email and password in the OS keyring after successful login. You can add, change, or delete them under **Settings → Account → Automatic sign-in at startup**.

Automatic sign-in runs once at startup. It prefers a saved session and tries email login only if that session is missing or invalid. Temporary network errors do not trigger password login, and a later disconnection caused by login from another PC does not trigger automatic re-login. Required phone verification is shown; incorrect credentials return to manual login. Credentials are stored in a Secret Service-compatible keyring, such as GNOME Keyring, never in config files or command arguments. A keyring failure disables automatic sign-in or returns to manual login. Unlocking the keyring may be required. Removing credentials or logging out disables automatic sign-in and deletes saved credentials. Use QR login for the experimental voice-call feature.

| Dependency | Notes |
| --- | --- |
| Rust | Stable toolchain |
| GTK4 + Libadwaita | GTK4 4.14+ and Libadwaita 1.5+; `pacman -S gtk4 libadwaita` on Arch |
| Deno | Deno 2; build-time protocol compiler; source runs use `deno` or `DENO` |
| Python 3 | Dependency verification and package smoke tests |
| ffmpeg / ffprobe | Voice recording and video thumbnails |
| libsecret (`secret-tool`) + Secret Service | Required for automatic email sign-in |
| Optional | `pdftoppm`, `pdftotext` for PDF previews |

Wayland and X11 both work. Testing has mainly been on Arch-based desktops.

## Install

### Arch / AUR

```bash
# Development snapshot
yay -S line-gtk-git

# Tagged release
yay -S line-gtk
```

### Flatpak bundle

```bash
flatpak install --user ./line-gtk-0.1.2-x86_64.flatpak
flatpak run dev.linegtk.LineGtk
```

## Build and run

Vietnamese and English use `Noto Sans` by default, including as fallback for custom chat and composer fonts. Install `noto-fonts` on Arch Linux. Existing Vietnamese settings using the old `Noto Sans Thai` default migrate automatically; custom fonts and an empty system-font setting are preserved. Selecting Japanese uses `Noto Sans CJK JP` with the default font; custom fonts are retained. Install `noto-fonts-cjk` on Arch Linux for Japanese.

```bash
git clone https://github.com/MidnightTale/Line-GTK.git
cd Line-GTK
cargo build --release
./scripts/build-sidecar.sh
LINE_GTK_ROOT="$PWD" ./target/release/line-gtk
```

Builds automatically download checksum-pinned libraries, including linejs, from GitHub into the ignored `protocol/vendor/deps` directory. Builds do not contact JSR. Version pins, the source manifest, and `protocol/deno.lock` remain committed. The first build needs GitHub, the npm registry, and the Deno compiler runtime. Once cached, use `DENO_OFFLINE=1 ./scripts/build-sidecar.sh` to prevent dependency downloads. See the [dependency and connection recovery notes](docs/protocol-reliability.md).

## Development checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
deno task --config protocol/deno.json check
```

## Fork integration

See the [upstream change list](docs/upstream-changes.md) for added features,
protocol and packaging impacts, validation results, and remaining limitations.

This branch integrates documented changes from [shirafukayayoi/Line-GTK](https://github.com/shirafukayayoi/Line-GTK) through `personal/features` commit `0907007`, while keeping Vietnamese as the app's default language. See the [Vietnamese integration notes](docs/fork-shirafukayayoi.md) for features and limits.

## Message decryption and group information

Line-GTK uses linejs 3.5.0 to address message bodies appearing as `(none)`. If an earlier login saved the wrong encryption key, use the key refresh button under **Settings → Account** or on a decryption-failed bubble to reauthorize with a phone QR scan. Existing private keys are kept, and the correct key generation is retrieved from the phone. Group information loads in batches; names and images are stored for later launches.

## Warning

This is an unofficial client. Using it may violate LINE's Terms of Service. Prefer a secondary account or device session. LINE login or encryption changes may break the client.

## Acknowledgments

Protocol and end-to-end encryption access LINE through [linejs](https://github.com/evex-dev/linejs) by [evex-dev](https://github.com/evex-dev). This app is a GTK frontend around that library and is not affiliated with LINE Corporation or evex-dev.

## License

[GPL-3.0-or-later](LICENSE)
