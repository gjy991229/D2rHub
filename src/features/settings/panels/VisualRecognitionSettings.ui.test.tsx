import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { invokeCommand } from "../../../platform/tauri";
import { VisualRecognitionSettings } from "./VisualRecognitionSettings";

vi.mock("../../../platform/tauri", () => ({ invokeCommand: vi.fn() }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });
const config = {
  diagnostic_enabled: false,
  chat_send: false, chat_follow: false, chat_account_id: "follower", poll_ms: 2000,
  chat_roi: { x: 0, y: 0.05, width: 0.6, height: 0.45 },
  area_enabled: false, area_roi: { x: 0.15, y: 0.02, width: 0.7, height: 0.3 },
};
describe("visual settings", () => {
  it("uses the primary account and exposes no conflicting send/listen switches", async () => {
    vi.mocked(invokeCommand).mockImplementation(async command => command === "vision_get_config"
      ? { generation: 1, config } : { message: "idle" });
    const user = userEvent.setup();
    const view = render(<VisualRecognitionSettings mode="chat" language="en-US" primaryAccountId="first" />);
    await user.click(screen.getByText("Cross-PC room signals"));
    await screen.findByText("first");
    expect(screen.queryByRole("combobox")).toBeNull();
    expect(screen.queryByRole("switch", { name: "Follow incoming room signals" })).toBeNull();
    expect(screen.queryByRole("switch", { name: "Send a signal when creating a room" })).toBeNull();
    view.rerender(<VisualRecognitionSettings mode="chat" language="en-US" primaryAccountId="next" />);
    expect(screen.getByText("next")).toBeTruthy();
    view.rerender(<VisualRecognitionSettings mode="chat" language="en-US" />);
    expect(screen.getByText("Choose a primary account")).toBeTruthy();
  });
  it("keeps audio timing independent of OCR resources and opens screenshots", async () => {
    vi.mocked(invokeCommand).mockResolvedValue(undefined);
    render(<VisualRecognitionSettings mode="audio" language="en-US" />);
    expect(screen.getByText("Combat time and drop screenshots")).toBeTruthy();
    expect(invokeCommand).not.toHaveBeenCalled();
    expect(screen.queryByRole("switch")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Open screenshots" }));
    expect(invokeCommand).toHaveBeenCalledExactlyOnceWith("open_drop_screenshots_dir");
  });
  it("waits for an in-flight save when switching pages immediately", async () => {
    let stored = { generation: 4, config: { ...config } };
    let finish!: () => void;
    const saving = new Promise<void>(resolve => { finish = resolve; });
    vi.mocked(invokeCommand).mockImplementation(async (command, payload) => {
      if (command === "vision_get_config") return stored;
      if (command === "vision_save_config") {
        await saving;
        stored = { generation: 5, config: (payload as { config: typeof config }).config };
        return stored;
      }
      return { message: "idle" };
    });
    const user = userEvent.setup();
    const view = render(<VisualRecognitionSettings mode="chat" language="en-US" />);
    await user.click(screen.getByText("Cross-PC room signals"));
    await screen.findByText("Diagnostics and resources");
    await user.click(screen.getByText("Diagnostics and resources"));
    await user.click(screen.getByRole("switch", { name: "Diagnostic logging" }));
    view.unmount();
    render(<VisualRecognitionSettings mode="chat" language="en-US" />);
    await user.click(screen.getByText("Cross-PC room signals"));
    expect(screen.queryByLabelText("Diagnostic logging")).toBeNull();
    finish();
    expect((await screen.findByLabelText("Diagnostic logging")).getAttribute("aria-checked")).toBe("true");
  });
  it("auto-saves switches, merges other settings and restores them after remount", async () => {
    let stored = { generation: 4, config: { ...config } };
    vi.mocked(invokeCommand).mockImplementation(async (command, payload) => {
      if (command === "vision_get_config") return stored;
      if (command === "vision_save_config") {
        stored = { generation: stored.generation + 1, config: (payload as { config: typeof config }).config };
        return stored;
      }
      return { message: "Resources idle", checked: 0, matched: 0, rejected: 0, unavailable: 0 };
    });
    const view = render(<VisualRecognitionSettings mode="chat" language="en-US" />);
    expect(invokeCommand).not.toHaveBeenCalled();
    const user = userEvent.setup();
    await user.click(screen.getByText("Cross-PC room signals"));
    await screen.findByText("Diagnostics and resources");
    // Another page changes an unrelated field before the next save.
    stored = { generation: 8, config: { ...stored.config, chat_send: true } };
    await user.click(screen.getByText("Diagnostics and resources"));
    await user.click(screen.getByRole("switch", { name: "Diagnostic logging" }));
    await waitFor(() => expect(stored.config.diagnostic_enabled).toBe(true));
    expect(stored.config.chat_send).toBe(true);
    expect(stored.config.area_enabled).toBe(false);
    view.unmount();
    render(<VisualRecognitionSettings mode="chat" language="en-US" />);
    await user.click(screen.getByText("Cross-PC room signals"));
    expect((await screen.findByLabelText("Diagnostic logging")).getAttribute("aria-checked")).toBe("true");

  });
  it("retains failed edits across collapse and allows explicit retry", async () => {
    vi.mocked(invokeCommand).mockImplementation(async (command) => {
      if (command === "vision_get_config") return { generation: 4, config };
      if (command === "vision_save_config") throw new Error("disk error");
      return { message: "idle" };
    });
    render(<VisualRecognitionSettings mode="chat" language="en-US" />);
    const user = userEvent.setup();
    await user.click(screen.getByText("Cross-PC room signals"));
    await screen.findByText("Diagnostics and resources");
    await user.click(screen.getByText("Diagnostics and resources"));
    await user.click(screen.getByRole("switch", { name: "Diagnostic logging" }));
    expect((await screen.findByRole("alert")).textContent).toContain("disk error");
    await user.click(screen.getByText("Cross-PC room signals"));
    await user.click(screen.getByText("Cross-PC room signals"));
    expect((screen.getByLabelText("Diagnostic logging")).getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText("Save needs attention")).toBeTruthy();
    await user.click(screen.getByText("Save changes"));
    await waitFor(() => expect(vi.mocked(invokeCommand).mock.calls.filter(([name]) => name === "vision_save_config")).toHaveLength(2));
  });
});
