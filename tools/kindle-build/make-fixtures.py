"""Generate tiny, local test documents for the Kindle rendering smoke test."""
from pathlib import Path
import sys
import zipfile

out = Path(sys.argv[1])
page_count = 3 if "--reader" in sys.argv[2:] else 1
out.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(out / "smoke.epub", "w", zipfile.ZIP_DEFLATED) as book:
    book.writestr("mimetype", "application/epub+zip", zipfile.ZIP_STORED)
    book.writestr(
        "META-INF/container.xml",
        '<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">'
        '<rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/>'
        "</rootfiles></container>",
    )
    manifest = "".join(
        f'<item id="page-{n}" href="page-{n}.xhtml" media-type="application/xhtml+xml"/>'
        for n in range(1, page_count + 1)
    )
    spine = "".join(f'<itemref idref="page-{n}"/>' for n in range(1, page_count + 1))
    book.writestr(
        "book.opf",
        '<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="id">'
        '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">'
        '<dc:title>Plato EPUB Test</dc:title><dc:identifier id="id">plato-render-smoke</dc:identifier>'
        f'<dc:language>en</dc:language></metadata><manifest>{manifest}</manifest>'
        f"<spine>{spine}</spine></package>",
    )
    for n in range(1, page_count + 1):
        book.writestr(
            f"page-{n}.xhtml",
            '<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Plato</title></head>'
            f'<body><h1>Plato EPUB Test</h1><p>Chapter {n} of {page_count}</p>'
            '<p>The quick brown fox jumps over the lazy dog.</p></body></html>',
        )

kids = " ".join(f"{4 + 2 * n} 0 R" for n in range(page_count))
objects = [
    b"<< /Type /Catalog /Pages 2 0 R >>",
    f"<< /Type /Pages /Kids [{kids}] /Count {page_count} >>".encode(),
    b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
]
for n in range(1, page_count + 1):
    stream = f"BT /F1 18 Tf 24 160 Td (Plato PDF Test - page {n} of {page_count}) Tj ET\n".encode()
    contents = 3 + 2 * n
    objects.extend([
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 280 200] "
        + f"/Resources << /Font << /F1 3 0 R >> >> /Contents {contents} 0 R >>".encode(),
        f"<< /Length {len(stream)} >>\nstream\n".encode() + stream + b"endstream",
    ])
pdf = bytearray(b"%PDF-1.4\n")
offsets = []
for number, obj in enumerate(objects, 1):
    offsets.append(len(pdf))
    pdf.extend(f"{number} 0 obj\n".encode() + obj + b"\nendobj\n")
xref = len(pdf)
pdf.extend(f"xref\n0 {len(offsets) + 1}\n0000000000 65535 f \n".encode())
for offset in offsets:
    pdf.extend(f"{offset:010} 00000 n \n".encode())
pdf.extend(f"trailer\n<< /Root 1 0 R /Size {len(objects) + 1} >>\nstartxref\n{xref}\n%%EOF\n".encode())
(out / "smoke.pdf").write_bytes(pdf)
