import { describe, expect, it } from "vitest";
import { formatTime, remainingTime } from "../../src/messages/countdown";

describe("approval budget display", () => {
  it("does not spend time while paused or make a negative budget", () => {
    expect(remainingTime(600000, false, 900000)).toBe(600000);
    expect(remainingTime(600000, true, 1000)).toBe(599000);
    expect(remainingTime(500, true, 1000)).toBe(0);
    expect(remainingTime(1000, true, -100)).toBe(1000);
  });
  it("rounds remaining seconds up without changing the stored budget", () => {
    expect(formatTime(600000)).toBe("10:00");
    expect(formatTime(599001)).toBe("10:00");
    expect(formatTime(1)).toBe("0:01");
    expect(formatTime(0)).toBe("0:00");
  });
});
