import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { LaunchProgressView } from "./LaunchProgress";

afterEach(() => { cleanup(); vi.useRealTimers(); });

it("keeps failure details readable and separates clearing from disclosure", () => {
  vi.useFakeTimers();
  const clear = vi.fn();
  render(<LaunchProgressView accounts={[]} onClear={clear} logs={[
    { timestamp: "2026-09-29T10:00:00Z", account_id: "a", step: "connect", status: "error", message: "连接失败" },
  ]} />);
  const disclosure = screen.getByRole("button", { name: /运行日志/ });
  act(() => vi.advanceTimersByTime(10_000));
  expect(disclosure.getAttribute("aria-expanded")).toBe("true");
  fireEvent.click(disclosure);
  expect(disclosure.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByRole("region", { name: "日志条目" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "清除日志" }));
  expect(clear).toHaveBeenCalledOnce();
  expect(disclosure.getAttribute("aria-expanded")).toBe("false");
  expect(disclosure.querySelector("button")).toBeNull();
});

it("respects a manual collapse when new logs arrive, but honors an explicit reveal", () => {
  const first = { timestamp: "2026-09-29T10:00:00Z", account_id: "a", step: "connect", status: "ok", message: "已连接" };
  const { rerender } = render(<LaunchProgressView accounts={[]} onClear={vi.fn()} logs={[first]} />);
  const disclosure = screen.getByRole("button", { name: /运行日志/ });
  fireEvent.click(disclosure);
  rerender(<LaunchProgressView accounts={[]} onClear={vi.fn()} logs={[first, { ...first, status: "error", message: "发生错误" }]} />);
  expect(disclosure.getAttribute("aria-expanded")).toBe("false");
  expect(screen.getByText("有失败记录")).toBeTruthy();
  rerender(<LaunchProgressView accounts={[]} onClear={vi.fn()} logs={[first]} revealRevision={1} />);
  expect(disclosure.getAttribute("aria-expanded")).toBe("true");
});
