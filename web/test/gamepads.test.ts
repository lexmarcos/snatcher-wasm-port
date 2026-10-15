import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "fs";
import { join } from "path";
import {
  assign, CONFIG_LETTERS, CONTROLS, DEFAULT_MAPS, detectFamily, Device, GENESIS_ORDER, layoutFor, PAD, padBits, parseGamepadId, promptFor, readPad, resolveIcon,
  sanitizeMaps, stickDirs, STYLE, type PadLike, type Family,
} from "../src/gamepads";

/** Fake pad: pressed button indices and axis values. */
function pad(id: string, mapping: string, pressed: number[] = [], axes: number[] = [], nButtons = 18, index = 0): PadLike {
  return {
    id, index, connected: true, mapping,
    buttons: Array.from({ length: nButtons }, (_, i) => ({ pressed: pressed.includes(i), value: pressed.includes(i) ? 1 : 0 })),
    axes: axes.length ? axes : [0, 0, 0, 0],
  };
}

describe("family detection from real-world Gamepad.id strings", () => {
  const cases: [string, Family][] = [
    // Chrome / Edge (Linux, Windows, macOS)
    ["Xbox 360 Controller (STANDARD GAMEPAD Vendor: 045e Product: 028e)", "xbox"],
    ["Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)", "xbox"],
    ["Microsoft X-Box One pad (Vendor: 045e Product: 02ea)", "xbox"],
    ["Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 09cc)", "playstation"],
    ["Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 05c4)", "playstation"],
    ["DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)", "playstation"],
    ["DualSense Edge Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0df2)", "playstation"],
    ["Sony Interactive Entertainment Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 09cc)", "playstation"],
    ["Pro Controller (STANDARD GAMEPAD Vendor: 057e Product: 2009)", "switch"],
    ["Nintendo Co., Ltd. Pro Controller (STANDARD GAMEPAD Vendor: 057e Product: 2009)", "switch"],
    ["Joy-Con (L) (Vendor: 057e Product: 2006)", "switch"],
    ["Joy-Con (R) (Vendor: 057e Product: 2007)", "switch"],
    ["Logitech Gamepad F310 (STANDARD GAMEPAD Vendor: 046d Product: c216)", "generic"],
    ["8BitDo Pro 2 (Vendor: 2dc8 Product: 6003)", "generic"],
    // Firefox (Linux ids are zero padded; Windows/macOS are not)
    ["045e-028e-Microsoft X-Box 360 pad", "xbox"],
    ["045e-0b12-Microsoft Xbox Controller", "xbox"],
    ["45e-2ea-Xbox Wireless Controller", "xbox"],
    ["054c-09cc-Sony Interactive Entertainment Wireless Controller", "playstation"],
    ["054c-0ce6-Sony Interactive Entertainment DualSense Wireless Controller", "playstation"],
    ["54c-9cc-Wireless Controller", "playstation"],
    ["054c-0df2-DualSense Edge Wireless Controller", "playstation"],
    ["057e-2009-Nintendo Co., Ltd. Pro Controller", "switch"],
    ["57e-2009-Pro Controller", "switch"],
    ["046d-c216-Logitech Dual Action", "generic"],
    // no ids: name heuristics
    ["xinput", "xbox"],
    ["Wireless Controller", "playstation"],
    ["PLAYSTATION(R)3 Controller", "playstation"],
    ["Nintendo Switch Pro Controller", "switch"],
    ["USB Gamepad", "generic"],
    ["", "generic"],
  ];
  for (const [id, fam] of cases) test(`${fam}: ${id}`, () => expect(detectFamily(id)).toBe(fam));

  test("id parsing", () => {
    expect(parseGamepadId("Xbox 360 Controller (STANDARD GAMEPAD Vendor: 045e Product: 028e)")).toEqual({ vendor: "045e", product: "028e", name: "Xbox 360 Controller" });
    expect(parseGamepadId("54c-9cc-Wireless Controller")).toEqual({ vendor: "054c", product: "09cc", name: "Wireless Controller" });
    expect(parseGamepadId("045E-028E-X").vendor).toBe("045e");
    expect(parseGamepadId("xinput").vendor).toBeNull();
  });
});

