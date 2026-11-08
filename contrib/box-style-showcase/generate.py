#!/usr/bin/env python3
"""Build the dependency-free, reproducible EPUB styling showcase."""
from pathlib import Path
from zipfile import ZipFile, ZipInfo, ZIP_DEFLATED, ZIP_STORED
from xml.etree import ElementTree

ROOT = Path(__file__).resolve().parent
OUTPUT = ROOT.parent / 'box-style-showcase.epub'
CSS = '''body {color:#222;} h1 {font-size:1.6em; text-align:left;}
h2 {font-size:1.15em;} p {text-indent:0; text-align:left; margin:0.7em 0;}
.demo {padding:0.7em; margin:0.8em 0;} .small {font-size:0.8em;}
'''
CHAPTERS = [
('rules', 'Horizontal rules', '''<h1>Horizontal rules</h1>
<p>Legacy HTML attributes: width, size, and align. Rules should get thicker as you read down this page.</p>
<p>Full width, default thickness</p><hr/>
<p>Left: 40%, size 2</p><hr align="left" width="40%" size="2"/>
<p>Center: 60%, size 6</p><hr align="center" width="60%" size="6"/>
<p>Right: 80%, size 12</p><hr align="right" width="80%" size="12"/>
<p>Fixed width: 160 CSS pixels, size 8</p><hr align="left" width="160" size="8"/>
<p>CSS overrides legacy dimensions: 50% wide, 4 pixels high</p>
<hr width="90%" size="20" style="width:50%;height:4px;background-color:#888"/>
<p>Dotted rule using a top border</p><hr style="height:0;border:0;border-top:3px dotted #333;width:75%"/>
'''),
('borders', 'Borders on every side', '''<h1>Borders on every side</h1>
<div class="demo" style="border:2px solid #333">A thin solid frame with padding.</div>
<div class="demo" style="border:5px dotted #555">A dotted frame, five CSS pixels thick.</div>
<div class="demo" style="border:3px dashed #333">Dashed borders are also supported.</div>
<div class="demo" style="border-top:2px solid; border-right:4px dotted; border-bottom:6px solid; border-left:8px dotted">Four independent sides: top 2, right 4, bottom 6, left 8 CSS pixels.</div>
<div class="demo" style="border-style:solid dotted; border-width:2px 4px 6px 8px; border-color:#222 #888 #444 #666">Four-value width and color shorthands; two-value style shorthand.</div>
<div class="demo" style="border:4px double #333">Unsupported styles such as double fall back to solid.</div>
<div class="demo" style="border:4px solid; border-right:none; border-left:none">Only top and bottom remain.</div>
'''),
('backgrounds', 'Background colors', '''<h1>Background colors</h1>
<div class="demo" style="background-color:#ddd; border:2px solid #555">A light gray background fills the box and its padding.</div>
<div class="demo" style="background:#444; color:white">White text on a dark background.</div>
<div class="demo" style="background:#ddd; border-left:6px solid #555"><p>A nested box follows.</p>
<div class="demo" style="background:#aaa;border:2px dotted #333">The child background paints above the parent background.</div><p>Back to the parent color.</p></div>
<p>Inline <span style="background-color:#bbb">highlighted words and spaces</span> followed by plain text.</p>
<div class="demo" style="background-color:#dbeafe">Pale blue. Color is converted to gray on monochrome displays.</div>
<div class="demo" style="background-color:#fef3c7">Pale yellow.</div>
'''),
('pagination', 'Across page breaks', '<h1>Across page breaks</h1><p>This long box checks page navigation, continued backgrounds, and side borders. The top edge appears only at the start; the bottom edge appears only at the end.</p><div class="demo" style="background:#eee;border:3px solid #555">' + ''.join(f'<p>Paragraph {i}. The background should stay behind the text on every page. Side borders continue alongside the text. Turn forward and backward to verify that no page repeats or disappears.</p>' for i in range(1, 19)) + '</div><p>End of the bordered box. This paragraph has a plain background.</p>'),
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
files['OEBPS/content.opf'] = f'''<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="book-id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="book-id">urn:plato:box-style-showcase:1</dc:identifier><dc:title>Plato — Rules, Borders &amp; Backgrounds</dc:title><dc:creator>Plato rendering tests</dc:creator><dc:language>en</dc:language></metadata><manifest>{manifest}</manifest><spine toc="ncx">{spine}</spine></package>'''
files['OEBPS/toc.ncx'] = f'''<?xml version="1.0"?><ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:uid" content="urn:plato:box-style-showcase:1"/><meta name="dtb:depth" content="1"/><meta name="dtb:totalPageCount" content="0"/><meta name="dtb:maxPageNumber" content="0"/></head><docTitle><text>Rules, Borders and Backgrounds</text></docTitle><navMap>{nav}</navMap></ncx>'''
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
