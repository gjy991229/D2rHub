import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { RoomCollaborationMode } from "./RoomCollaborationMode";
afterEach(cleanup);
it("explains all four modes on hover and selects independent creation", () => {
  const onChange = vi.fn();
  render(<RoomCollaborationMode compact onChange={onChange} />);
  const modes = screen.getAllByRole("radio");
  expect(modes).toHaveLength(4);
  for (const mode of modes) expect(mode.closest("label")?.title).toBeTruthy();
  fireEvent.click(screen.getByRole("radio", { name: /超级迪亚波罗/ }));
  expect(onChange).toHaveBeenCalledWith("uber_diablo");
  expect(screen.getByRole("radio", { name: /超级迪亚波罗/ }).closest("label")?.title).toContain("密码相同");
});
