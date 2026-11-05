# 请求地址与全宽输入框修复

本机配置（未输出密钥）为 Anthropic 协议配 https://api.deepseek.com。SDK 会追加 /v1/messages，缺少 DeepSeek Anthropic 兼容的 /anthropic 前缀。构建后端时对已知 DeepSeek 根地址进行规范化，并去除重复 /v1；未更改用户密钥或配置文件。官方端点参考 https://api-docs.deepseek.com/quick_start/pricing/ 。真实模型请求未发送。

上下横线改为每次进入输入时读取终端列数，预留一列防止自动折行；底部标签左侧填充横线。当前输入过程中缩放窗口需下一次输入才重新计算宽度。测试覆盖 40/80/120 列与端点规范化，总计 7 项通过。
