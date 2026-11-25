import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useGlobalConfig } from "../../store/globalConfig";
import type { AccountMeta, GlobalConfig } from "../../store/types";
import { AccountGrid } from "./AccountGrid";

it("keeps cards as the default at every account count and remembers an explicit list selection", () => {
  localStorage.removeItem("hub-account-view");
  const accounts = Array.from({ length: 21 }, (_, i) => ({ id: String(i), is_running: i === 0 } as AccountMeta));
  const renderGrid = () => render(<AccountGrid accounts={accounts} issue={a => a.id === "1" ? "setup" : null} onReorder={vi.fn()}>
    {a => <div key={a.id} data-testid="row">{a.id}</div>}
  </AccountGrid>);
  const first = renderGrid();
  expect(first.container.querySelector(".spatial-grid")?.getAttribute("data-view")).toBe("cards");
  fireEvent.click(screen.getByRole("button", { name: /待处理|Needs setup/ }));
  expect(screen.getAllByTestId("row").map(el => el.textContent)).toEqual(["1"]);
  fireEvent.click(screen.getByRole("button", { name: /^(列表|List)$/ }));
  first.unmount();
  const reopened = renderGrid();
  expect(reopened.container.querySelector(".spatial-grid")?.getAttribute("data-view")).toBe("list");
  expect(screen.getAllByTestId("row")).toHaveLength(21);
  reopened.unmount();
  localStorage.removeItem("hub-account-view");
});

it("persists the roster choice in app configuration and restores it without WebView storage", async () => {
  const previousStore = useGlobalConfig.getState();
  const config = { account_roster_view: "cards" } as GlobalConfig;
  const patch = vi.fn(async (viewPatch: Partial<GlobalConfig>) => {
    const saved = { ...config, ...viewPatch };
    useGlobalConfig.setState({ config: saved });
    return saved;
  });
  useGlobalConfig.setState({ config, patch });
  const accounts = [{ id: "1" } as AccountMeta];
  const renderGrid = () => render(<AccountGrid accounts={accounts} issue={() => null} onReorder={vi.fn()}>
    {a => <div key={a.id}>{a.id}</div>}
  </AccountGrid>);

  try {
    const first = renderGrid();
    fireEvent.click(screen.getByRole("button", { name: /^(列表|List)$/ }));
    await waitFor(() => expect(patch).toHaveBeenCalledWith({ account_roster_view: "list" }));
    first.unmount();
    localStorage.removeItem("hub-account-view");
    const reopened = renderGrid();
    expect(reopened.container.querySelector(".spatial-grid")?.getAttribute("data-view")).toBe("list");
    reopened.unmount();
  } finally {
    useGlobalConfig.setState(previousStore);
  }
});
