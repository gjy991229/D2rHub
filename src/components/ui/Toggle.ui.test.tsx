import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Toggle } from "./Toggle";

afterEach(cleanup);

describe("Toggle", () => {
  it("exposes switch semantics and a 24 by 40 pixel minimum control", () => {
    const onChange = vi.fn();
    render(<Toggle checked={false} ariaLabel="Test toggle" onChange={onChange} />);

    const toggle = screen.getByRole("switch", { name: "Test toggle" });
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(toggle.className).toContain("h-[24px]");
    expect(toggle.className).toContain("w-[40px]");

    fireEvent.click(toggle);
    expect(onChange).toHaveBeenCalledWith(true);
  });
});
