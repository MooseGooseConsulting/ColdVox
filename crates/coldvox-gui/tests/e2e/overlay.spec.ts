import { expect, test } from "@playwright/test";

// These specs run against the Vite dev server only (no Tauri host process).
// The hook can't reach the Rust backend in that environment, so it falls into
// the error branch of `useOverlayShell` and renders the expanded shell with an
// error badge. The smoke verifies that the React layer mounts and the error
// path surfaces correctly.

test("overlay mounts and renders the expanded shell when no Tauri host is present", async ({
  page,
}) => {
  await page.goto("/");

  await expect(page.getByRole("heading", { name: "ColdVox", level: 1 })).toBeVisible();
  await expect(page.getByTestId("final-transcript")).toBeVisible();
  await expect(page.getByTestId("partial-transcript")).toBeVisible();
});

test("overlay surfaces an error badge when the host bridge is unreachable", async ({
  page,
}) => {
  await page.goto("/");

  await expect(page.getByRole("alert")).toBeVisible();
});
