# original-agent

[简体中文](README.md) · [English](README.en.md) · [日本語](README.ja.md) · **한국어**

Original은 Rust로 만든 터미널 코딩 에이전트입니다. 자연어로 코드를 검색하고 파일을 수정한 뒤 프로젝트 테스트를 실행할 수 있습니다. 파일 편집을 되돌리거나 로컬에 저장한 대화 기록을 열 수도 있습니다.

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
