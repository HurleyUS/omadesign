import { describe, expect, it, vi } from "vitest";
vi.mock("./release", () => ({ NIGHTLY_VERSION: "0.6.3-nightly.20261011.0002" }));
import { latestUpdate, updateBySlug } from "./updates";
import { NIGHTLY_VERSION } from "./release";

describe("Release notes", () => {
  it("serves the app's dated nightly link and keeps the stable update current", () => {
    expect(updateBySlug(NIGHTLY_VERSION)?.title).toBe("Brush packs");
    expect(updateBySlug(NIGHTLY_VERSION)?.version).toBe(NIGHTLY_VERSION);
    expect(updateBySlug("0.6.3-nightly.20261011.0001")?.title).toBe("Brush packs");
    expect(latestUpdate.version).toBe("0.6.3");
  });
});
