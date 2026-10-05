import { describe, expect, it } from "vitest";
import { parseFrame } from "../composables/useCamera";

describe("parseFrame", () => {
  it("splits header and jpeg", () => {
    const header = new TextEncoder().encode(JSON.stringify({ width: 480, height: 640, faces: [] }));
    const jpeg = [0xff, 0xd8, 0xff, 0xd9];
    const buf = new Uint8Array(4 + header.length + jpeg.length);
    new DataView(buf.buffer).setUint32(0, header.length, true);
    buf.set(header, 4);
    buf.set(jpeg, 4 + header.length);

    for (const raw of [buf.buffer, Array.from(buf)]) {
      const f = parseFrame(raw);
      expect(f.header.width).toBe(480);
      expect(Array.from(f.jpeg)).toEqual(jpeg);
    }
  });
});
