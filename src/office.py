"""Embedded Office conversion helper. Input and output travel over UTF-8 or binary pipes."""
import io
import sys
from pathlib import Path
from zipfile import ZipFile

mode, filename = sys.argv[1:]
path = Path(filename)
writing = mode == "write"
if not writing:
    with ZipFile(path) as archive:
        if sum(item.file_size for item in archive.infolist()) > 128 * 1024 * 1024:
            sys.exit("Expanded document exceeds 128 MiB")
lines = sys.stdin.buffer.read().decode("utf-8").splitlines() if writing else []
if path.suffix.lower() == ".docx":
    from docx import Document
    from docx.table import Table
    document = Document(None if writing else path)
    if writing:
        for line in lines:
            if line.startswith("# "):
                document.add_heading(line[2:], level=1)
            else:
                document.add_paragraph(line)
    else:
        for block in document.iter_inner_content():
            if isinstance(block, Table):
                for row in block.rows:
                    print("\t".join(cell.text for cell in row.cells))
            else:
                print(block.text)
else:
    from pptx import Presentation
    document = Presentation(None if writing else path)
    if writing:
        slide = None
        for line in lines or [""]:
            if slide is None or line.startswith("# "):
                slide = document.slides.add_slide(document.slide_layouts[1])
                slide.shapes.title.text = line.removeprefix("# ")
                body = slide.placeholders[1].text_frame
                continue
            paragraph = body.add_paragraph() if body.text else body.paragraphs[0]
            paragraph.text = line
    else:
        for index, slide in enumerate(document.slides, start=1):
            print(f"[Slide {index}]")
            for shape in slide.shapes:
                if shape.has_text_frame:
                    print(shape.text)
                elif shape.has_table:
                    for row in shape.table.rows:
                        print("\t".join(cell.text for cell in row.cells))
if writing:
    output = io.BytesIO()
    document.save(output)
    sys.stdout.buffer.write(output.getvalue())
