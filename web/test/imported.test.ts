// bun test web/test/imported.test.ts – choosing the cue and its images among the picked/dropped files (the cue itself is parsed by the
// Rust wasm module when web/pkg is built; the tests skip the wasm part otherwise).
import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "fs";
import { join } from "path";
import { resolvePick } from "../src/imported";

const pkgDir = join(import.meta.dir, "../pkg");
let cueFiles: (t: string) => string[];
if (existsSync(join(pkgDir, "snengine_bg.wasm"))) {
  const pkg: any = await import(join(pkgDir, "snengine.js"));
  pkg.initSync({ module: readFileSync(join(pkgDir, "snengine_bg.wasm")) });
  cueFiles = (t) => JSON.parse(pkg.cue_files(t));
} else {
  cueFiles = (t) => [...t.matchAll(/^FILE\s+"([^"]+)"/gim)].map((m) => m[1]);
}
const f = (name: string, content = "x") => new File([content], name);
const cue1 = 'FILE "snatcher.bin" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n';
const cueN = (n: number) => Array.from({ length: n }, (_, i) => `FILE "Snatcher (USA) (Track ${String(i + 1).padStart(2, "0")}).bin" BINARY\r\n  TRACK ${String(i + 1).padStart(2, "0")} ${i ? "AUDIO" : "MODE1/2352"}\r\n    INDEX 01 00:00:00\r\n`).join("");

test("single bin", async () => {
  const r: any = await resolvePick([f("snatcher.bin"), f("snatcher.cue", cue1), f("notes.txt")], cueFiles);
  expect(r.cueName).toBe("snatcher.cue");
  expect(r.files.map((x: File) => x.name)).toEqual(["snatcher.bin"]);
});
test("multi bin in any pick order, names with spaces and different case", async () => {
  const names = Array.from({ length: 21 }, (_, i) => `snatcher (usa) (track ${String(i + 1).padStart(2, "0")}).BIN`);
  const picked = [f("Snatcher (USA).cue", cueN(21)), ...names.slice().reverse().map((n) => f(n))];
  const r: any = await resolvePick(picked, cueFiles);
  expect(r.files.map((x: File) => x.name)).toEqual(names);
});
test("a renamed single image is accepted when it is the only one", async () => {
  const r: any = await resolvePick([f("disc.cue", cue1), f("other.bin")], cueFiles);
  expect(r.files[0].name).toBe("other.bin");
  expect(r.notes.length).toBe(1);
  const r2: any = await resolvePick([f("disc.cue", cue1), f("a.bin"), f("b.bin")], cueFiles);
  expect(r2.code).toBe("nobin");
});
test("errors: no cue, two cues, missing bins, bad cue, huge cue", async () => {
  expect(((await resolvePick([f("a.bin")], cueFiles)) as any).code).toBe("nocue");
  expect(((await resolvePick([f("a.cue", cue1), f("b.cue", cue1)], cueFiles)) as any).code).toBe("manycues");
  const m: any = await resolvePick([f("Snatcher (USA).cue", cueN(3)), f("Snatcher (USA) (Track 01).bin"), f("Snatcher (USA) (Track 03).bin")], cueFiles);
  expect(m.code).toBe("nobin");
  expect(m.detail).toBe("Snatcher (USA) (Track 02).bin");
  expect(((await resolvePick([f("a.cue", "no tracks here"), f("snatcher.bin")], cueFiles)) as any).code).toBe("badcue");
  expect(((await resolvePick([new File([new Uint8Array(2 << 20)], "big.cue")], cueFiles)) as any).code).toBe("toobig");
});
test("a path in the cue file name is ignored", async () => {
  const r: any = await resolvePick([f("x.cue", 'FILE "C:\\dumps\\snatcher.bin" BINARY\n TRACK 01 MODE1/2352\n  INDEX 01 00:00:00\n'), f("snatcher.bin")], cueFiles);
  expect(r.files[0].name).toBe("snatcher.bin");
});
