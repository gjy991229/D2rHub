import { useState } from "react";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { AccountMeta, GlobalConfig } from "../../../store/types";
import { LaunchStrategyPanel } from "./LaunchStrategyPanel";

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

const initialConfig = { cn_game_path: "D:\\D2R", global_game_path: "", agent_mode: 3 } as GlobalConfig;
const account = (id: string, overrides: Partial<AccountMeta> = {}): AccountMeta => ({
  id, display_name: id, initialized: true, auth_mode: "bnet", region: "CN", order: 0,
  ...overrides,
} as AccountMeta);

function Harness({ accounts, config = initialConfig }: { accounts: AccountMeta[]; config?: GlobalConfig }) {
  const [draft, setDraft] = useState(config);
  return <>
    <LaunchStrategyPanel config={draft} accounts={accounts} updateConfig={updater => setDraft(current => {
      const next = { ...current };
      updater(next);
      return next;
    })} />
    <output aria-label="Saved Battle.net account">{draft.keep_battle_net_account_id ?? "disabled"}</output>
  </>;
}

describe("Battle.net retention after multi-account launch", () => {
  it("lets keyboard users change Agent compatibility strategies with the arrow keys", async () => {
    // jsdom lacks CSS.escape; the radio name used here has no escapable characters.
    vi.stubGlobal("CSS", { escape: (value: string) => value });
    const user = userEvent.setup();
    render(<Harness accounts={[]} />);
    await user.click(screen.getByText(/^高级启动兼容设置/));
    const delay = screen.getByRole("radio", { name: "延时结束 Agent" });
    await user.click(delay);
    await user.keyboard("{ArrowRight}");
    expect((screen.getByRole("radio", { name: "按客户端数量结束" }) as HTMLInputElement).checked).toBe(true);
    expect(screen.getByRole("radiogroup", { name: "战网客户端运行数阈值" })).toBeTruthy();
  });

  it("offers initialized Battle.net accounts in card order and defaults to off", () => {
    render(<Harness accounts={[
      account("later", { order: 3, display_name: "我的战网" }),
      account("token", { auth_mode: "token" }),
      account("uninitialized", { initialized: false }),
      account("global-legacy", { region: "EU" }),
      account("unsupported", { auth_mode: "invalid" }),
      account("legacy", { auth_mode: null, order: 1 }),
    ]} />);
    const select = screen.getByRole("combobox", { name: "保留战网的账号" });
    expect((select as HTMLSelectElement).value).toBe("");
    expect(within(select).getAllByRole("option").map(option => option.textContent))
      .toEqual(["不保留", "legacy", "我的战网"]);
  });

  it("updates the global draft when an account is selected or retention is disabled", async () => {
    render(<Harness accounts={[account("one"), account("two")]} />);
    const select = screen.getByRole("combobox", { name: "保留战网的账号" });
    await userEvent.selectOptions(select, "two");
    expect(screen.getByLabelText("Saved Battle.net account").textContent).toBe("two");
    expect(initialConfig.keep_battle_net_account_id).toBeUndefined();
    await userEvent.selectOptions(select, "");
    expect(screen.getByLabelText("Saved Battle.net account").textContent).toBe("disabled");
  });

  it("recognizes saved account IDs with case aliases or surrounding whitespace", () => {
    render(<Harness accounts={[account("one")]}
      config={{ ...initialConfig, keep_battle_net_account_id: " ONE " }} />);
    expect((screen.getByRole("combobox", { name: "保留战网的账号" }) as HTMLSelectElement).value).toBe("one");
    expect(screen.queryByText("原账号不可用，请重新选择")).toBeNull();
  });

  it("keeps an unavailable selection visible until the user replaces or disables it", async () => {
    render(<Harness accounts={[account("one", { auth_mode: "token" }), account("two")]}
      config={{ ...initialConfig, keep_battle_net_account_id: "one" }} />);
    const select = screen.getByRole("combobox", { name: "保留战网的账号" });
    expect((select as HTMLSelectElement).value).toBe("one");
    expect((within(select).getByRole("option", { name: "原账号不可用，请重新选择" }) as HTMLOptionElement).disabled).toBe(true);
    expect(screen.getByText("原账号已删除、未初始化或已改为 Token 模式，请重新选择。")).toBeTruthy();
    await userEvent.selectOptions(select, "two");
    expect(screen.queryByText("原账号不可用，请重新选择")).toBeNull();
    expect(screen.getByLabelText("Saved Battle.net account").textContent).toBe("two");
  });

  it("explains how to enable retention when no eligible accounts exist", () => {
    render(<Harness accounts={[account("token", { auth_mode: "token" })]} />);
    expect(screen.getByText("暂无可用的战网模式账号，请先添加并初始化战网模式账号。")).toBeTruthy();
  });
});
