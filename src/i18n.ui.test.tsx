import { describe, expect, it } from "vitest";
import { translateUiText } from "./i18n";

describe("translateUiText settings copy", () => {
  it("translates the settings title with its dynamic save status", () => {
    expect(translateUiText("设置中心 · 已自动保存", "en-US")).toBe(
      "Settings Center · Auto-saved",
    );
    expect(translateUiText("设置中心 · 自动保存中", "en-US")).toBe(
      "Settings Center · Auto-saving",
    );
    expect(translateUiText("设置中心 · 有未保存改动", "en-US")).toBe(
      "Settings Center · Unsaved changes",
    );
  });

  it("translates dynamic account and shortcut labels", () => {
    expect(translateUiText("Ladder Sorc · 画质与启动", "en-US")).toBe(
      "Ladder Sorc · Graphics and Launch",
    );
    expect(translateUiText("位置 4 快捷键", "en-US")).toBe("Position 4 shortcut");
    expect(translateUiText("清除位置 4 快捷键", "en-US")).toBe(
      "Clear position 4 shortcut",
    );
    expect(translateUiText("选择账号 Ladder Sorc", "en-US")).toBe(
      "Select account Ladder Sorc",
    );
  });

  it("translates long settings guidance without changing Chinese mode", () => {
    const guidance = "软件界面显示语言，游戏内容和符文名称不受影响";
    expect(translateUiText(guidance, "en-US")).toBe(
      "Language for the app UI. Game content and rune names are not changed.",
    );
    expect(translateUiText(guidance, "zh-CN")).toBe(guidance);
  });

  it("translates interpolated path and automation guidance", () => {
    expect(
      translateUiText(
        "国服存档目录中未检测到 Settings.json，账号独立画质快照与覆盖暂不可用。",
        "en-US",
      ),
    ).toBe(
      "Settings.json was not found in the CN save directory. Per-account graphics snapshots and overrides are unavailable for now.",
    );
    expect(
      translateUiText("只识别“Ladder Sorc”对应的游戏声音。", "en-US"),
    ).toBe("Only audio from “Ladder Sorc” will be recognized.");
  });
});
