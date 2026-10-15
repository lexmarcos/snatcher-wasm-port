// The pt-BR page strings must cover exactly the keys of the English table and use the same {variables}.
import { expect, test } from "bun:test";
import { readFileSync } from "fs";
import { join } from "path";
import { UI_EN, T, setUiDict, parseLang } from "../src/i18n";

const ui = JSON.parse(readFileSync(join(import.meta.dir, "../../i18n/pt-BR/ui.json"), "utf8")).strings as Record<string, string>;
const vars = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");

test("ui.json has the same keys as the English table", () => {
  expect(Object.keys(ui).sort()).toEqual(Object.keys(UI_EN).sort());
});
test("every pt-BR string keeps the variables of the English one and is not empty", () => {
  for (const [k, v] of Object.entries(UI_EN)) {
    expect(ui[k]?.length, k).toBeGreaterThan(0);
    expect(vars(ui[k]), k).toBe(vars(v));
  }
});
test("T falls back to English for a missing key and substitutes variables", () => {
  setUiDict("pt-BR", { "msg.ready": "pronto" });
  expect(T("msg.ready")).toBe("pronto");
  expect(T("msg.loading")).toBe(UI_EN["msg.loading"]);
  expect(T("msg.files", { n: 3 })).toBe("3 game files loaded");
  setUiDict("en", null);
  expect(T("msg.ready")).toBe("ready");
});
test("language codes", () => {
  expect(parseLang("pt-BR")).toBe("pt-BR");
  expect(parseLang("pt_br")).toBe("pt-BR");
  expect(parseLang("en-US")).toBe("en");
  expect(parseLang("xx")).toBeNull();
});
