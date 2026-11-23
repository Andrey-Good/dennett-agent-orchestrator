import type { RuntimeQuotaWindow } from "../api/contracts";

export interface QuotaRingsProps {
  quotas: readonly RuntimeQuotaWindow[];
}

const RING_RADIUS = 10;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

export function QuotaRings({ quotas }: QuotaRingsProps) {
  if (!quotas || quotas.length === 0) {
    return null;
  }

  const visible = quotas.slice(0, 2);

  return (
    <div className="quota-rings" data-testid="quota-rings" aria-label="Остаток лимитов">
      {visible.map((window) => {
        const clamped = Math.max(0, Math.min(100, Math.round(window.remaining_percent)));
        const offset = RING_CIRCUMFERENCE * (1 - clamped / 100);
        const isLow = clamped <= 15;

        return (
          <div
            key={window.label}
            className="quota-ring-item"
            title={`${window.label}: ${clamped}`}
            data-testid={`quota-ring-${window.label}`}
          >
            <span className="quota-ring-label">{window.label}</span>
            <div className="quota-ring-graphic">
              <svg
                width="26"
                height="26"
                viewBox="0 0 26 26"
                className="quota-ring-svg"
                aria-hidden="true"
              >
                <circle
                  cx="13"
                  cy="13"
                  r={RING_RADIUS}
                  fill="none"
                  stroke="rgba(255, 255, 255, 0.12)"
                  strokeWidth="2.2"
                />
                <circle
                  cx="13"
                  cy="13"
                  r={RING_RADIUS}
                  fill="none"
                  stroke={isLow ? "#f3c677" : "rgba(255, 255, 255, 0.88)"}
                  strokeWidth="2.2"
                  strokeDasharray={RING_CIRCUMFERENCE}
                  strokeDashoffset={offset}
                  strokeLinecap="round"
                  transform="rotate(-90 13 13)"
                />
              </svg>
              <span className="quota-ring-value">{clamped}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
