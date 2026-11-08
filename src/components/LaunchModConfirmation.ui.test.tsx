import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { LaunchModConfirmation } from "./LaunchModConfirmation";
import { confirmLaunchMod } from "../store/launchModConfirmation";
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({
  unminimize: vi.fn(async () => {}), show: vi.fn(async () => {}), setFocus: vi.fn(async () => {}),
}) }));

afterEach(cleanup);
it("uses the themed dialog and returns without launch on Escape", async () => {
  render(<LaunchModConfirmation />);
  let decision!: Promise<boolean>;
  await act(async () => { decision = confirmLaunchMod("账号甲缺少识别\n账号乙缺少房间工具"); });
  expect(screen.getByRole("dialog")).toBeTruthy();
  expect(screen.getByText("账号甲缺少识别")).toBeTruthy();
  expect(screen.getByText("账号乙缺少房间工具")).toBeTruthy();
  await userEvent.keyboard("{Escape}");
  expect(await decision).toBe(false);
});
it("continues only after clicking the explicit launch action", async () => {
  render(<LaunchModConfirmation />);
  let decision!: Promise<boolean>;
  await act(async () => { decision = confirmLaunchMod("Mod 不支持识别"); });
  await userEvent.click(screen.getByRole("button", { name: "跳过并启动" }));
  expect(await decision).toBe(true);
});
