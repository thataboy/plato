#!/usr/bin/env python3
"""Build the dependency-free, reproducible EPUB styling showcase."""
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED, ZIP_STORED
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parent
OUTPUT = ROOT.parent / 'table-style-showcase.epub'
CSS = '''body {color:#222;} h1 {font-size:1.5em;text-align:left;} p {text-indent:0;text-align:left;}
table {margin:0.6em 0; border:3px solid #333; background:#eee; width:100%;}
th,td {padding:0.5em; border:1px solid #777; text-align:left;} th {background:#444;color:white;}
.light {background:#ddd;} .dark {background:#aaa;} .white {background:white;}
'''
CHAPTERS = [
('cells', 'Tables and cells', '''<h1>Tables and cells</h1><p>The table has a solid outer border and a light background. Headers have dark backgrounds and white text.</p>
<table><thead><tr><th>Item</th><th>Description</th><th>Status</th></tr></thead><tbody>
<tr><td class="light">One</td><td>A short description.</td><td class="dark">Ready</td></tr>
<tr><td class="light">Two</td><td>This deliberately longer description wraps over several lines. Every cell in this row should fill the same height.</td><td class="dark">Ready</td></tr>
<tr><td class="light">Three</td><td>Transparent cells reveal the table background.</td><td class="white">Done</td></tr>
</tbody></table><p>The outer border should fit the table grid.</p>'''),
('edges', 'Independent cell borders', '''<h1>Independent cell borders</h1><p>Each cell can set its own side widths, styles, colors, and background.</p>
<table><tr><th>Style</th><th>Example</th></tr>
<tr><td>Solid</td><td style="border:4px solid #333;background:#ddd">A solid cell frame.</td></tr>
<tr><td>Dotted</td><td style="border:4px dotted #555;background:#eee">Dotted on every side.</td></tr>
<tr><td>Dashed</td><td style="border:3px dashed #333;background:#bbb">Dashed on every side.</td></tr>
<tr><td>Mixed</td><td style="border-top:2px solid; border-right:4px dotted; border-bottom:6px dashed; border-left:8px solid; background:#ddd">Four independent sides with different widths.</td></tr>
<tr><td>No side edges</td><td style="border-left:none;border-right:none; background:white">Top and bottom only.</td></tr></table>
<p>Adjacent cell borders are separate; border-collapse is not implemented.</p>'''),
('empty', 'Empty cells and spans', '''<h1>Empty cells and spans</h1><p>Empty cells keep their backgrounds and borders. A spanning header covers both columns.</p>
<table><tr><th colspan="2">A header spanning two columns</th></tr>
<tr><td class="dark"></td><td class="light">The empty cell on the left fills this entire row, including all the space beside this longer text.</td></tr>
<tr><td class="light">The empty cell on the right has a dotted border.</td><td style="border:3px dotted #333;background:#bbb"></td></tr>
<tr><td colspan="2" style="background:#ddd;border-bottom:5px solid #333">A data cell spanning both columns.</td></tr></table>
<p>An empty row still has padding and borders:</p><table><tr><td class="light"></td><td class="dark"></td></tr></table>'''),
('pages', 'Table across pages', '<h1>Table across pages</h1><p>A single tall row continues across pages. Both cell backgrounds and side borders should continue; bottom borders belong at the end. Turn forward and backward to check navigation.</p><table><tr><th>Short cell</th><th>Long cell</th></tr><tr><td class="dark">This cell ends early.</td><td class="light">' + '<br/>'.join(f'Line {i}: The cell background and side borders continue with the text.' for i in range(1, 37)) + '</td></tr></table><p>End of the table.</p>'),
]

def xhtml(title, body):
    return f'<?xml version="1.0" encoding="UTF-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title><link rel="stylesheet" type="text/css" href="style.css"/></head><body>{body}</body></html>'

files = {
    'mimetype': 'application/epub+zip',
    'META-INF/container.xml': '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
    'OEBPS/style.css': CSS,
}
manifest = '<item id="css" href="style.css" media-type="text/css"/><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>'
spine = ''
nav = ''
for number, (name, title, body) in enumerate(CHAPTERS, 1):
    files[f'OEBPS/{name}.xhtml'] = xhtml(title, body)
    manifest += f'<item id="{name}" href="{name}.xhtml" media-type="application/xhtml+xml"/>'
    spine += f'<itemref idref="{name}"/>'
    nav += f'<navPoint id="{name}" playOrder="{number}"><navLabel><text>{title}</text></navLabel><content src="{name}.xhtml"/></navPoint>'
files['OEBPS/content.opf'] = f'''<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="book-id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="book-id">urn:plato:table-style-showcase:1</dc:identifier><dc:title>Plato — Table Borders &amp; Backgrounds</dc:title><dc:creator>Plato rendering tests</dc:creator><dc:language>en</dc:language></metadata><manifest>{manifest}</manifest><spine toc="ncx">{spine}</spine></package>'''
files['OEBPS/toc.ncx'] = f'''<?xml version="1.0"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="urn:plato:table-style-showcase:1"/><meta name="dtb:depth" content="1"/><meta name="dtb:totalPageCount" content="0"/><meta name="dtb:maxPageNumber" content="0"/></head><docTitle><text>Table Borders and Backgrounds</text></docTitle><navMap>{nav}</navMap></ncx>'''
with ZipFile(OUTPUT, 'w') as archive:
    for name, content in files.items():
        if name.endswith(('.xml', '.opf', '.ncx', '.xhtml')):
            ElementTree.fromstring(content)
        info = ZipInfo(name, (2026, 11, 6, 0, 0, 0))
        info.compress_type = ZIP_STORED if name == 'mimetype' else ZIP_DEFLATED
        archive.writestr(info, content.encode('utf-8'))
with ZipFile(OUTPUT) as archive:
    assert archive.testzip() is None
    assert archive.infolist()[0].filename == 'mimetype'
    assert archive.infolist()[0].compress_type == ZIP_STORED
print(OUTPUT)