describe("stick: radial dead zone", () => {
  test("inside the zone nothing, outside 8 directions", () => {
    expect(stickDirs(0.25, 0.2)).toEqual({ up: false, down: false, left: false, right: false });
    expect(stickDirs(0.36, 0)).toEqual({ up: false, down: false, left: false, right: true });
    expect(stickDirs(0.24, 0.24)).toEqual({ up: false, down: false, left: false, right: false }); // |v| = 0.339
  });
  test("diagonals and the per-axis case the old 0.5 threshold missed", () => {
    expect(stickDirs(0.4, -0.4)).toEqual({ up: true, down: false, left: false, right: true });
    expect(stickDirs(-0.5, 0.5)).toEqual({ up: false, down: true, left: true, right: false });
    expect(stickDirs(0.45, 0)).toEqual({ up: false, down: false, left: false, right: true }); // 0.45 < 0.5 used to be dead
    expect(stickDirs(1, 0.2)).toEqual({ up: false, down: false, left: false, right: true }); // slight Y does not add a diagonal
    expect(stickDirs(0, -0.9).up).toBe(true);
  });
});

describe("layouts", () => {
  test("standard mapping is positional for every family", () => {
    for (const f of ["xbox", "playstation", "switch", "generic"] as Family[]) {
      const st = readPad(pad("x", "standard", [0, 2, 9, 12]), f);
      expect([...st.down].sort()).toEqual(["south", "start", "west"]);
      expect(st.dir.up).toBe(true);
    }
  });
  test("standard: sticks and select", () => {
    const st = readPad(pad("x", "standard", [8], [-1, 0, 0, 0]), "xbox");
    expect(st.dir.left).toBe(true);
    expect(st.down.has("select")).toBe(true);
  });
  test("raw xbox (Firefox/Linux xpad): A=0 B=1 X=2 Y=3, triggers on axes 2 and 5 (rest -1), hat on axes 6/7", () => {
    const axes = [0, 0, 1, 0, 0, -1, 0, 0]; // LT fully pressed, RT at rest
    const st = readPad(pad("045e-028e-Microsoft X-Box 360 pad", "", [0, 7], axes, 11));
    expect(st.family).toBe("xbox");
    expect([...st.down].sort()).toEqual(["l2", "south", "start"]);
    expect(readPad(pad("045e-028e-x", "", [], [0, 0, -1, 0, 0, -1, 1, -1], 11)).dir).toEqual({ up: true, down: false, left: false, right: true });
  });
  test("raw playstation (Linux): cross=0 circle=1 triangle=2 square=3, options=9", () => {
    const st = readPad(pad("054c-09cc-Sony Interactive Entertainment Wireless Controller", "", [0, 1, 2, 3, 9], [], 13));
    expect([...st.down].sort()).toEqual(["east", "north", "south", "start", "west"]);
  });
  test("raw switch (HYPOTHESIS order): B=0 A=1 X=2 Y=3 capture=4, L=5", () => {
    const st = readPad(pad("057e-2009-Nintendo Co., Ltd. Pro Controller", "", [1, 5], [], 14));
    expect([...st.down].sort()).toEqual(["east", "l1"]);
    expect(layoutFor("switch", "standard").buttons.south).toBe(0);
  });
  test("a standard pad of any family ignores the raw tables", () => {
    expect(layoutFor("playstation", "standard")).toBe(layoutFor("xbox", "standard"));
  });
});

