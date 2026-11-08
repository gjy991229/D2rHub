import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Input } from "./Input";

afterEach(cleanup);

describe("Input labels and validation", () => {
  it("keeps repeated labels associated with their own fields", () => {
    render(<><Input label="昵称" /><Input label="昵称" /></>);
    const fields = screen.getAllByRole("textbox", { name: "昵称" });
    expect(fields[0].id).not.toBe(fields[1].id);
    expect(screen.getAllByLabelText("昵称")).toHaveLength(2);
  });

  it("preserves help text while exposing its validation error", () => {
    render(<>
      <p id="nickname-help">仅保存在本机</p>
      <Input id="nickname" label="昵称" error="请输入昵称" aria-describedby="nickname-help" />
    </>);
    const field = screen.getByRole("textbox", { name: "昵称" });
    expect(field.getAttribute("aria-invalid")).toBe("true");
    expect(field.getAttribute("aria-describedby")).toBe("nickname-help nickname-error");
    expect(document.getElementById("nickname-error")?.textContent).toBe("请输入昵称");
  });
});
