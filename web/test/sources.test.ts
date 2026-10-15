// bun test web/test/sources.test.ts – recognising a picked folder (webkitRelativePath layouts).
import { expect, test } from "bun:test";
import { pickedSource } from "../src/sources";

function f(path: string, content = "x"): File {
  const file = new File([content], path.split("/").pop()!);
  Object.defineProperty(file, "webkitRelativePath", { value: path });
  return file;
}
const names = (s: ReturnType<typeof pickedSource>) => s.gameFiles().map((g) => g.name).sort();

const tree = (root: string) => [
  "raw/files/SP01.BIN", "raw/files/ABS.TXT", "raw/files/PCMLD_01.BIN", "raw/header.bin", "raw/ip.bin", "raw/sp.bin", "raw/filelist.tsv",
  "audio/pcm.json", "audio/fm.json", "audio/web/pcm.json", "audio/web/fm.json", "audio/web/fm/sfx_01.ogg", "audio/web/voice/v0036.ogg",
  "web/music/track05.ogg",
].map((p) => f(`${root}/${p}`, p));

test("whole assets folder", async () => {
  const s = pickedSource(tree("assets"));
  expect(names(s)).toEqual(["ABS.TXT", "SP01.BIN"]);
  expect(await s.boot("HEADER.BIN")).not.toBeNull();
  expect(await s.boot("IP.BIN")).not.toBeNull();
  expect(new TextDecoder().decode((await s.audio("audio/web/pcm.json"))!)).toBe("audio/web/pcm.json"); // not the raw audio/pcm.json
  expect(new TextDecoder().decode((await s.audio("web/music/track05.ogg"))!)).toBe("web/music/track05.ogg");
  expect(await s.audio("web/music/track09.ogg")).toBeNull();
});

test("only assets/raw/files (no audio, no boot files)", async () => {
  const s = pickedSource(tree("assets").filter((x) => x.webkitRelativePath.includes("/raw/files/")).map((x) => f("files/" + x.name)));
  expect(names(s)).toEqual(["ABS.TXT", "SP01.BIN"]);
  expect(await s.boot("HEADER.BIN")).toBeNull();
  expect(await s.audio("audio/web/pcm.json")).toBeNull();
});

test("audio/web picked alone", async () => {
  const s = pickedSource(tree("assets").filter((x) => x.webkitRelativePath.includes("/audio/web/")).map((x) => f(x.webkitRelativePath.replace("assets/audio/", ""), x.webkitRelativePath)));
  expect(new TextDecoder().decode((await s.audio("audio/web/fm.json"))!)).toBe("assets/audio/web/fm.json");
  expect(await s.audio("audio/web/fm/sfx_01.ogg")).not.toBeNull();
  expect(names(s)).toEqual([]);
});

test("flat selection (no directory info)", async () => {
  const s = pickedSource([new File(["a"], "SP01.BIN"), new File(["b"], "header.bin"), new File(["c"], "pcm.json"), new File(["d"], "track05.ogg")]);
  expect(names(s)).toEqual(["SP01.BIN"]);
  expect(await s.audio("audio/web/pcm.json")).not.toBeNull();
  expect(await s.audio("web/music/track05.ogg")).not.toBeNull();
});
