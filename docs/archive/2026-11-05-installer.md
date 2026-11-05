# 预编译安装归档

增加 scripts/install.ps1、scripts/install.sh 与 release.yml。Windows x64、Linux x64、macOS x64/arm64 由标签触发构建。所有下载先校验 SHA-256 再安装，Windows 自动维护用户 PATH。源代码安装脚本继续保留。

尚未推送或发布 Release，因此未进行真实远程下载安装。需要 GitHub Actions 完成后进行各平台端到端验证。

验证：PowerShell 解析通过，工作流内两段 Python 编译通过。当前环境无 YAML 解析库及 Bash，未进行 YAML 解析或 Unix 脚本执行。macOS runner 已按 https://github.com/actions/runner-images 核对为 macos-15 与 macos-15-intel。

## PowerShell 空值错误修复

用户截图出现 InvokeMethodOnNull，但未包含内部行号。当前机器的 RuntimeInformation 架构读取正常，无法确认与用户环境相同的根因。改用 Windows 架构环境变量，校验下载的 checksum 非空后再 Trim，并增加 TLS 1.2 与下载失败来源/行号信息。新增 scripts/test-installer.ps1，使用模拟下载测试空校验文件及无效版本，Windows PowerShell 5.1 执行两项通过。测试不安装程序、不修改 PATH。远程脚本获取在本环境发生 TLS 认证失败，未关闭证书校验，未验证实际 Release 下载。
