"""Regenerate the synthetic mixed PDF using Python 3 and Poppler's pdftoppm."""
from pathlib import Path
import subprocess
import tempfile
import zlib


def stream(content, extra=b""):
    return b"<< /Length " + str(len(content)).encode() + b" " + extra + b">>\nstream\n" + content + b"\nendstream"


def pdf(image=None):
    text = lambda value: stream(b"BT /F1 24 Tf 60 700 Td (" + value + b") Tj ET")
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> /XObject << /Scan 8 0 R >> >> /Contents 7 0 R >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
        text(b"DIGITAL PAGE EVIDENCE"),
        stream(b"q 612 0 0 792 0 0 cm /Scan Do Q") if image else text(b"SCANNED PAGE EVIDENCE"),
        stream(zlib.compress(image[2]), b"/Type /XObject /Subtype /Image /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Width " + image[0] + b" /Height " + image[1] + b" ") if image else b"null",
    ]
    result = b"%PDF-1.4\n"
    offsets = [0]
    for number, body in enumerate(objects, 1):
        offsets.append(len(result))
        result += str(number).encode() + b" 0 obj\n" + body + b"\nendobj\n"
    xref = len(result)
    result += b"xref\n0 9\n0000000000 65535 f \n"
    result += b"".join(f"{offset:010d} 00000 n \n".encode() for offset in offsets[1:])
    return result + b"trailer\n<< /Size 9 /Root 1 0 R >>\nstartxref\n" + str(xref).encode() + b"\n%%EOF\n"


with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    (root / "seed.pdf").write_bytes(pdf())
    subprocess.run(["pdftoppm", "-f", "2", "-l", "2", "-singlefile", "-r", "144", "-gray", str(root / "seed.pdf"), str(root / "scan")], check=True, timeout=30)
    magic, size, maximum, pixels = (root / "scan.pgm").read_bytes().split(b"\n", 3)
    assert magic == b"P5" and maximum == b"255"
    width, height = size.split()
    assert len(pixels) == int(width) * int(height)
    Path(__file__).with_name("mixed-text-scan.pdf").write_bytes(pdf((width, height, pixels)))
