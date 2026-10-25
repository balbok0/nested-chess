// SPDX-License-Identifier: MIT
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';

const BACKGROUND = [37, 37, 37];
const LIGHT = [238, 232, 215];
const DARK = [123, 143, 116];
const CRC_POLYNOMIAL = 0xedb88320;
const CRC_TABLE = Uint32Array.from({ length: 256 }, (_, value) => {
  for (let bit = 0; bit < 8; bit++) value = value & 1 ? CRC_POLYNOMIAL ^ (value >>> 1) : value >>> 1;
  return value >>> 0;
});
function chunk(type, data) {
  const bytes = Buffer.concat([Buffer.from(type), data]);
  let crc = 0xffffffff;
  for (const byte of bytes) crc = CRC_TABLE[(crc ^ byte) & 255] ^ (crc >>> 8);
  const result = Buffer.alloc(data.length + 12);
  result.writeUInt32BE(data.length); bytes.copy(result, 4); result.writeUInt32BE((crc ^ 0xffffffff) >>> 0, result.length - 4);
  return result;
}
function png(width, height, icon) {
  const pixels = Buffer.alloc(height * (width * 3 + 1));
  const art = icon ? Math.min(width, height) * 0.64 : Math.min(width, height) * 0.18;
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    let color = BACKGROUND;
    const u = (x - (width - art) / 2) / art, v = (y - (height - art) / 2) / art;
    if (u >= 0 && u < 1 && v >= 0 && v < 1) {
      const boardX = u < 0.46 ? u : u >= 0.54 ? u - 0.54 : null;
      const boardY = v < 0.46 ? v : v >= 0.54 ? v - 0.54 : null;
      if (boardX !== null && boardY !== null) color = (Math.floor(boardX / 0.115) + Math.floor(boardY / 0.115)) % 2 ? DARK : LIGHT;
    }
    pixels.set(color, y * (width * 3 + 1) + 1 + x * 3);
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width); header.writeUInt32BE(height, 4); header[8] = 8; header[9] = 2;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', header), chunk('IDAT', deflateSync(pixels)), chunk('IEND', Buffer.alloc(0))]);
}
function generate(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = `${directory}/${entry.name}`;
    if (entry.isDirectory()) generate(path);
    else if (entry.name.endsWith('.png')) {
      const original = readFileSync(path);
      writeFileSync(path, png(original.readUInt32BE(16), original.readUInt32BE(20), /ic_launcher|AppIcon/.test(entry.name)));
    }
  }
}
generate('mobile/android/app/src/main/res');
generate('mobile/ios/App/App/Assets.xcassets');
