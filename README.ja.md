# original-agent

[简体中文](README.md) · [English](README.en.md) · **日本語** · [한국어](README.ko.md)

Original は Rust 製のターミナル向けコーディングエージェントです。自然言語でコードを検索し、ファイルを編集し、プロジェクトのテストを実行できます。編集の取り消しとセッションのローカル保存にも対応しています。

## v0.3.10 をソースからインストール

タグ `v0.3.10` では、検証エラーが繰り返された際のログを短縮し、端末の右端で入力枠が折り返される問題を修正しました。Cargo のバージョンは `0.3.9` で、実行時にもこの番号が表示されます。リリースにはタグと Cargo のバージョンの一致が必要なため、このタグは Rust でビルドしてください。

```sh
cargo install --locked --git https://github.com/haoranyoung09-sys/original-agent --tag v0.3.10 original-agent
original --version
```

## インストール

Windows (PowerShell)

```powershell
iex (irm https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.ps1)
```

Linux / macOS

```sh
curl -fsSL https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.sh | sh
```

ソースからビルドする場合は、リポジトリのルートで `cargo install --locked --path .` を実行します。

## 最初のセッション

プロジェクトのディレクトリで `original` を起動し、`/login` を入力します。矢印キーと Enter で API 接続またはサブスクリプションを選べます。API 接続では Anthropic、OpenAI、DeepSeek または互換サービスを選び、API キーを入力します。モデルはログイン後に `/model <name>` で変更できます。

サブスクリプションを選ぶと、インストール済みの Codex、Claude Code または Gemini CLI が起動します。認証と利用枠は各クライアントが管理します。

| コマンド | 用途 |
|---|---|
| `/settings` | 設定を確認し、思考レベルを変更 |
| `/usage` | 現在のセッションのトークン使用量を確認 |
| `/history` | プロジェクトのセッション記録を表示 |
| `/diff` · `/undo` | ファイルツールの変更を確認・取り消し |
| `/help` | コマンド一覧を表示 |

信頼できるプロジェクトでは `original --auto-verify` で編集後にテストを実行できます。`--allow-shell` を指定すると、エージェントはユーザー権限で任意のシェルコマンドを実行できます。

## 構成

Rust の本体は Anthropic 互換 API にコミュニティ SDK の claudius を、OpenAI 互換 API に reqwest を使用します。Word・PowerPoint の読み書きには Python、画像の文字認識には Tesseract が必要です。

[Office / OCR](docs/archive/office-tools.md) · [Security](SECURITY.md) · [MIT License](LICENSE)
