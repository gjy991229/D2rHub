import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { AccountMeta } from "../../store/types";
import { SortableAccountCard } from "./SortableAccountCard";

const mocks = vi.hoisted(() => ({ dragKeyDown: vi.fn() }));

vi.mock("@dnd-kit/sortable", () => ({
  useSortable: () => ({
    attributes: { tabIndex: 0 },
    listeners: { onKeyDown: mocks.dragKeyDown },
    setNodeRef: vi.fn(),
    transform: null,
    transition: undefined,
    isDragging: false,
  }),
}));

vi.mock("@dnd-kit/utilities", () => ({
  CSS: { Transform: { toString: () => undefined } },
}));

vi.mock("../../store/launch", () => ({
  useLaunch: () => ({ progress: {} }),
}));

vi.mock("./AccountCard", () => ({
  AccountGridItem: () => <input aria-label="Mod 参数" />,
}));

const account: AccountMeta = {
  id: "account-1",
  display_name: "一号账号",
  mod_args: "",
  created_at: "2026-08-31T00:00:00Z",
  last_launched_at: null,
  last_reset_at: null,
  initialized: true,
  order: 0,
  is_running: false,
};

describe("SortableAccountCard keyboard boundary", () => {
  it("keeps spaces in the Mod input without activating keyboard drag", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <SortableAccountCard
        account={account}
        onRename={vi.fn()}
        onDelete={vi.fn()}
        onConfigure={vi.fn()}
        onLaunch={vi.fn()}
        onBattleNetOnly={vi.fn()}
      />,
    );

    const input = screen.getByRole("textbox", { name: "Mod 参数" });
    await user.type(input, "-mod alpha beta");

    expect(input).toHaveProperty("value", "-mod alpha beta");
    expect(mocks.dragKeyDown).not.toHaveBeenCalled();

    const sortable = container.querySelector("[data-sortable-account-id]");
    expect(sortable).not.toBeNull();
    fireEvent.keyDown(sortable!, { key: " " });
    expect(mocks.dragKeyDown).toHaveBeenCalledOnce();
  });
});
