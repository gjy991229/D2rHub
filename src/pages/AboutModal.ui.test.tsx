import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
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

it("shows both full QR images directly and celebrates when returning to About", async () => {
  const user = userEvent.setup();
  const closeAbout = vi.fn();
  render(<AboutModal open onClose={closeAbout} />);
  const opener = screen.getByRole("button", { name: "赞助支持作者" });
  await user.click(opener);
  const support = await screen.findByRole("dialog", { name: "赞助支持作者" });
  expect(within(support).getByRole("img", { name: "微信收款码" })).toBeTruthy();
  expect(within(support).getByRole("img", { name: "支付宝收款码" })).toBeTruthy();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "赞助支持作者" })).toBeNull());
  expect(document.activeElement).toBe(opener);
  expect(document.querySelector(".about-fireworks")).toBeTruthy();
  expect(closeAbout).not.toHaveBeenCalled();
});

it("opens the sponsor list in its own dialog and returns to the About entry", async () => {
  const user = userEvent.setup();
  render(<AboutModal open onClose={vi.fn()} />);
  const entry = screen.getByRole("button", { name: "感谢赞助名单" });
  await user.click(entry);
  const thanks = await screen.findByRole("dialog", { name: "感谢赞助名单" });
  expect(within(thanks).getByRole("listitem").textContent).toBe("Sponsor");
  expect(screen.queryByRole("dialog", { name: "赞助支持作者" })).toBeNull();
  await user.click(within(thanks).getByRole("button", { name: "关闭对话框" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "感谢赞助名单" })).toBeNull());
  expect(document.activeElement).toBe(entry);
});