describe("Genesis bits and button maps", () => {
  const up = (st: PadLike) => padBits(readPad(st), DEFAULT_MAPS[detectFamily(st.id)]);
  const XB = "Xbox 360 Controller (STANDARD GAMEPAD Vendor: 045e Product: 028e)";
  const PS = "DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)";
  const SW = "Pro Controller (STANDARD GAMEPAD Vendor: 057e Product: 2009)";
  test("Xbox: A=confirm(C) B=cancel(B) X=A, Menu=START", () => {
    expect(up(pad(XB, "standard", [0]))).toBe(PAD.C);
    expect(up(pad(XB, "standard", [1]))).toBe(PAD.B);
    expect(up(pad(XB, "standard", [2]))).toBe(PAD.A);
    expect(up(pad(XB, "standard", [9]))).toBe(PAD.START);
    expect(up(pad(XB, "standard", [3]))).toBe(0); // Y unassigned
  });
  test("PlayStation: cross=confirm circle=back square=A options=START", () => {
    expect(up(pad(PS, "standard", [0]))).toBe(PAD.C);
    expect(up(pad(PS, "standard", [1]))).toBe(PAD.B);
    expect(up(pad(PS, "standard", [2]))).toBe(PAD.A);
    expect(up(pad(PS, "standard", [9]))).toBe(PAD.START);
  });
  test("Switch: A (right, index 1) = confirm, B (bottom, index 0) = back, Y (left) = A, + = START", () => {
    expect(up(pad(SW, "standard", [1]))).toBe(PAD.C);
    expect(up(pad(SW, "standard", [0]))).toBe(PAD.B);
    expect(up(pad(SW, "standard", [2]))).toBe(PAD.A);
    expect(up(pad(SW, "standard", [9]))).toBe(PAD.START);
  });
  test("directions and buttons combine; View/Share/- is not a game button", () => {
    expect(up(pad(XB, "standard", [0, 14, 8]))).toBe(PAD.C | PAD.LEFT);
    expect(up(pad(XB, "standard", [8]))).toBe(0);
  });
  test("assign swaps so a control drives only one Genesis button", () => {
    const m = assign(DEFAULT_MAPS.xbox, "A", "south"); // south was C
    expect(m).toEqual({ C: "west", B: "east", A: "south", START: "start" });
    expect(new Set(Object.values(m)).size).toBe(4);
    expect(assign(DEFAULT_MAPS.xbox, "C", "r2").C).toBe("r2");
    expect(DEFAULT_MAPS.xbox.C).toBe("south"); // not mutated
  });
  test("a remapped trigger works (raw xbox LT on an axis)", () => {
    const map = assign(DEFAULT_MAPS.xbox, "C", "l2");
    const st = readPad(pad("045e-028e-x", "", [], [0, 0, 1, 0, 0, -1, 0, 0], 11));
    expect(padBits(st, map)).toBe(PAD.C);
  });
  test("stored maps are validated", () => {
    expect(sanitizeMaps(null)).toEqual(DEFAULT_MAPS);
    expect(sanitizeMaps({ xbox: { C: "nonsense", B: "east", A: "west", START: "start" } }).xbox).toEqual(DEFAULT_MAPS.xbox);
    const dup = sanitizeMaps({ playstation: { C: "east", B: "east", A: "west", START: "start" } });
    expect(dup.playstation).toEqual(DEFAULT_MAPS.playstation); // two Genesis buttons on one control: discarded
    const ok = sanitizeMaps({ switch: { C: "r2", B: "south", A: "west", START: "start" } });
    expect(ok.switch.C).toBe("r2");
    expect(ok.xbox).toEqual(DEFAULT_MAPS.xbox);
    expect(sanitizeMaps({ xbox: { C: "r1" } }).xbox.C).toBe("r1"); // partial: the rest from the defaults
    expect(sanitizeMaps("junk")).toEqual(DEFAULT_MAPS);
  });
});

