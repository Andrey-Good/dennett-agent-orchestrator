import type { IncomingHttpHeaders } from "node:http";

const LOOPBACK_HOST_RE = /^(?:127\.0\.0\.1|localhost|\[::1\])(?::(\d{1,5}))?$/i;
const LOOPBACK_ORIGIN_RE = /^https?:\/\/(?:127\.0\.0\.1|localhost|\[::1\])(?::(\d{1,5}))?$/i;

function isValidPort(rawPort: string | undefined): boolean {
  if (!rawPort) {
    return true;
  }
  const num = Number(rawPort);
  return Number.isInteger(num) && num >= 1 && num <= 65535;
}

/**
 * D2-SEC-03: Strict loopback Host/Origin + custom header validation for
 * `/__dennett_dev_dirs` so arbitrary websites cannot enumerate the local
 * filesystem via cross-site fetches or DNS rebinding while Vite dev runs.
 */
export function isLoopbackHostHeader(host: string | undefined): boolean {
  if (!host) {
    return false;
  }
  const match = LOOPBACK_HOST_RE.exec(host.trim());
  return Boolean(match && isValidPort(match[1]));
}

export function isLoopbackOriginHeader(origin: string | undefined): boolean {
  if (!origin) {
    return true;
  }
  const match = LOOPBACK_ORIGIN_RE.exec(origin.trim());
  return Boolean(match && isValidPort(match[1]));
}

export function validateDevDirsRequest(
  method: string | undefined,
  headers: IncomingHttpHeaders,
): { ok: true } | { ok: false; status: number; message: string } {
  if (method !== "GET") {
    return { ok: false, status: 405, message: "Method Not Allowed" };
  }
  const host = Array.isArray(headers.host) ? headers.host[0] : headers.host;
  if (!isLoopbackHostHeader(host)) {
    return { ok: false, status: 403, message: "Forbidden: non-loopback Host" };
  }
  const pickerHeader = Array.isArray(headers["x-dennett-dev-picker"])
    ? headers["x-dennett-dev-picker"][0]
    : headers["x-dennett-dev-picker"];
  if (pickerHeader !== "1") {
    return { ok: false, status: 403, message: "Forbidden: missing X-Dennett-Dev-Picker header" };
  }
  const fetchSite = Array.isArray(headers["sec-fetch-site"])
    ? headers["sec-fetch-site"][0]
    : headers["sec-fetch-site"];
  if (fetchSite !== "same-origin") {
    return { ok: false, status: 403, message: "Forbidden: Sec-Fetch-Site must be same-origin" };
  }
  const origin = Array.isArray(headers.origin) ? headers.origin[0] : headers.origin;
  if (!isLoopbackOriginHeader(origin)) {
    return { ok: false, status: 403, message: "Forbidden: non-loopback Origin" };
  }
  return { ok: true };
}
