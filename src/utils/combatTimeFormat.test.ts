import { formatCombatTime } from "./combatTimeFormat";

function equal(actual: string, expected: string) {
  if (actual !== expected) throw new Error(`Expected ${expected}, received ${actual}`);
}

equal(formatCombatTime(75_400).value, "75.4");
equal(formatCombatTime(75_400, "minutes").value, "01:15");
equal(formatCombatTime(3_661_400, "hours").value, "1:01:01");
equal(formatCombatTime(3_661_400, "minutes").value, "61:01");
equal(formatCombatTime(75_400, "legacy").value, "75.4");
equal(formatCombatTime(75_400, "seconds", true).unit, "SEC");
console.log("combat timer format tests passed");