describe("two-layer icon resolution", () => {
  // layer 1 ($78BA) from docs/re/script.md section 7: 0:(A,B,C) 1:(A,C,B) 2:(B,A,C) 3:(C,A,B) 4:(C,B,A) 5:(B,C,A) for (EC, EE, F0)
  const DOC = ["ABC", "ACB", "BAC", "CAB", "CBA", "BCA"];
  test("CONFIG_LETTERS matches the documented table", () => {
    expect(CONFIG_LETTERS.map((r) => r.join(""))).toEqual(DOC);
  });
  const subcode = join(import.meta.dir, "../../assets/raw/files/SUBCODE.BIN");
  test.skipIf(!existsSync(subcode))("CONFIG_LETTERS matches the table in the user's SUBCODE.BIN (S:0xE034)", () => {
    const d = readFileSync(subcode);
    const o = 0xe034 - 0xd400;
    for (let cfg = 0; cfg < 6; cfg++) {
      const bitsForABC = [d[o + 3 * cfg], d[o + 3 * cfg + 1], d[o + 3 * cfg + 2]]; // logical bit of physical A, B, C
      const letterOfBit = (bit: number) => "ABC"[bitsForABC.indexOf(bit)];
      expect([letterOfBit(6), letterOfBit(4), letterOfBit(5)].join("")).toBe(DOC[cfg]);
    }
  });

  const dev = (family: Family): Device => ({ kind: "pad", index: 0, family, id: "x" });
  test("all six configurations x three functions x four devices (default maps)", () => {
    // default maps: A -> west, B -> east, C -> south (switch: C -> east, B -> south)
    const ctl: Record<string, Record<string, string>> = {
      xbox: { A: "west", B: "east", C: "south" }, playstation: { A: "west", B: "east", C: "south" },
      switch: { A: "west", B: "south", C: "east" }, generic: { A: "west", B: "east", C: "south" },
    };
    for (let cfg = 0; cfg < 6; cfg++) {
      (["EC", "EE", "F0"] as const).forEach((fn, k) => {
        const letter = DOC[cfg][k];
        for (const f of ["xbox", "playstation", "switch", "generic"] as Family[]) {
          const r = resolveIcon(cfg, fn, dev(f), DEFAULT_MAPS);
          expect(r.letter).toBe(letter);
          expect(r.control).toBe(ctl[f][letter]);
          expect(r.set).toBe(f === "playstation" ? "playstation" : f === "switch" ? "switch" : "xbox");
        }
        const kb = resolveIcon(cfg, fn, { kind: "keyboard" }, DEFAULT_MAPS);
        expect(kb.control).toBe({ A: "Z", B: "X", C: "C" }[letter as "A"]);
      });
    }
  });
  test("a remap changes the icon of the function, not the function", () => {
    const maps = sanitizeMaps({ xbox: { C: "r2", B: "east", A: "west", START: "start" } });
    // config 0: F0 ("fire") is Genesis C -> RT; config 1: F0 is B -> east
    expect(resolveIcon(0, "F0", dev("xbox"), maps).control).toBe("r2");
    expect(resolveIcon(1, "F0", dev("xbox"), maps).control).toBe("east");
    expect(resolveIcon(1, "EE", dev("xbox"), maps).control).toBe("r2");
  });
  test("engine arguments: style and control ids for each device", () => {
    expect(promptFor({ kind: "keyboard" }, DEFAULT_MAPS)).toEqual({ style: STYLE.keyboard, ids: [0, 1, 2, 3] });
    expect(promptFor(dev("xbox"), DEFAULT_MAPS)).toEqual({ style: STYLE.xbox, ids: [CONTROLS.indexOf("west"), CONTROLS.indexOf("east"), CONTROLS.indexOf("south"), 8] });
    expect(promptFor(dev("playstation"), DEFAULT_MAPS).style).toBe(STYLE.playstation);
    expect(promptFor(dev("switch"), DEFAULT_MAPS).ids).toEqual([2, 0, 1, 8]); // A=west(Y) B=south(B) C=east(A)
    expect(promptFor(dev("generic"), DEFAULT_MAPS).style).toBe(STYLE.xbox);
    expect(GENESIS_ORDER).toEqual(["A", "B", "C", "START"]);
  });
});
