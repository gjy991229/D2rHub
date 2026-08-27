import {
  aggregateOverlayDrops,
  getAppendedOverlayDrops,
  getOverlayDropKey,
  getOverlayDropLabel,
  type OverlayDropLike,
} from "./overlayDrops";

function assertEqual(actual: unknown, expected: unknown, message: string) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(`FAIL: ${message}\nexpected=${JSON.stringify(expected)}\nactual=${JSON.stringify(actual)}`);
  }
  console.log(`PASS: ${message}`);
}

function makeDrop(overrides: Partial<OverlayDropLike> = {}): OverlayDropLike {
  return {
    kind: "rune",
    telemetryId: 10,
    itemCode: "r10",
    name: "书尔",
    nameEn: "Thul",
    runeNumber: 10,
    ...overrides,
  };
}

export function runTests() {
  const drops = [
    makeDrop(),
    makeDrop({ kind: "item", telemetryId: 101, itemCode: "cm1", name: "小型神符", nameEn: "Small Charm", runeNumber: null }),
    makeDrop(),
  ];
  const aggregated = aggregateOverlayDrops(drops);

  assertEqual(
    aggregated.map(({ key, count, latestIndex }) => ({ key, count, latestIndex })),
    [
      { key: "rune:r10", count: 2, latestIndex: 2 },
      { key: "item:cm1", count: 1, latestIndex: 1 },
    ],
    "overlay groups identical drops and orders groups by their latest observation",
  );
  assertEqual(
    getAppendedOverlayDrops(drops.slice(0, 1), drops),
    drops.slice(1),
    "all drops appended in one render remain available for simultaneous drop notices",
  );
  assertEqual(
    getAppendedOverlayDrops(drops, drops.slice(0, 2)),
    [],
    "removing an overlay entry never creates a false new-drop notice",
  );
  assertEqual(
    getOverlayDropKey(makeDrop({ itemCode: null, telemetryId: 33 })),
    "rune:33",
    "overlay drop key falls back to the telemetry id when a code is unavailable",
  );
  assertEqual(getOverlayDropLabel(makeDrop(), false), "#10 书尔", "rune labels include the rune number in Chinese");
  assertEqual(getOverlayDropLabel(makeDrop(), true), "#10 Thul", "rune labels use the English name when requested");
  assertEqual(
    getOverlayDropLabel(makeDrop({ kind: "item", name: "小型神符", nameEn: "Small Charm", runeNumber: null }), true),
    "Small Charm",
    "non-rune labels omit the rune prefix",
  );
}

const g = globalThis as any;
if (typeof g.process !== "undefined" && typeof g.process.argv !== "undefined") {
  try {
    runTests();
  } catch (error) {
    console.error(error);
    g.process.exit(1);
  }
}
