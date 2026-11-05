# Release v0.3.3 修复

失败运行：https://github.com/haoranyoung09-sys/original-agent/actions/runs/42038814826/job/127283880369

日志明确显示 Ubuntu 在版本检查的 `import os, tomllib` 报 ModuleNotFoundError。其他平台因矩阵 fail-fast 被取消，publish 跳过。修复：在 Python 步骤前配置 actions/setup-python，固定 Python 3.12；设置 fail-fast: false，保留各平台独立诊断。Cargo.toml 与 Cargo.lock 升级为 0.3.3，避免覆盖 v0.3.2 标签。

本地验证：执行版本检查 Python 块，匹配 v0.3.3；cargo test --locked。远端四平台构建需用户提交并推送 v0.3.3 后验证，不能通过重跑旧标签读取新配置。
