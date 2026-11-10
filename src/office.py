"""Version 1 local Office worker."""
import contextlib
import io
import json
import sys
from pathlib import Path
from zipfile import ZipFile


def convert(req):
    path = Path(req["path"])
    if req["version"] != 1 or req["operation"] not in ("read", "write") or path.suffix.lower() not in (".docx", ".pptx"):
        raise ValueError("Unsupported Office request")
    writing = req["operation"] == "write"
    if not writing:
        with ZipFile(path) as archive:
            if sum(item.file_size for item in archive.infolist()) > 128 * 1024 * 1024:
                raise ValueError("Expanded document exceeds 128 MiB")
    lines = req["content"].splitlines() if writing else []
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
        return output.getvalue().hex()


def main():
    try:
        req = json.load(sys.stdin)
        text = io.StringIO()
        with contextlib.redirect_stdout(text):
            binary = convert(req)
        response = {"version": 1, "result": binary if binary is not None else text.getvalue()}
        code = 0
    except ImportError as exc:
        response = {"version": 1, "error": {"kind": "dependency", "message": f"{exc}. Install in this interpreter: python -m pip install python-docx>=1.1.2 python-pptx"}}
        code = 2
    except Exception as exc:
        response = {"version": 1, "error": {"kind": "conversion", "message": str(exc)}}
        code = 1
    print(json.dumps(response))
    return code


if __name__ == "__main__":
    sys.exit(main())
