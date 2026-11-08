import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AccountStatusLight } from "./AccountStatusLight";
import { emptySelection, toggleBatchSelection, reconcileBatchSelection } from "../../hooks/useAccountBatch";
import type { AccountMeta } from "../../store/types";

afterEach(cleanup);
describe("account batch selection", () => {
  it("names runtime and batch selection states in English without a DOM translation pass", () => {
    render(<AccountStatusLight name="A" running issue={null} selected mode="close" english />);
    expect(screen.getByText("To close")).toBeTruthy();
    expect(screen.getByRole("button", { name: "A：Selected to close. Click to deselect." })).toBeTruthy();
  });
  it("allows multiple same-action accounts, rejects mixed actions and leaves mode after last deselection", () => {
    const first = toggleBatchSelection(emptySelection, "a", false);
    const second = toggleBatchSelection(first, "b", false);
    expect(second).toEqual({ mode: "launch", ids: ["a", "b"] });
    expect(toggleBatchSelection(second, "c", true)).toBe(second);
    expect(toggleBatchSelection(toggleBatchSelection(second, "a", false), "b", false)).toEqual(emptySelection);
    expect(toggleBatchSelection(emptySelection, "c", true)).toEqual({ mode: "close", ids: ["c"] });
  });
  it("removes deleted accounts and accounts whose process state changed without switching intent", () => {
    const accounts = [{ id: "a", is_running: true }, { id: "b", is_running: false }] as AccountMeta[];
    expect(reconcileBatchSelection({ mode: "launch", ids: ["a", "b", "deleted"] }, accounts)).toEqual({ mode: "launch", ids: ["b"] });
    expect(reconcileBatchSelection({ mode: "close", ids: ["b"] }, accounts)).toEqual(emptySelection);
  });
});
describe("account status light", () => {
  it("selects via the light without opening or dragging its card", () => {
    const toggle = vi.fn(), parent = vi.fn(), drag = vi.fn();
    render(<div onClick={parent} onMouseDown={drag}><AccountStatusLight name="A" running={false} issue={null} onToggle={toggle} /></div>);
    const light = screen.getByRole("button", { name: /未运行/ });
    fireEvent.mouseDown(light);
    fireEvent.click(light);
    expect(toggle).toHaveBeenCalledOnce();
    expect(parent).not.toHaveBeenCalled();
    expect(drag).not.toHaveBeenCalled();
  });
  it("uses distinct selection states and text for launch and close", () => {
    const { container, rerender } = render(<AccountStatusLight name="A" running={false} issue={null} selected mode="launch" />);
    expect(screen.getByText("待启动")).toBeTruthy();
    expect(container.querySelector('[data-selection="launch"]')).toBeTruthy();
    rerender(<AccountStatusLight name="A" running issue={null} selected mode="close" />);
    expect(screen.getByText("待关闭")).toBeTruthy();
    expect(container.querySelector('[data-selection="close"]')).toBeTruthy();
  });
  it("repairs an idle unhealthy account, but allows closing a running unhealthy account", () => {
    const repair = vi.fn(), toggle = vi.fn();
    const { rerender } = render(<AccountStatusLight name="A" running={false} issue="缺少 Token" onRepair={repair} onToggle={toggle} />);
    fireEvent.click(screen.getByRole("button"));
    expect(repair).toHaveBeenCalledOnce();
    expect(toggle).not.toHaveBeenCalled();
    rerender(<AccountStatusLight name="A" running issue="缺少 Token" onRepair={repair} onToggle={toggle} />);
    fireEvent.click(screen.getByRole("button"));
    expect(toggle).toHaveBeenCalledOnce();
  });
  it("blocks opposite intent and unknown state, but lets an existing selection be cancelled", () => {
    const toggle = vi.fn();
    const { rerender } = render(<AccountStatusLight name="A" running issue={null} mode="launch" onToggle={toggle} />);
    expect(screen.getByRole("button")).toHaveProperty("disabled", true);
    rerender(<AccountStatusLight name="A" running={false} issue={null} uncertain onToggle={toggle} />);
    expect(screen.getByRole("button")).toHaveProperty("disabled", true);
    rerender(<AccountStatusLight name="A" running={false} issue="配置失效" uncertain selected mode="launch" onToggle={toggle} />);
    fireEvent.click(screen.getByRole("button"));
    expect(toggle).toHaveBeenCalledOnce();
  });
});
