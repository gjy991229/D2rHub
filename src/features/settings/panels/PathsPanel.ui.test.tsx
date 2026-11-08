import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { GlobalConfig } from "../../../store/types";
import { PathsPanel } from "./PathsPanel";

afterEach(cleanup);

it("offers only detected paths and gives each browser action a distinct name", () => {
  const pickFolder = vi.fn();
  const applyDetectedPath = vi.fn();
  const path = "D:\\Games\\A very long installation directory\\Diablo II Resurrected";
  render(<PathsPanel config={{ cn_game_path: path, cn_saved_games_path: "", cn_battle_net_path: "", global_game_path: "",
    global_saved_games_path: "", program_data_agent_path: "", app_data_roaming_bnet_path: "", browser_path: "", browser_type: "" } as GlobalConfig}
    settingsAvailable={{ CN: true, Global: null }} detectedPaths={{ cnSavedGames: "C:\\Saves", globalSavedGames: null }}
    updateConfig={vi.fn()} pickFile={vi.fn()} pickFolder={pickFolder} applyDetectedPath={applyDetectedPath} />);
  expect(screen.getByRole("textbox", { name: "国服游戏安装目录" }).getAttribute("title")).toBe(path);
  expect((screen.getByRole("button", { name: "使用探测到的国际服配置目录" }) as HTMLButtonElement).disabled).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "使用探测到的国服配置目录" }));
  expect(applyDetectedPath).toHaveBeenCalledExactlyOnceWith("cn_saved_games_path", "C:\\Saves");
  fireEvent.click(screen.getByRole("button", { name: "浏览国际服游戏安装目录" }));
  expect(pickFolder).toHaveBeenCalledExactlyOnceWith("global_game_path", "国际服游戏安装目录");
});
