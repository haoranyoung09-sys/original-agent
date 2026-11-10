# Office 和图片工具 / Office and image tools

现有 read 和 write 工具按扩展名处理 DOCX 和 PPTX。写入时以 "# 标题" 创建 Word 一级标题或开始新幻灯片，其余各行进入正文。读取支持段落、幻灯片文本和表格。图片使用 read 提取简体中文和英文。转换在本地运行，工具结果可进入模型对话。
Install Python 3.9+ and run `python -m pip install "python-docx>=1.1" python-pptx`. For image OCR install Tesseract on PATH with chi_sim and eng trained data. Supported images: PNG, JPEG, BMP and TIFF. Writing creates a new document and replaces any existing layout.
