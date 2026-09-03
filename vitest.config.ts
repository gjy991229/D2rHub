import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.ui.test.tsx"],
    clearMocks: true,
    restoreMocks: true,
  },
});
