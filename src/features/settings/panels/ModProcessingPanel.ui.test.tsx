import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ModProcessingPanel } from "./ModProcessingPanel";
import { modState, workflowFixture } from "../../mods/workflow/testFixtures";
import { initialModFeatures } from "../../mods/workflow/model";
import type { TaskSnapshot } from "../../tasks/types";

vi.mock("../../../platform/tauri", () => ({ invokeCommand: vi.fn(async () => ({ ready: true })) }));
afterEach(cleanup);

describe("ModProcessingPanel", () => {


  it("supports arrow keys for source and output radio choices", async () => {
    const workflow = workflowFixture({ draft: { recipe: { kind: "create", source: "MyExistingMod", name: "New", replaceSource: false } } });
    render(<ModProcessingPanel workflow={workflow} />);
    const source = screen.getByRole("radio", { name: /已有 Mod/ });
    source.focus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(screen.getByRole("radio", { name: /原版游戏/ }));
    expect(workflow.actions.changeRecipe).toHaveBeenLastCalledWith({ kind: "create", source: null, name: "New" });
    screen.getByRole("radio", { name: /生成新副本/ }).focus();
    await userEvent.keyboard("{End}");
    expect(document.activeElement).toBe(screen.getByRole("radio", { name: /沿用源名称/ }));
    expect(workflow.actions.changeRecipe).toHaveBeenLastCalledWith(expect.objectContaining({ replaceSource: true }));
  });
  it("offers default Hell as an editable room-tool option and preserves saved choices", async () => {
    const workflow = workflowFixture();
    const view = render(<ModProcessingPanel workflow={workflow} />);
    const option = screen.getByRole("checkbox", { name: "创建房间默认地狱" }) as HTMLInputElement;
    expect(option.checked).toBe(true);
    expect(option.disabled).toBe(false);
    await userEvent.click(option);
    expect(workflow.actions.changeFeatures).toHaveBeenCalledWith({ roomToolsDefaultHell: false });
    const state = { ...modState, installed_mods: [{ ...modState.installed_mods[0], room_tools_default_hell: false }] };
    const saved = workflowFixture({ state, draft: { recipe: { kind: "augment", modName: "MyExistingMod" } } });
    view.rerender(<ModProcessingPanel workflow={saved} />);
    expect(option.checked).toBe(false);
    const changed = workflowFixture({ state, draft: { recipe: { kind: "augment", modName: "MyExistingMod" },
      features: { ...initialModFeatures("recognition"), roomToolsDefaultHell: true } } });
    expect(changed.blockedReason).toBe("");
    view.rerender(<ModProcessingPanel workflow={changed} />);
    expect((screen.getByRole("button", { name: "校验并更新" }) as HTMLButtonElement).disabled).toBe(false);
    view.rerender(<ModProcessingPanel workflow={{ ...changed, busy: true }} />);
    expect(option.disabled).toBe(true);
    view.rerender(<ModProcessingPanel workflow={workflowFixture({ draft: {
      recipe: { kind: "create", source: null, name: "AudioOnly" },
    } })} />);
    expect(screen.queryByRole("checkbox", { name: "创建房间默认地狱" })).toBeNull();
  });
  it("offers native loot buttons and same-name processing with retained backups", async () => {
    const workflow = workflowFixture({ draft: { recipe: { kind: "create", source: "MyExistingMod", name: "Unused", replaceSource: true } } });
    render(<ModProcessingPanel workflow={workflow} />);
    expect(workflow.blockedReason).toBe("");
    expect(screen.queryByPlaceholderText("MyD2RHubMod")).toBeNull();
    expect(screen.getByRole("radio", { name: /沿用源名称/ }).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText(/先生成并校验，再替换/)).toBeTruthy();
    await userEvent.click(screen.getByRole("checkbox", { name: /左侧玩家尸体拾取按钮/ }));
    expect(workflow.actions.changeFeatures).toHaveBeenCalledWith({ includePartyLoot: true });
    await userEvent.click(screen.getByRole("radio", { name: /生成新副本/ }));
    expect(workflow.actions.changeRecipe).toHaveBeenCalledWith(expect.objectContaining({ replaceSource: false }));
  });
  it("exposes scene restart with a concise waypoint description", async () => {
    const workflow = workflowFixture();
    render(<ModProcessingPanel workflow={workflow} />);
    const choice = screen.getByRole("checkbox", { name: /场景重开/ });
    expect(screen.getByText(/小站处重开或创房/)).toBeTruthy();
    await userEvent.click(choice);
    expect(workflow.actions.changeFeatures).toHaveBeenCalledWith({ includeSceneShortcuts: true });
  });

  it("blocks rebuilding from a missing or processed original before execution", () => {
    const original = { ...modState.installed_mods[0], name: "Original", feature_groups: [], update_required: false };
    const target = { ...modState.installed_mods[0], source_mod_name: "Original", update_required: true };
    const draft = { origin: "library" as const, recipe: { kind: "augment" as const, modName: target.name } };
    const missing = workflowFixture({ draft, state: { ...modState, installed_mods: [target] } });
    const { rerender } = render(<ModProcessingPanel workflow={missing} />);
    expect(missing.blockedReason).toContain("原始源 Mod 不可用");
    expect(screen.getByRole("button", { name: "校验并更新" }).hasAttribute("disabled")).toBe(true);
    const processed = workflowFixture({ draft, state: { ...modState, installed_mods: [target, { ...original, feature_groups: ["esc_next_game"] }] } });
    rerender(<ModProcessingPanel workflow={processed} />);
    expect(processed.blockedReason).toContain("原始源 Mod 不可用");
    const restored = workflowFixture({ draft, state: { ...modState, installed_mods: [target, original] } });
    rerender(<ModProcessingPanel workflow={restored} />);
    expect(restored.blockedReason).toBe("");
  });
  it("requires repair when the bundled processor is unavailable", async () => {
    const workflow = workflowFixture({ processorReady: false, draft: { recipe: { kind: "create", source: null, name: "" } } });
    render(<ModProcessingPanel workflow={workflow} />);
    const button = screen.getByRole("button", { name: "请修复 Hub 安装" });
    expect(button.hasAttribute("disabled")).toBe(true);
    await userEvent.click(button);
    expect(workflow.actions.prepare).not.toHaveBeenCalled();
  });

  it("waits for processor inspection but lets an already prepared result be applied", async () => {
    const workflow = workflowFixture({ processorReady: null });
    const view = render(<ModProcessingPanel workflow={workflow} />);
    expect(screen.getByRole("button", { name: "正在读取加工器…" }).hasAttribute("disabled")).toBe(true);
    view.rerender(<ModProcessingPanel workflow={{ ...workflow, prepared: true }} />);
    await userEvent.click(screen.getByRole("button", { name: "重试应用" }));
    expect(workflow.actions.prepare).toHaveBeenCalledTimes(1);
  });
  it("offers scoped cancellation while processing and acknowledges a pending cancel request", async () => {
    const cancel = vi.fn(async () => {});
    const workflow = workflowFixture({ busy: true, preparationTask: {
      currentTask: { task_id: 42, state: "running", cancel_requested: false } as TaskSnapshot,
      cancel, cancelError: null, cancelling: false,
    } });
    const { rerender } = render(<ModProcessingPanel workflow={workflow} />);
    await userEvent.click(screen.getByRole("button", { name: "取消加工" }));
    expect(cancel).toHaveBeenCalledTimes(1);
    rerender(<ModProcessingPanel workflow={{ ...workflow, preparationTask: { ...workflow.preparationTask, cancelling: true } }} />);
    expect((screen.getByRole("button", { name: "正在取消…" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: "正在加工…" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("associates a disabled preparation action with its reason and enables it after correction", async () => {
    const blocked = workflowFixture({ draft: { recipe: { kind: "create", source: null, name: "" } } });
    const { rerender } = render(<ModProcessingPanel workflow={blocked} />);
    const button = screen.getByRole("button", { name: "开始加工并应用" }) as HTMLButtonElement;
    const reason = document.getElementById(button.getAttribute("aria-describedby")!)!;
    expect(reason.textContent).toContain("请输入新 Mod 名称");
    expect(document.getElementById(button.getAttribute("aria-describedby")!)).toBe(reason);
    expect(button.disabled).toBe(true);
    await userEvent.click(button);
    expect(blocked.actions.prepare).not.toHaveBeenCalled();
    const ready = workflowFixture({ draft: { recipe: { kind: "create", source: null, name: "MyNewMod" } } });
    rerender(<ModProcessingPanel workflow={ready} />);
    expect(button.disabled).toBe(false);
    expect(button.getAttribute("aria-describedby")).toBeNull();
    await userEvent.click(button);
    expect(ready.actions.prepare).toHaveBeenCalledTimes(1);
  });

  it("requires recognition alone and lets the user explicitly add room tools", async () => {
    const workflow = workflowFixture({ draft: { recipe: { kind: "create", source: null, name: "New" } } });
    render(<ModProcessingPanel workflow={workflow} />);
    const audio = screen.getByRole("checkbox", { name: /声纹识别/ }) as HTMLInputElement;
    expect(audio.checked).toBe(true); expect(audio.disabled).toBe(true);
    expect(screen.getByText("本次目标 · 必选")).toBeTruthy();
    const rooms = screen.getByRole("checkbox", { name: /局内房间工具/ }) as HTMLInputElement;
    expect(rooms.checked).toBe(false);
    expect(rooms.disabled).toBe(false);
    await userEvent.click(rooms);
    expect(workflow.actions.changeFeatures).toHaveBeenCalledWith({ includeRoomTools: true });
    expect((screen.getByRole("checkbox", { name: /死亡后自动退房/ }) as HTMLInputElement).disabled).toBe(false);
  });

  it("locks every inherited feature from the selected source", () => {
    render(<ModProcessingPanel workflow={workflowFixture()} />);
    for (const name of [/声纹识别/, /局内房间工具/]) {
      const control = screen.getByRole("checkbox", { name }) as HTMLInputElement;
      expect(control.checked).toBe(true); expect(control.disabled).toBe(true);
    }
    expect(screen.getAllByText("源 Mod 已有")).toHaveLength(2);
  });

  it("returns room setup to its own origin and locks the room prerequisite", async () => {
    const workflow = workflowFixture({ draft: { origin: "room-automation", features: initialModFeatures("room-automation"), recipe: { kind: "create", source: null, name: "Rooms" } } });
    render(<ModProcessingPanel workflow={workflow} />);
    const rooms = screen.getByRole("checkbox", { name: /局内房间工具/ }) as HTMLInputElement;
    expect(rooms.checked).toBe(true); expect(rooms.disabled).toBe(true);
    expect(screen.getByText("自动跟房必选")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "返回识别设置" })).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "返回自动跟房" }));
    expect(workflow.actions.back).toHaveBeenCalledTimes(1);
  });

  it("preserves installed modules during augmentation without rendering activation switches", () => {
    const state = { ...modState, installed_mods: [{ ...modState.installed_mods[0], name: "Ready",
      feature_groups: ["audio_telemetry", "auto_exit_on_death"], auto_exit_on_death_enabled: true }] };
    render(<ModProcessingPanel workflow={workflowFixture({ state, draft: { origin: "library", recipe: { kind: "augment", modName: "Ready" },
      features: { ...initialModFeatures("library"), includeRoomTools: true } } })} />);
    for (const name of [/声纹识别/, /死亡后自动退房/]) {
      const field = screen.getByRole("checkbox", { name }) as HTMLInputElement;
      expect(field.checked).toBe(true); expect(field.disabled).toBe(true);
    }
    expect((screen.getByRole("checkbox", { name: /局内房间工具/ }) as HTMLInputElement).disabled).toBe(false);
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.getByRole("button", { name: "增补所选模块" })).toBeTruthy();
  });

  it("uses English feature names with identical required-feature semantics", () => {
    render(<ModProcessingPanel workflow={workflowFixture({ en: true })} />);
    expect(screen.getByText("Feature modules")).toBeTruthy();
    expect((screen.getByRole("checkbox", { name: /Audio recognition/ }) as HTMLInputElement).disabled).toBe(true);
    expect(screen.getByRole("checkbox", { name: /In-game room tools/ })).toBeTruthy();
    expect(screen.getByRole("checkbox", { name: /Auto-exit after death/ })).toBeTruthy();
  });
});
