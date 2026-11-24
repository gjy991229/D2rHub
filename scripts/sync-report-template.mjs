import { build } from "esbuild";
import { readFileSync, writeFileSync } from "node:fs";

const file = new URL("../docs/stats.html", import.meta.url);
const template = readFileSync(file, "utf8");
const { outputFiles } = await build({
  stdin: { contents: 'export { sanitizeBattleReportConfig, buildBattleReportSnapshot, drawBattleReport } from "./src/utils/battleReport"', resolveDir: process.cwd() },
  bundle: true, format: "iife", globalName: "HubReport", minify: true, charset: "utf8", write: false,
});
const start = "/* BEGIN SHARED REPORT */";
const end = "/* END SHARED REPORT */";
const from = template.indexOf(start), to = template.indexOf(end);
if (from < 0 || to < from) throw Error("Missing report bundle markers");
const newline = template.includes("\r\n") ? "\r\n" : "\n";
const next = template.slice(0, from + start.length) + newline + outputFiles[0].text.trim().replaceAll("\n", newline) + newline + "    " + template.slice(to);
if (process.argv.includes("--check")) {
  if (next !== template) throw Error("Refresh the stats page with npm run sync:report");
} else writeFileSync(file, next);
