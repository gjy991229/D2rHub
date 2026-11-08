import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Modal } from "./Modal";

function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>Open settings</button>
      <Modal open={open} onClose={() => setOpen(false)} title="Settings">
        <button type="button">First action</button>
        <button type="button">Last action</button>
      </Modal>
    </>
  );
}

afterEach(cleanup);

describe("Modal focus management", () => {
  it("renders outside the app shell so its fixed positioning cannot be overridden or clipped", () => {
    render(<div className="app-shell"><Modal open onClose={() => {}} title="Account">Account</Modal></div>);
    const dialog = screen.getByRole("dialog", { name: "Account" });
    expect(dialog.parentElement).toBe(document.body);
    expect(dialog.closest(".app-shell")).toBeNull();
  });

  it("sends Escape only to the top dialog", () => {
    const closeSettings = vi.fn();
    const closeAccount = vi.fn();
    const { rerender } = render(<Modal open onClose={closeSettings} title="Settings">Settings</Modal>);
    rerender(<>
      <Modal open onClose={closeSettings} title="Settings">Settings</Modal>
      <Modal open onClose={closeAccount} title="Add account">Account</Modal>
    </>);

    fireEvent.keyDown(window, { key: "Escape" });
    expect(closeAccount).toHaveBeenCalledTimes(1);
    expect(closeSettings).not.toHaveBeenCalled();
  });

  it("skips hidden sections when wrapping keyboard focus", () => {
    render(<Modal open onClose={() => {}} title="Settings">
      <button type="button">Visible action</button>
      <div hidden><button type="button">Hidden action</button></div>
    </Modal>);
    screen.getByRole("button", { name: "Visible action" }).focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "关闭对话框" }));
  });

  it("includes a disclosure summary while skipping collapsed content and hidden ancestors", () => {
    render(<Modal open onClose={() => {}} title="Guide">
      <details><summary>Show guide</summary><button type="button">Guide action</button></details>
      <div style={{ display: "none" }}><button type="button">Hidden action</button></div>
    </Modal>);
    const summary = screen.getByText("Show guide");
    summary.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "关闭对话框" }));
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(summary);
  });

  it("traps keyboard focus and restores it to the opener", async () => {
    render(<Harness />);
    const opener = screen.getByRole("button", { name: "Open settings" });
    opener.focus();
    fireEvent.click(opener);

    const dialog = screen.getByRole("dialog", { name: "Settings" });
    const content = dialog.querySelector<HTMLElement>("[tabindex='-1']");
    await waitFor(() => expect(document.activeElement).toBe(content));

    const close = screen.getByRole("button", { name: "关闭对话框" });
    const last = screen.getByRole("button", { name: "Last action" });
    last.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(close);

    fireEvent.click(close);
    await waitFor(() => expect(document.activeElement).toBe(opener));
  });
});
