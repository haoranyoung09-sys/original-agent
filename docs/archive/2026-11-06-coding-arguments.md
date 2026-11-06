# 2026-11-06 Coding 工具参数安全

以 Documents/GitHub/original-agent 的当前代码为基准，保留现有 glob、grep、multi_edit、verify 等功能。

修复 OpenAI 工具分发缺失参数默认空字符串的问题：write 必须显式提供 content，edit 和 multi_edit 必须显式提供 new_str；必需文本参数拒绝 null、数字及缺失；read offset/limit 拒绝非整数和无法转换为 usize 的值。显式空字符串仍合法。

新增真实文件回归测试，检查四种非法修改不会改变原文件，并验证显式清空仍可使用。cargo test --offline --locked：33 项全部通过。未发送真实 API 请求，未提交或推送。
