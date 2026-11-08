import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { DisplaySection, GameplaySection, GraphicsSection } from "./fields";

afterEach(cleanup);
it("preserves boolean and numeric switch representations", () => {
  const update = vi.fn();
  render(<GameplaySection settings={{ "Camera Shake": true, "Always Run": 0 }} update={update} />);
  fireEvent.click(screen.getByRole("switch", { name: "镜头震动" }));
  fireEvent.click(screen.getByRole("switch", { name: "始终奔跑" }));
  expect(update.mock.calls).toEqual([["Camera Shake", false], ["Always Run", 1]]);
});
it("does not invent missing switches or interpret unexpected enum values", () => {
  render(<GameplaySection settings={{ "Camera Shake": 2 }} update={vi.fn()} />);
  expect(screen.queryByRole("switch", { name: "镜头震动" })).toBeNull();
  expect(screen.queryByRole("switch", { name: "始终奔跑" })).toBeNull();
});

it("names non-toggle game controls and commits a complete numeric edit on blur", () => {
  const update = vi.fn();
  const view = render(<DisplaySection settings={{}} update={update} />);
  expect(screen.getByRole("combobox", { name: "窗口模式" })).toBeTruthy();
  expect(screen.getByRole("slider", { name: "亮度" })).toBeTruthy();
  view.rerender(<GraphicsSection settings={{ "Framerate Cap": 30 }} update={update} />);
  const input = screen.getByRole("spinbutton", { name: "帧数上限" });
  fireEvent.focus(input);
  fireEvent.change(input, { target: { value: "" } });
  fireEvent.change(input, { target: { value: "144" } });
  expect(update).not.toHaveBeenCalled();
  fireEvent.blur(input);
  expect(update).toHaveBeenCalledExactlyOnceWith("Framerate Cap", 144);
});
