# 2026-11-05 无 Rust 终端安装路径

## 目标

完善「无需本机 Rust」的预编译安装：一条终端命令下载 GitHub Release、校验 SHA-256、安装到约定目录并处理 PATH。提交由仓库所有者完成；本轮未 git commit / push。

## 用法（与 README 一致）

Windows PowerShell：

```powershell
irm https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.ps1 | iex
# 固定版本：
irm https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.ps1 | iex -Version v0.3.3
```

本地仓库调试：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\install.ps1 -Version v0.3.3
```

Linux x64 / macOS：

```sh
curl -fsSL https://raw.githubusercontent.com/haoranyoung09-sys/original-agent/main/scripts/install.sh | sh
sh install.sh v0.3.3   # 下载脚本后固定版本
```

安装位置：

| 平台 | 二进制 | PATH |
| --- | --- | --- |
| Windows x64 | `%USERPROFILE%\.original-agent\bin\original.exe` | 自动写入用户 PATH |
| Linux/macOS | `~/.local/bin/original` | 若未在 PATH 则打印 `export` 提示 |

## 测试钩子（不访问 GitHub）

| 变量 / 参数 | 作用 |
| --- | --- |
| `-BaseUrl` / `ORIGINAL_INSTALL_BASE` | HTTPS 前缀，或含 asset+`.sha256` 的本地目录 |
| `-BinDir` / `ORIGINAL_INSTALL_BIN` | 安装目录覆盖 |
| `-NoPathUpdate` / `ORIGINAL_INSTALL_NO_PATH=1` | 跳过 PATH 修改（Windows）或 PATH 提示（Unix） |

`scripts/test-installer.ps1` / `scripts/test-installer.sh` 用本地假 Release 覆盖：成功安装、校验失败、版本非法、传输失败回退等。

## 维护者发布

推送与 `Cargo.toml` 版本一致的标签（如 `v0.3.3`）。`.github/workflows/release.yml` 构建四平台并上传 `*.sha256`。首次 Release 完成前远程 `irm`/`curl|sh` 会失败，不会覆盖已有安装。

## 限制

- 需要能访问 GitHub Releases（或自备 BaseUrl 镜像）。
- 仅校验 SHA-256，无独立代码签名 / Authenticode。
- Linux 构建基于 Ubuntu 22.04 glibc；更旧发行版可能无法运行。
- Windows 仅 x64；Unix 覆盖 Linux x64 与 macOS Intel/Apple Silicon。
- 真实远程下载仍依赖网络与已发布的 Release；CI/本机 fixture 测试不替代端到端发布验证。

## 本机验证结果

2026-11-05（Asia/Shanghai）：scripts/test-installer.ps1 **8 passed**（含本地假 Release 成功安装、SHA 不匹配、非法版本、curl 回退诊断）。本机无 bash，未跑 	est-installer.sh（已加入 CI ubuntu/macOS job）。未执行真实 GitHub Release 下载（API 403 / 需已发布附件）。未 git commit / push。
