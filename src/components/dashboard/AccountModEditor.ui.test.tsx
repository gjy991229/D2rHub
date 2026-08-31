import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { AccountMeta } from "../../store/types";
import { AccountModEditor } from "./AccountModEditor";

const mocks = vi.hoisted(() => ({
  addAccountMod: vi.fn(async () => true),
  updateAccountMods: vi.fn(async () => true),
}));

vi.mock("../../store/accounts", () => ({
  useAccounts: () => mocks,
}));

const account: AccountMeta = {
  id: "account-1",
  display_name: "一号账号",
  mod_args: "",
  mod_list: [],
  created_at: "2026-08-31T00:00:00Z",
  last_launched_at: null,
  last_reset_at: null,
  initialized: true,
  order: 0,
  is_running: false,
};

describe("AccountModEditor", () => {
  it("commits a Mod argument containing spaces as one configuration", async () => {
    const user = userEvent.setup();
    render(<AccountModEditor account={account} />);

    await user.click(screen.getByTitle("添加 mod"));
    const input = screen.getByPlaceholderText("-mod xxx");
    await user.type(input, "-mod alpha beta{Enter}");

    await waitFor(() => {
      expect(mocks.addAccountMod).toHaveBeenCalledWith(account.id, "-mod alpha beta");
    });
  });
});
