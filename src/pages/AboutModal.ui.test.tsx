import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AboutModal } from "./AboutModal";

vi.mock("../platform/tauri", () => ({ invokeCommand: vi.fn(async () => "0.9.119") }));
vi.mock("../components/ui/UpdateConfirmModal", () => ({ default: () => null }));

beforeEach(() => {
  vi.stubGlobal("fetch", vi.fn(async () => ({ ok: true, json: async () => ["Sponsor"] })));
  localStorage.clear();
});
afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

it("closes only the donation image preview on Escape and restores its opener focus", async () => {
  const user = userEvent.setup();
  const closeAbout = vi.fn();
  render(<AboutModal open onClose={closeAbout} />);
  await screen.findByText("版本 v0.9.119");
  await user.click(screen.getByRole("button", { name: /赞助支持作者/ }));
  const opener = screen.getByRole("button", { name: "放大 微信 收款码" });
  await user.click(opener);
  expect(screen.getByRole("dialog", { name: "收款码预览" })).toBeTruthy();
  expect(screen.getByRole("dialog", { name: "关于" })).toBeTruthy();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog", { name: "收款码预览" })).toBeNull();
  expect(screen.getByRole("dialog", { name: "关于" })).toBeTruthy();
  expect(closeAbout).not.toHaveBeenCalled();
  await waitFor(() => expect(document.activeElement).toBe(opener));
  await user.keyboard("{Escape}");
  expect(closeAbout).toHaveBeenCalledTimes(1);
});
