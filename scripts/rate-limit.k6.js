import http from "k6/http";
import { check, sleep } from "k6";
import { Counter } from "k6/metrics";

const baseUrl = __ENV.RUSTFLOW_RATE_LIMIT_BASE_URL || "http://127.0.0.1:3002";
const routePath = __ENV.RUSTFLOW_RATE_LIMIT_PATH || "/limited";
const rejectedRequests = new Counter("rate_limit_rejected");

http.setResponseCallback(http.expectedStatuses(200, 429));

function responseHeader(response, name) {
  const headerName = Object.keys(response.headers).find(
    (candidate) => candidate.toLowerCase() === name.toLowerCase(),
  );
  return headerName === undefined ? undefined : response.headers[headerName];
}

export const options = {
  vus: 10,
  duration: "15s",
  thresholds: {
    http_req_failed: ["rate<0.01"],
    http_req_duration: ["p(95)<100"],
    checks: ["rate==1"],
    rate_limit_rejected: ["count>0"],
  },
};

export default function () {
  const response = http.get(`${baseUrl}${routePath}`);
  const isAllowed = response.status === 200;
  const isLimited = response.status === 429;
  if (isLimited) {
    rejectedRequests.add(1);
  }

  check(response, {
    "request receives an allowed or rate-limited response": () =>
      isAllowed || isLimited,
    "allowed response includes limit metadata": () =>
      !isAllowed ||
      (responseHeader(response, "RateLimit-Limit") !== undefined &&
        responseHeader(response, "RateLimit-Remaining") !== undefined),
    "limited response includes retry metadata": () =>
      !isLimited ||
      (responseHeader(response, "Retry-After") !== undefined &&
        responseHeader(response, "RateLimit-Reset") !== undefined),
    "limited response is not cacheable": () =>
      !isLimited || responseHeader(response, "Cache-Control") === "no-store",
  });

  sleep(0.01);
}
