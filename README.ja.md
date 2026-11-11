# original-agent

[简体中文](README.md) · [English](README.en.md) · **日本語** · [한국어](README.ko.md)

Original は Rust 製のターミナル向けコーディングエージェントです。自然言語でコードを検索し、ファイルを編集し、プロジェクトのテストを実行できます。編集の取り消しとセッションのローカル保存にも対応しています。

## 全画面ターミナル

対話モードは全画面で起動します。幅が 110 列以上になると右側にセッション一覧を表示し、狭い画面では自動的に隠します。Page Up / Page Down で履歴をスクロールし、F10 で終了します。`original --classic` で従来の行単位インターフェースを起動できます。入力欄は会話の末尾に表示されます。

## ログインと実行許可

`/login` では上下キーで選び、Enter で確定、Esc でキャンセルします。モデルの変更は `/model <name>`、思考レベルの変更は Shift+Tab または `/settings` です。

起動したディレクトリを使用します。`original --cwd <path>` で指定もできます。シェル実行時にはコマンドと作業ディレクトリを確認し、`y` または `yes` でその一回だけ許可します。空入力は拒否です。`--allow-shell` は事前許可を与えるため、信頼できる環境で使用してください。

## 記録・使用量・取り消し

`/usage` は現在のセッションのトークン使用量を表示します。`/history` で記録一覧、`/history ID` で内容を確認できます。サイドバーも記録の索引です。`/export` は現在の記録を Markdown に出力します。`--no-record` で新しい記録の保存を無効にできます。

`/diff` と `/undo` は内蔵ファイルツールによる編集を対象にします。シェルから変更したファイルの復元は別途確認してください。

## Office・OCR とトラブル対処

DOCX/PPTX の処理には Python 3 と `python-docx`、`python-pptx` が必要です。画像の文字認識には PATH 上の Tesseract と `chi_sim`、`eng` の言語データが必要です。現在の OCR 設定は簡体字中国語と英語です。

画面描画に問題がある場合は F10 で終了し、`original --classic` を試してください。ツールの詳細を確認するには `--show-tools`、モデルが返す思考情報を表示するには `--show-thinking` を指定します。

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
