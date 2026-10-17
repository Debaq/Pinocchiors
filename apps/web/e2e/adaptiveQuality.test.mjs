// Calidad adaptativa del visor: escala según el estado (node --experimental-strip-types)
import assert from "node:assert/strict";
import { MOTION_GAP_MS, MOTION_SCALE, isMotionFrame, qualityScale } from "../src/lib/adaptiveQuality.ts";

assert.equal(isMotionFrame(1000, null), false);
assert.equal(isMotionFrame(1000, 1000 - MOTION_GAP_MS + 1), true);
assert.equal(isMotionFrame(1000, 1000 - MOTION_GAP_MS), false);

assert.equal(qualityScale(true, true), MOTION_SCALE);
assert.equal(qualityScale(true, false), 1);
assert.equal(qualityScale(false, true), 1);
assert.equal(qualityScale(true, true, true), 1);
assert.ok(MOTION_SCALE > 0 && MOTION_SCALE < 1);
console.log("adaptiveQuality OK");
