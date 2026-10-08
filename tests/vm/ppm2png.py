"""Convert a binary PPM (QEMU screendump) to PNG with the standard library."""
import struct
import sys
import zlib

src, dst = sys.argv[1], sys.argv[2]
data = open(src, 'rb').read()
fields, pos = [], 0
while len(fields) < 4:
    while data[pos:pos + 1].isspace():
        pos += 1
    if data[pos:pos + 1] == b'#':
        pos = data.index(b'\n', pos)
        continue
    end = pos
    while not data[end:end + 1].isspace():
        end += 1
    fields.append(data[pos:end])
    pos = end
assert fields[0] == b'P6', 'not a binary PPM'
width, height = int(fields[1]), int(fields[2])
pixels = data[pos + 1:]
rows = b''.join(b'\x00' + pixels[y * width * 3:(y + 1) * width * 3] for y in range(height))

def chunk(kind: bytes, body: bytes) -> bytes:
    return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body) & 0xFFFFFFFF)

png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
png += chunk(b'IDAT', zlib.compress(rows, 6)) + chunk(b'IEND', b'')
open(dst, 'wb').write(png)
