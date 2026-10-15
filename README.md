Unofficial native LINE client for Linux. GTK4 / Libadwaita UI with a Deno protocol sidecar ([linejs](https://github.com/evex-dev/linejs)).

> この `personal/features` ブランチは自分用の機能を開発するためのものです。上流へのPR用の `main` とは分離しています。
> 画面上部の「既読をつけない」をオンにすると、このアプリからの既読通知を停止します。設定は再起動後も維持され、オンの間は未読バッジも残ります。オフに戻しただけではまとめて既読にせず、次にトークを開くなどの通常操作から通知を再開します。すでに付いた既読や、別の端末で付く既読には影響しません。
>
> 元の開発ブランチは `main` です。日本語UI・フォント、メッセージ復号・グループ名、メールログインの変更を統合して開発しています。
> トーク一覧のプレビューは本文の取得時にも更新し、背景取得は同時2件で一覧全体を処理します。最新の本文を取得できない場合は「タップして開く」に戻し、読み込み中の表示を残しません。

![main](./assets/761546283_1013837091419784_4898538377611644175_n.jpg)

---

## Implementation

- [x] QR login (additional device)
- [x] メールアドレス・パスワードでログイン（スマホでの認証番号入力に対応）
- [x] Chats, messages, stickers, friends
- [x] メッセージ横の矢印ボタン・右クリックから引用付きのテキスト返信（リプライ）
- [x] トークを「全部・友だち・グループ・公式アカウント」で絞り込み、カテゴリ内を検索
- [x] トークを右クリックで非表示にし、設定から再表示
- [x] グループの吹き出しに送信者名を表示し、引用元の名前も更新
- [x] アルバム一覧・写真の閲覧、拡大表示、原本の保存
- [x] Photos, video, voice messages, files
- [x] Paste and drag-and-drop attachments
- [x] In-app viewers (image, video, PDF, text)
- [x] Downloads, tray, Thai / English / Japanese, themes
- [x] Discord RPC
- [ ] Voice call (WIP)
- [ ] Account switcher

メッセージ横の矢印ボタンを押すと引用元が選ばれ、そのまま入力欄で返信を書けます。右クリックの「リプライ」からも選べます。×ボタンまたはEscで取り消せます。別のトークへ移ると引用元を解除します。

グループ・複数人トークでは、相手の吹き出しの上に送信者名を表示します。友だち登録していないメンバーも名前を取得し、取得中はメンバーを区別できる仮表示を出します。本文の表示を待たせず、名前や引用元の表示を後から更新します。取得に失敗した場合は、次回の履歴表示や新着時に一定時間を置いて再確認します。

トーク一覧を右クリックして「非表示」を選ぶと、このPCの一覧・検索結果・トレイの最近のトークから隠せます。設定 → 機能 → 非表示のトークで「再表示」を押すと戻せます。トーク履歴は削除せず、非表示・再表示だけでは既読を付けません。開いているトークを非表示にすると、別のトークを自動で開かずに選択を解除します。

非表示の状態はアカウントごとに保存し、新着メッセージを受信しても、再起動しても維持します。スマホや他のPCの表示状態には同期しません。通知の設定は変えないため、必要に応じて通知を別途オフにしてください。

トーク上部の「アルバム」から、そのトークのアルバムを開けます。「すべてのトーク」で全体の一覧へ切り替えられ、トークを選んでいないときは最初から全体を表示します。アルバム内の写真を押すと拡大でき、写真ごとの「保存」や画像ビューアーのダウンロードボタンから原本を保存できます。保存先と保存ダイアログの有無は既存の写真ダウンロード設定に従います。通常保存は同名ファイルを上書きしません。

写真は12件ずつ原本を取得し、同時取得は3枚までに制限します。取得済みの原本はメディアキャッシュとして再利用し、キャッシュ削除の対象になります。JPEG・PNG・GIF・WebPに対応し、原本は1枚50MBまでです。動画は未対応の表示を出し、アルバムの作成・写真の追加・削除は行いません。取得失敗時は「再試行」、情報が古くなった場合は更新ボタンを使えます。

---

## Requirements

メールログインはPCの追加デバイスとして認証します。スマホ側で認証番号の入力が必要になる場合があります。
パスワードは保存せず、次回はPC用の認証情報でセッションを復元します。
音声通話の試験機能を使う場合はQRコードでログインしてください。

| Dependency | Notes |
| --- | --- |
| Rust | stable toolchain |
| GTK4 + Libadwaita | `pacman -S gtk4 libadwaita` |
| Deno | Build-time protocol compiler; source-tree runs use `deno` or `DENO` |
| ffmpeg / ffprobe | voice record, video thumbs |
| Optional | `pdftoppm`, `pdftotext` for PDF preview |

Wayland and X11 both work. Tested mainly on Arch-based desktops.

---

## Install (Arch / AUR)

```bash
# Development snapshot
yay -S line-gtk-git

# Tagged release
yay -S line-gtk
```

## Install (Flatpak bundle)
```bash
flatpak install --user ./line-gtk-0.1.2-x86_64.flatpak
flatpak run dev.linegtk.LineGtk
```

## Build and run

設定の表示言語で「日本語」を選択できます。標準フォントを使用している場合は
「Noto Sans CJK JP」に切り替え、指定済みの独自フォントは維持します。
Arch Linuxでは `noto-fonts-cjk` をインストールしてください。

```bash
git clone https://github.com/MidnightTale/Line-GTK.git
cd Line-GTK
cargo build --release
./target/release/line-gtk
```

## Development checks

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
deno task --config protocol/deno.json check
```

---

## Warning

This is an unofficial client. Using it may violate LINE Terms of Service. Prefer a secondary account or device session. Expect breakage when LINE changes login or crypto.

---

## Acknowledgments

Protocol and E2EE talk to LINE through [linejs](https://github.com/evex-dev/linejs) by [evex-dev](https://github.com/evex-dev). This app is a GTK frontend around that library; it is not affiliated with LINE Corporation or evex-dev.

---

## License

[GPL-3.0-or-later](LICENSE)

## メッセージの復号とグループ情報

本文が `(none)` になる問題に対応するため、linejs 3.5.0を使用します。
以前のログインで誤った鍵が保存されていた場合は、設定の「アカウント」または
復号失敗の吹き出しにある暗号鍵の再取得ボタンから、スマホでQR再認証してください。
保存済みの秘密鍵は保持し、正しい世代の鍵をスマホから受け取ります。
グループ情報は小分けに取得し、名前と画像を次回起動時にも表示できるよう保存します。
