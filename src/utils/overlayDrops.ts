export interface OverlayDropLike {
  kind: "rune" | "item";
  telemetryId: number;
  itemCode: string | null;
  name: string;
  nameEn: string | null;
  runeNumber: number | null;
}

export interface AggregatedOverlayDrop<T extends OverlayDropLike> {
  key: string;
  drop: T;
  count: number;
  latestIndex: number;
}

export function getOverlayDropKey(drop: OverlayDropLike): string {
  return `${drop.kind}:${drop.itemCode || drop.telemetryId}`;
}

export function getAppendedOverlayDrops<T>(previous: readonly T[], current: readonly T[]): T[] {
  return current.length > previous.length ? current.slice(previous.length) : [];
}

export function aggregateOverlayDrops<T extends OverlayDropLike>(
  drops: readonly T[],
): AggregatedOverlayDrop<T>[] {
  const groups = new Map<string, AggregatedOverlayDrop<T>>();

  drops.forEach((drop, index) => {
    const key = getOverlayDropKey(drop);
    const current = groups.get(key);
    if (current) {
      current.count += 1;
      current.latestIndex = index;
      current.drop = drop;
      return;
    }

    groups.set(key, { key, drop, count: 1, latestIndex: index });
  });

  return [...groups.values()].sort((left, right) => right.latestIndex - left.latestIndex);
}

export function getOverlayDropLabel(drop: OverlayDropLike, useEnglish: boolean): string {
  const localizedName = useEnglish && drop.nameEn ? drop.nameEn : drop.name;
  return drop.kind === "rune" && drop.runeNumber !== null
    ? `#${drop.runeNumber} ${localizedName}`
    : localizedName;
}
