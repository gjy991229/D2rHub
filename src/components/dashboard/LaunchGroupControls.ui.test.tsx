import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { AccountMeta, LaunchGroup } from "../../store/types";
import { FavoriteLaunchGroups } from "./FavoriteLaunchGroups";
import { LaunchGroupPanel } from "./LaunchGroupPanel";

const readyAccount: AccountMeta = {
  id: "account-1",
  display_name: "一号账号",
  mod_args: "",
  created_at: "2026-08-31T00:00:00Z",
  last_launched_at: null,
  last_reset_at: null,
  initialized: true,
  order: 0,
  is_running: false,
  auth_mode: "token",
  region: "KR",
};

const group: LaunchGroup = {
  id: "group-1",
  name: "刷符文",
  account_ids: [readyAccount.id],
  members: [{
    account_id: readyAccount.id,
    mod_args: "",
    position_preset_id: null,
    position_configured: true,
    graphics_configured: true,
    resolution: "1280x720",
    fps: 60,
  }],
};

describe("launch group controls", () => {
  it("adds and removes a scheme from the main action bar through the star control", () => {
    const onToggleFavorite = vi.fn();
    const { rerender } = render(
      <LaunchGroupPanel
        groups={[group]}
        accounts={[readyAccount]}
        config={null}
        onClose={vi.fn()}
        onLaunch={vi.fn()}
        onCreate={vi.fn()}
        onEdit={vi.fn()}
        onToggleFavorite={onToggleFavorite}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: `设为常用启动方案“${group.name}”` }));
    expect(onToggleFavorite).toHaveBeenLastCalledWith(group);

    rerender(
      <LaunchGroupPanel
        groups={[group]}
        accounts={[readyAccount]}
        config={null}
        favoriteGroupIds={[group.id]}
        onClose={vi.fn()}
        onLaunch={vi.fn()}
        onCreate={vi.fn()}
        onEdit={vi.fn()}
        onToggleFavorite={onToggleFavorite}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: `取消常用启动方案“${group.name}”` }));
    expect(onToggleFavorite).toHaveBeenLastCalledWith(group);
  });

  it("keeps an unavailable favorite visible but prevents direct launch", () => {
    const onLaunch = vi.fn();
    render(
      <FavoriteLaunchGroups
        groups={[group]}
        favoriteGroupIds={[group.id]}
        accounts={[{ ...readyAccount, initialized: false }]}
        config={null}
        onLaunch={onLaunch}
        onManageFavorites={vi.fn()}
      />,
    );

    const favorite = screen.getByRole("button", { name: group.name });
    expect(favorite).toHaveProperty("disabled", true);
    fireEvent.click(favorite);
    expect(onLaunch).not.toHaveBeenCalled();
  });
});
