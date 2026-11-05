# DeepSeek 工具格式与窗口宽度

用户截图返回 tools[0] unknown variant custom。vendored SDK 的 CustomTool 原先使用内部 type 标签；改为 untagged 自定义工具，保留内置服务端工具标签。新增应用层序列化回归测试。Windows 通过 GetConsoleScreenBufferInfo 的 srWindow 读取可见列数，不再减一列。输入中缩放尚不实时刷新；真实终端渲染与真实 API 调用尚未验证。
