import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
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
