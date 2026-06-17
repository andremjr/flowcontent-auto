const CLOSED_REASONS = Object.freeze({
  SESSION_NOT_READY: "SESSION_NOT_READY",
  CAPABILITY_UNKNOWN: "CAPABILITY_UNKNOWN",
  MODEL_UNAVAILABLE: "MODEL_UNAVAILABLE",
  COST_UNKNOWN: "COST_UNKNOWN",
  CREDITS_UNKNOWN: "CREDITS_UNKNOWN",
  INSUFFICIENT_CREDITS: "INSUFFICIENT_CREDITS",
  COOLDOWN_ACTIVE: "COOLDOWN_ACTIVE",
  CAPACITY_REACHED: "CAPACITY_REACHED",
  NOT_AUTHORIZED: "NOT_AUTHORIZED",
});

export class DispatchPolicy {
  constructor({ now = () => Date.now() } = {}) {
    this.now = now;
    this.sessionReady = false;
    this.availableCredits = null;
    this.modelStatuses = new Map();
    this.inFlight = new Map();
    this.reservations = new Map();
    this.cooldownUntil = null;

    // Conservative default. This value is never raised from credit balance or
    // from probing rate limits.
    this.validatedMaxInFlight = 1;
  }

  observeSession({ ready }) {
    this.sessionReady = Boolean(ready);
  }

  observeCredits(credits) {
    if (!Number.isFinite(credits) || credits < 0) {
      throw new TypeError("credits must be a non-negative finite number");
    }
    this.availableCredits = credits;
  }

  observeModelStatus(model, status) {
    this.modelStatuses.set(model, status);
  }

  observeRateLimit({ retryAfterMs } = {}) {
    this.validatedMaxInFlight = 1;
    this.cooldownUntil = Number.isFinite(retryAfterMs)
      ? this.now() + Math.max(0, retryAfterMs)
      : Number.POSITIVE_INFINITY;
  }

  observeReadyAfterRateLimit() {
    this.cooldownUntil = null;
  }

  evaluate(item) {
    let requiredCredits;
    try {
      requiredCredits = this.requiredCredits(item);
    } catch {
      return this.closed(CLOSED_REASONS.COST_UNKNOWN);
    }
    const reservedCredits = this.reservedCredits();

    if (!this.sessionReady) return this.closed(CLOSED_REASONS.SESSION_NOT_READY);
    if (!this.modelStatuses.has(item.model)) return this.closed(CLOSED_REASONS.CAPABILITY_UNKNOWN);
    if (this.modelStatuses.get(item.model) !== "HEALTHY") {
      return this.closed(CLOSED_REASONS.MODEL_UNAVAILABLE);
    }
    if (this.availableCredits === null) return this.closed(CLOSED_REASONS.CREDITS_UNKNOWN);
    if (this.cooldownUntil !== null && this.now() < this.cooldownUntil) {
      return this.closed(CLOSED_REASONS.COOLDOWN_ACTIVE, { cooldownUntil: this.cooldownUntil });
    }
    if (this.inFlight.size >= this.validatedMaxInFlight) {
      return this.closed(CLOSED_REASONS.CAPACITY_REACHED);
    }
    if (!item.authorized) return this.closed(CLOSED_REASONS.NOT_AUTHORIZED);
    if (requiredCredits > this.availableCredits - reservedCredits) {
      return this.closed(CLOSED_REASONS.INSUFFICIENT_CREDITS, {
        availableUnreservedCredits: this.availableCredits - reservedCredits,
        requiredCredits,
      });
    }

    return {
      allowed: true,
      requiredCredits,
      availableUnreservedCredits: this.availableCredits - reservedCredits,
    };
  }

  reserveAndDispatch(item) {
    const decision = this.evaluate(item);
    if (!decision.allowed) return decision;

    this.reservations.set(item.id, decision.requiredCredits);
    this.inFlight.set(item.id, {
      model: item.model,
      submittedAt: this.now(),
    });
    return { ...decision, dispatched: true };
  }

  settle(itemId, { chargedCredits = 0 } = {}) {
    if (!Number.isFinite(chargedCredits) || chargedCredits < 0) {
      throw new TypeError("chargedCredits must be a non-negative finite number");
    }
    this.inFlight.delete(itemId);
    this.reservations.delete(itemId);
    if (this.availableCredits !== null) {
      this.availableCredits = Math.max(0, this.availableCredits - chargedCredits);
    }
  }

  requiredCredits(item) {
    const generationCount = item.generationCount ?? 1;
    if (!Number.isInteger(generationCount) || generationCount < 1) {
      throw new TypeError("generationCount must be a positive integer");
    }
    if (!Number.isFinite(item.creditsPerGeneration) || item.creditsPerGeneration < 0) {
      throw new TypeError("creditsPerGeneration must be a non-negative finite number");
    }
    return generationCount * item.creditsPerGeneration;
  }

  reservedCredits() {
    return [...this.reservations.values()].reduce((total, credits) => total + credits, 0);
  }

  closed(reason, details = {}) {
    return { allowed: false, reason, ...details };
  }
}

export { CLOSED_REASONS };
