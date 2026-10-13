import { describe, expect, it } from "vitest";
import { latestUpdate, updateBySlug } from "./updates";
import { NIGHTLY_VERSION, RELEASE_VERSION } from "./release";

describe("Release notes", () => {
  it("features the published release and preserves earlier notes", () => {
    expect(latestUpdate.version).toBe(RELEASE_VERSION);
    expect(updateBySlug(NIGHTLY_VERSION)?.title).toBe("Brushes that follow your hand");
    expect(updateBySlug("0.6.3-nightly.20261011.0001")?.title).toBe("Brush packs");
    expect(updateBySlug("0.6.3")?.title).toBe("A workspace that moves with you");
  });
  it("does not invent notes for an unpublished nightly", () => {
    expect(updateBySlug("0.6.3-nightly.20990101.0001")).toBeUndefined();
  });
});
