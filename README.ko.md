# original-agent

[简体中文](README.md) · [English](README.en.md) · [日本語](README.ja.md) · **한국어**

Original은 Rust로 만든 터미널 코딩 에이전트입니다. 자연어로 코드를 검색하고 파일을 수정한 뒤 프로젝트 테스트를 실행할 수 있습니다. 파일 편집을 되돌리거나 로컬에 저장한 대화 기록을 열 수도 있습니다.

## 전체 화면 터미널

대화 모드는 전체 화면으로 시작합니다. 너비가 110열 이상이면 오른쪽에 세션 목록을 표시하고, 좁은 화면에서는 자동으로 숨깁니다. Page Up / Page Down으로 기록을 스크롤하고 F10으로 종료합니다. 기존 방식은 `original --classic`으로 실행할 수 있습니다. 입력 프롬프트는 대화 끝에 표시됩니다.

## 로그인과 실행 권한

`/login`에서 위아래 방향키로 선택하고 Enter로 확인합니다. Esc는 취소합니다. 모델은 `/model <name>`, 사고 수준은 Shift+Tab 또는 `/settings`에서 변경합니다.

프로그램을 실행한 디렉터리를 작업 공간으로 사용하며 `original --cwd <path>`로 지정할 수도 있습니다. 셸 명령 실행 전에는 명령과 디렉터리를 표시합니다. `y` 또는 `yes`를 입력하면 이번 실행만 허용하고, 빈 응답은 거부합니다. `--allow-shell`은 사전 허용이므로 신뢰하는 환경에서 사용하세요.

## 기록·사용량·실행 취소

`/usage`는 현재 세션의 토큰 사용량을 보여 줍니다. `/history`로 목록을, `/history ID`로 내용을 확인합니다. 사이드바 역시 기록 색인입니다. `/export`로 현재 기록을 Markdown으로 내보내고 `--no-record`로 새 기록 저장을 끌 수 있습니다.

`/diff`와 `/undo`는 내장 파일 도구의 편집을 대상으로 합니다. 셸로 수정한 파일의 복원은 별도로 확인해야 합니다.

## Office·OCR 및 문제 해결

DOCX/PPTX 처리는 Python 3와 `python-docx`, `python-pptx`가 필요합니다. 이미지 OCR에는 PATH에 등록된 Tesseract 및 `chi_sim`, `eng` 언어 데이터가 필요합니다. 현재 OCR 설정은 중국어 간체와 영어입니다.

화면 문제가 발생하면 F10으로 종료한 후 `original --classic`을 사용해 보세요. 도구 상세 출력은 `--show-tools`, 모델이 제공하는 사고 내용은 `--show-thinking`으로 표시합니다.

## v0.3.10 소스 설치

태그 `v0.3.10`에는 반복되는 검증 오류 로그를 줄이는 변경과 터미널 오른쪽 끝에서 입력 테두리가 줄바꿈되는 문제의 수정이 포함되어 있습니다. 이 태그의 Cargo 버전과 프로그램에 표시되는 버전은 `0.3.9`입니다. 릴리스 빌드에는 태그와 패키지 버전이 일치해야 하므로 이 태그는 Rust로 직접 설치하세요.

```sh
cargo install --locked --git https://github.com/haoranyoung09-sys/original-agent --tag v0.3.10 original-agent
original --version
```

## 설치

Windows (PowerShell)

```powershell
iex (irm https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.ps1)
```

Linux / macOS

```sh
curl -fsSL https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.sh | sh
```

소스에서 설치하려면 저장소 루트에서 `cargo install --locked --path .` 명령을 실행하세요.

## 첫 세션 시작하기

프로젝트 디렉터리에서 `original`을 실행한 뒤 `/login`을 입력하세요. 방향키와 Enter로 API 연결 또는 구독을 선택합니다. API 연결에서는 Anthropic, OpenAI, DeepSeek 또는 호환 서비스를 고른 뒤 API 키를 입력합니다. 로그인한 후 `/model <name>`으로 모델을 바꿀 수 있습니다.

구독을 선택하면 설치된 Codex, Claude Code 또는 Gemini CLI가 열립니다. 로그인과 사용 한도는 해당 클라이언트에서 관리합니다.

| 명령 | 기능 |
|---|---|
| `/settings` | 설정 확인 및 사고 수준 변경 |
| `/usage` | 현재 세션의 토큰 사용량 확인 |
| `/history` | 프로젝트별 세션 기록 조회 |
| `/diff` · `/undo` | 파일 도구의 변경 내역 확인 및 실행 취소 |
| `/help` | 명령 목록 확인 |

신뢰하는 프로젝트에서는 `original --auto-verify`로 편집 후 테스트를 실행할 수 있습니다. `--allow-shell`을 켜면 에이전트가 사용자 권한으로 임의의 셸 명령을 실행할 수 있습니다.

## 구성

Rust 본체는 Anthropic 호환 API에 커뮤니티 SDK인 claudius를 사용하고, OpenAI 호환 API에는 reqwest를 사용합니다. Word와 PowerPoint 파일을 읽고 쓰려면 Python이, 이미지에서 글자를 추출하려면 Tesseract가 필요합니다.

[Office / OCR](docs/archive/office-tools.md) · [Security](SECURITY.md) · [MIT License](LICENSE)
