import test from "node:test";
import assert from "node:assert/strict";
import { CLOSED_REASONS, DispatchPolicy } from "../core/dispatch-policy.mjs";

const healthyItem = {
  id: "job-1",
  model: "veo_3_1_fast",
  creditsPerGeneration: 10,
  generationCount: 1,
  authorized: true,
};

function readyPolicy(options) {
  const policy = new DispatchPolicy(options);
  policy.observeSession({ ready: true });
  policy.observeCredits(50_000);
  policy.observeModelStatus("veo_3_1_fast", "HEALTHY");
  return policy;
}

test("credit balance does not raise concurrency above the conservative default", () => {
  const policy = readyPolicy();
  assert.equal(policy.reserveAndDispatch(healthyItem).allowed, true);

  const second = policy.evaluate({ ...healthyItem, id: "job-2" });
  assert.equal(second.allowed, false);
  assert.equal(second.reason, CLOSED_REASONS.CAPACITY_REACHED);
  assert.equal(policy.validatedMaxInFlight, 1);
});

test("unknown cost blocks dispatch", () => {
  const policy = readyPolicy();
  const decision = policy.evaluate({ ...healthyItem, creditsPerGeneration: undefined });
  assert.equal(decision.allowed, false);
  assert.equal(decision.reason, CLOSED_REASONS.COST_UNKNOWN);
});

test("a request reserves the cost of every generated result", () => {
  const policy = readyPolicy();
  const decision = policy.reserveAndDispatch({
    ...healthyItem,
    generationCount: 2,
  });

  assert.equal(decision.requiredCredits, 20);
  assert.equal(policy.reservedCredits(), 20);
});

test("rate limit closes dispatch until Flow is observed ready again", () => {
  let time = 1_000;
  const policy = readyPolicy({ now: () => time });
  policy.observeRateLimit({ retryAfterMs: 5_000 });

  assert.equal(policy.evaluate(healthyItem).reason, CLOSED_REASONS.COOLDOWN_ACTIVE);
  time = 7_000;
  assert.equal(policy.evaluate(healthyItem).allowed, true);

  policy.observeRateLimit();
  time = 100_000;
  assert.equal(policy.evaluate(healthyItem).reason, CLOSED_REASONS.COOLDOWN_ACTIVE);
  policy.observeReadyAfterRateLimit();
  assert.equal(policy.evaluate(healthyItem).allowed, true);
});

test("settling a failed generation can release reservation without charging credits", () => {
  const policy = readyPolicy();
  policy.reserveAndDispatch(healthyItem);
  policy.settle(healthyItem.id, { chargedCredits: 0 });

  assert.equal(policy.inFlight.size, 0);
  assert.equal(policy.reservedCredits(), 0);
  assert.equal(policy.availableCredits, 50_000);
});
