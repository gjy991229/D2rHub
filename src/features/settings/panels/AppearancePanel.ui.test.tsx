import { useState } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { AppearancePanel, type AppearanceSettingsDraft } from "./AppearancePanel";

afterEach(cleanup);

function Harness({ english = false, dirty = true }: { english?: boolean; dirty?: boolean }) {
  const [draft, setDraft] = useState<AppearanceSettingsDraft>({ app_language: english ? "en-US" : "zh-CN", theme: "light", main_opacity: 96,
    font_scale: "default", separate_game_taskbar_icons: false });
  return <AppearancePanel draft={draft} dirty={dirty} applying={false} navigationSaving={false}
    onApply={async () => true} onChange={patch => setDraft(current => ({ ...current, ...patch }))} />;
}

it("allows clearing and replacing opacity without clamping each keystroke", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  const input = screen.getByRole("spinbutton", { name: "主界面背景不透明度百分比" }) as HTMLInputElement;
  await user.clear(input);
  await user.type(input, "85");
  expect(input.value).toBe("85");
  await user.tab();
  expect(input.value).toBe("85");
  expect((screen.getByRole("slider", { name: "主界面背景不透明度" }) as HTMLInputElement).value).toBe("91");
});

it("renders complete English appearance status, guidance, and actions locally", () => {
  render(<Harness english dirty={false} />);
  expect(screen.getByText("Appearance saved and applied")).toBeTruthy();
  expect(screen.getByText("The theme, opacity, and text size use your saved settings.")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Apply appearance" })).toBeTruthy();
  expect(screen.getByRole("heading", { name: "Interface language" })).toBeTruthy();
  expect(screen.getByRole("heading", { name: "Main window theme" })).toBeTruthy();
  expect(screen.getByRole("button", { name: /Dark Black surfaces/ })).toBeTruthy();
  expect(screen.getByRole("spinbutton", { name: "Main window background opacity percentage" })).toBeTruthy();
});

it("preserves nonlinear slider mapping and restores incomplete opacity edits with Escape", async () => {
  const user = userEvent.setup();
  render(<Harness />);
  const input = screen.getByRole("spinbutton", { name: "主界面背景不透明度百分比" }) as HTMLInputElement;
  const slider = screen.getByRole("slider", { name: "主界面背景不透明度" });
  expect((slider as HTMLInputElement).value).toBe("98");
  fireEvent.change(slider, { target: { value: "50" } });
  expect(input.value).toBe("33");
  await user.clear(input);
  await user.keyboard("{Escape}{Tab}");
  expect(input.value).toBe("33");
});
