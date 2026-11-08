import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { AppShell } from "../layout/AppShell";
import { showToast, ToastContainer } from "./Toast";

vi.mock("../../i18n", () => ({ useI18n: () => ({ language: "zh-CN" }), I18nRuntime: () => null }));
vi.mock("../../store/globalConfig", () => ({ useGlobalConfig: () => ({ config: { main_opacity: 95 } }) }));
vi.mock("../../store/theme", () => ({ useTheme: () => ({ theme: "light" }) }));

beforeEach(() => vi.useFakeTimers());
afterEach(() => { cleanup(); vi.clearAllTimers(); vi.useRealTimers(); });

it("portals notifications outside AppShell with appropriate announcements and named dismissal", () => {
  render(<AppShell><span>Workspace</span><ToastContainer /></AppShell>);
  act(() => { showToast("success", "设置已保存"); showToast("error", "保存失败"); });
  const status = screen.getByRole("status");
  const alert = screen.getByRole("alert");
  expect(status.textContent).toContain("设置已保存");
  expect(alert.textContent).toContain("保存失败");
  for (const notification of [status, alert]) {
    expect(notification.closest(".app-shell")).toBeNull();
    expect(notification.parentElement?.parentElement).toBe(document.body);
  }
  fireEvent.click(within(alert).getByRole("button", { name: "关闭提示" }));
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.getByRole("status")).toBe(status);
  act(() => vi.advanceTimersByTime(4000));
  expect(screen.queryByRole("status")).toBeNull();
});
